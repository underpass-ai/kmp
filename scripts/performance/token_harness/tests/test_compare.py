"""Comparator: conclusion table, A/A equality and annex A.17 controls 4, 6 and 7."""
import json
from pathlib import Path
import tempfile
import unittest

from ..application.measure import measure_journey
from ..application.provenance import header
from ..application.verify import verify
from ..compare.conclusion import Conclusion as C, Side, conclude
from ..compare.inputs import ComparisonRejected
from ..compare.pairing import _incompatibility
from ..compare.markdown import render
from ..compare.report import compare_runs
from ..domain.errors import CaptureIntegrityError
from ..domain.representation import RepresentationId
from .fakes import FakeCounter
from .oracle_fixtures import claim_v2, evidence, journey_events, wake_page, write_native_run

REPRESENTATIONS = [RepresentationId.JSON_COMPACT_LEXICAL_V1]


def judged(name, passed=True, selection=None):
    return {'journey': name, 'case_id': 't01', 'goal_id': 'orient_test', 'budget': 4096,
            'selection': selection or {'about': 'fixture:t'}, 'quality_pass': passed,
            'quality_failures': [] if passed else ['task_obligations_satisfied'],
            'completeness': {}, 'obligations': [], 'detail': {},
            'metrics': {'rpc_calls': 1, 'first_supported_unit': {'after_rpc': 1, 'event_index': 4},
                        'task_ready': {'after_rpc': 1, 'event_index': 4}}}


def side(folder, name, journeys, counter=None, passed=True, selection=None):
    run = write_native_run(Path(folder) / name, journeys)
    _, traces, verification = verify(run, 1 << 20)
    counters = [counter or FakeCounter()]
    metrics = {**header(counters, REPRESENTATIONS, verification),
               'journeys': [measure_journey(t, counters, REPRESENTATIONS, 1 << 20) for t in traces]}
    oracle = {'manifest_sha256': verification['manifest_sha256'],
              'journeys': [judged(t.journey, passed, selection) for t in traces]}
    paths = []
    for label, value in (('metrics', metrics), ('oracle', oracle)):
        path = Path(folder) / f'{name}-{label}.json'
        path.write_text(json.dumps(value))
        paths.append(path)
    return (run, *paths)


def journey(extra_state=()):
    return journey_events([wake_page([claim_v2()], [evidence()],
                                     state=['(observation) The backup moved to 02:30.', *extra_state])])


class ConclusionTest(unittest.TestCase):
    def test_every_state_is_reachable_and_a_smaller_failure_is_never_a_saving(self):
        good, bad = Side(True, 100, True), Side(True, 100, False)
        self.assertIs(conclude(good, Side(True, 90, True))[0], C.REDUCTION_WITH_QUALITY_PASS)
        self.assertIs(conclude(good, Side(True, 110, True))[0], C.INCREASE_WITH_QUALITY_PASS)
        self.assertIs(conclude(bad, Side(True, 120, True))[0], C.QUALITY_FIX)
        self.assertIs(conclude(good, Side(True, 10, False))[0], C.QUALITY_REGRESSION)
        self.assertIs(conclude(good, Side(True, 90, None))[0], C.DESCRIPTIVE_ONLY)
        self.assertIs(conclude(good, good)[0], C.DESCRIPTIVE_ONLY)
        self.assertIs(conclude(good, Side(True, 90, True), 'different goals')[0], C.NOT_COMPARABLE)
        self.assertIs(conclude(good, Side(False, None, None))[0], C.CAPTURE_FAILED)

    def test_orientation_against_full_audit_is_not_comparable(self):  # A.17 #6
        orientation = judged('j', selection={'about': 'fixture:t', 'budget': {'detail': 'compact'}})
        audit = judged('j', selection={'about': 'fixture:t', 'budget': {'detail': 'full'}})
        self.assertIsNotNone(_incompatibility(None, audit, orientation))
        self.assertIsNone(_incompatibility(None, audit, dict(audit)))


class CompareRunsTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.base = side(self.folder.name, 'base', {'t01-b4096': journey()}, passed=False)

    def test_a_a_on_identical_artifacts_has_zero_deltas(self):
        report = compare_runs(self.base, self.base, 1 << 20, 'aa')
        self.assertEqual({row['delta_tokens'] for row in report['rows']}, {0})
        self.assertEqual({c['weighted_reduction'] for c in report['cohorts']}, {0.0})
        self.assertEqual({c['memory_weighted_reduction'] for c in report['cohorts']}, {0.0})

    def test_startup_is_reported_per_method_and_excluded_from_memory_change(self):
        cand = side(self.folder.name, 'cand', {'t01-b4096': journey(['(decision) more'])})
        report = compare_runs(self.base, cand, 1 << 20, 'x')
        for name in ('baseline', 'candidate'):
            (startup,) = report[name]['startup']
            self.assertEqual(startup['tokens'], sum(m['tokens'] for m in startup['methods'].values()))
            self.assertEqual(startup['distinct_totals'], [startup['tokens']])
        row, (cohort, *_) = report['rows'][0], report['cohorts']
        self.assertEqual(row['baseline']['startup_tokens'], report['baseline']['startup'][0]['tokens'])
        memory = row['candidate']['memory_tokens'] - row['baseline']['memory_tokens']
        self.assertEqual(memory, row['delta_tokens'])  # same startup: the whole delta is memory
        self.assertAlmostEqual(cohort['memory_weighted_reduction'],
                               -memory / row['baseline']['memory_tokens'])
        rendered = render(report, 'report.json', history='## History\n\nearlier')
        self.assertIn('### Startup per session', rendered)
        self.assertTrue(rendered.rstrip().endswith('earlier'))

    def test_quality_fix_is_reported_with_its_growth(self):
        cand = side(self.folder.name, 'cand', {'t01-b4096': journey(['(decision) more'])})
        row = compare_runs(self.base, cand, 1 << 20, 'x')['rows'][0]
        self.assertGreater(row['delta_tokens'], 0)
        self.assertEqual(row['conclusion'], C.QUALITY_FIX.value)

    def test_mixed_encoders_are_rejected(self):
        cand = side(self.folder.name, 'cand', {'t01-b4096': journey()}, counter=FakeCounter('other'))
        with self.assertRaises(ComparisonRejected):
            compare_runs(self.base, cand, 1 << 20, 'x')

    def test_the_same_capture_under_two_names_is_rejected(self):  # A.17 #4
        twice = side(self.folder.name, 'twice', {'t01-b4096': journey(), 't01-b10000': journey()})
        with self.assertRaises(ComparisonRejected):
            compare_runs(twice, twice, 1 << 20, 'x')

    def test_a_corrupted_original_blocks_the_report(self):  # A.17 #7
        run = self.base[0]
        manifest = json.loads((run / 'manifest.json').read_text())
        manifest['uncompressed_files']['t01-b4096.jsonl']['sha256'] = '0' * 64
        (run / 'manifest.json').write_text(json.dumps(manifest))
        with self.assertRaises((CaptureIntegrityError, ComparisonRejected)):
            compare_runs(self.base, self.base, 1 << 20, 'x')

    def test_metrics_from_another_run_are_rejected(self):
        other = side(self.folder.name, 'other', {'t01-b4096': journey(['x'])})
        mixed = (self.base[0], other[1], self.base[2])
        with self.assertRaises(ComparisonRejected):
            compare_runs(mixed, self.base, 1 << 20, 'x')


if __name__ == '__main__':
    unittest.main()
