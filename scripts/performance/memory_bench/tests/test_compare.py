"""BT10 comparison, verdict, report and Markdown over synthetic run directories."""
import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
import unittest

from ..application import controls, markdown, report as reports, verdict
from ..application.compare import ComparisonRefused, Pairing, delta, drift, require_comparable
from ..application.run_data import load_run
from ..application.score import ScoringRefused, score_run
from ..application import tokens as tokens_module
from ..application.tokens import unavailable
from ..domain import cachekey
from ..domain.variant import parse_variant_toml
from .run_fixture import WordCounter, answer, facet, make_run, question


class LetterCounter(WordCounter):
    """Words plus the letter `a`: a count that moves with the hex digits of a handle."""

    def count(self, text):
        return super().count(text) + text.count('a')

N = 40
QUESTIONS = tuple(question(f'q{i:02d}', 'singular_anchored', {
    'answerable': 'KNOWN', 'shape': 'singular', 'facets': [facet('fact', [i])]}, anchors=[f'C{i}'])
    for i in range(N))
NEGATIVES = tuple(question(f'n{i:02d}', 'near_miss_attribute', {
    'answerable': 'UNKNOWN', 'shape': 'singular', 'facets': [facet('attr')],
    'unknown_reason': 'attribute_not_found', 'absent_terms': ['Kubernetes']}, anchors=[f'C{i}'])
    for i in range(10))
ALL = QUESTIONS + NEGATIVES
COUNTERS = (WordCounter('o200k_base'), WordCounter('cl100k_base'))


def variant(claim='quality', effect=0.3, guards=('false_answer_rate',), name='candidate'):
    if claim == 'parity':
        body = 'targets = ["parity_rate"]'
    else:
        body = f'targets = [{{ metric = "useful_rate", direction = "up", effect = {effect} }}]'
    guard_list = ', '.join(f'"{g}"' for g in guards)
    return parse_variant_toml(f'name = "{name}"\nclaim = "{claim}"\n{body}\nguards = [{guard_list}]\n'
                              f'n = {N}\nstore = "shared"\njev = "off"\n[binary]\npath = "target/release/kmp-mcp"\n')


def baseline_pages(q, sample=0):
    """Right on even questions, UNKNOWN on odd ones; abstains on every negative."""
    if q.id.startswith('n'):
        return [answer(unknown=True, evidence=[], missing=['any stored memory for: x'])]
    i = int(q.id[1:])
    return [answer([i])] if i % 2 == 0 else [answer(unknown=True, evidence=[], missing=['any stored memory for: x'])]


def better_pages(q, sample=0):
    """Right on every positive, with a longer proof; still abstains on negatives."""
    if q.id.startswith('n'):
        return baseline_pages(q)
    i = int(q.id[1:])
    return [answer([i], evidence=[i, 100, 101, 102])]


def leaky_pages(q, sample=0):
    """Right on every positive, but answers the negatives."""
    if q.id.startswith('n'):
        return [answer([200])]
    return better_pages(q)


def arm(name, run, variant_value=None):
    return reports.Arm(name, (reports.ArmRun(run, 1000, 'mono'),), variant_value)


def options(**extra):
    return reports.ReportOptions(mode='test', bootstrap_b=200, **extra)


class CompareTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name

    def tearDown(self):
        self.tmp.cleanup()

    def run_dir(self, name, pages, **kwargs):
        kwargs.setdefault('binary', ('a' if name.startswith('base') else 'b') * 64)
        return load_run(make_run(self.root, kwargs.pop('questions', ALL), pages, name=name, **kwargs))

    def report(self, base, cand, variant_value=None, counters=COUNTERS, **extra):
        candidate = None if cand is None else arm('candidate', cand, variant_value)
        return reports.build_report(arm('baseline', base), candidate, ALL, counters, options(**extra))

    # --- A/A and parity -----------------------------------------------------------------

    def test_aa_has_zero_quality_and_token_deltas(self):
        base = self.run_dir('base', baseline_pages)
        again = self.run_dir('base-again', baseline_pages, binary='a' * 64)
        report = self.report(base, again, variant('parity', guards=()))
        aa = report['controls']['aa']
        self.assertTrue(aa['holds'])
        self.assertTrue(aa['quality_delta_zero'])
        self.assertTrue(aa['token_delta_zero'])
        self.assertEqual(aa['deltas']['useful_rate'], 0.0)
        self.assertEqual(aa['deltas']['tokens_journey'], 0.0)
        self.assertEqual(report['controls']['parity']['parity_rate'], 1.0)
        self.assertEqual(report['verdict']['value'], 'neutral')
        self.assertEqual(report['verdict']['deltas']['tokens_journey']['delta'], 0.0)

    def test_aa_normalizes_the_random_continuation_handles(self):
        """Two fresh processes mint different `read_<32 hex>` handles: the raw token count may
        move, the normalized one may not, and only the normalized one decides `holds`."""
        def paged(handle):
            def pages(q, sample=0):
                return [{**page, 'continuation': handle} for page in baseline_pages(q, sample)]
            return pages
        base = self.run_dir('base', paged('read_' + 'a' * 32))
        again = self.run_dir('base-again', paged('read_' + 'b' * 32), binary='a' * 64)
        counters = (LetterCounter('o200k_base'), LetterCounter('cl100k_base'))
        aa = self.report(base, again, variant('parity', guards=()), counters=counters)['controls']['aa']
        self.assertFalse(aa['token_delta_zero'])
        self.assertEqual(aa['raw_moved'], ['tokens_first_page', 'tokens_journey'])
        self.assertTrue(aa['token_delta_zero_normalized'])
        self.assertEqual(aa['deltas']['tokens_journey_normalized'], 0.0)
        self.assertEqual(aa['moved'], [])
        self.assertTrue(aa['holds'])

    def test_normalizing_keeps_length_and_touches_only_handles(self):
        raw = b'{"continuation":"read_' + b'f' * 32 + b'","x":"read_' + b'f' * 33 + b'","y":"xread_' + b'1' * 32 + b'"}'
        fixed = tokens_module.normalize_handles(raw)
        self.assertEqual(len(fixed), len(raw))
        self.assertIn(b'"read_' + b'0' * 32 + b'"', fixed)
        self.assertIn(b'read_' + b'f' * 33, fixed)  # not a handle: 33 hex digits
        self.assertIn(b'xread_' + b'1' * 32, fixed)  # not a handle: glued to a word

    def test_parity_claim_broken_is_a_regression(self):
        base = self.run_dir('base', baseline_pages)
        cand = self.run_dir('cand', better_pages)
        report = self.report(base, cand, variant('parity', guards=()))
        self.assertLess(report['controls']['parity']['parity_rate'], 1.0)
        self.assertEqual(report['verdict']['value'], 'regresion')

    # --- anti-drift ---------------------------------------------------------------------

    def test_drift_in_driver_or_encoders_is_no_comparable(self):
        base = self.run_dir('base', baseline_pages)
        other_driver = self.run_dir('cand-driver', baseline_pages, driver='kmp.native_driver.v2')
        other_encoders = self.run_dir('cand-enc', baseline_pages,
                                      encoders=[{'encoding': 'o200k_base', 'asset_sha256': '0' * 64}])
        self.assertEqual(drift(base, other_driver), ('driver_version',))
        self.assertEqual(drift(base, other_encoders), ('encoders',))
        with self.assertRaises(ComparisonRefused):
            require_comparable(base, other_driver)
        report = self.report(base, other_driver, variant())
        self.assertEqual(report['verdict']['value'], 'no_comparable')
        self.assertEqual(report['provenance']['drift'], ['driver_version'])
        self.assertFalse(report['provenance']['comparable'])
        self.assertEqual(report['by_type']['singular_anchored']['deltas'], {})

    def test_other_question_set_is_refused(self):
        base = self.run_dir('base', baseline_pages)
        other = self.run_dir('cand-q', baseline_pages, questions=QUESTIONS)
        self.assertEqual(drift(base, other), ('questions_digest',))
        with self.assertRaises(ScoringRefused):
            self.report(base, other, variant())

    # --- verdicts -----------------------------------------------------------------------

    def test_underpowered_target_is_indecidible(self):
        base = self.run_dir('base', baseline_pages)
        cand = self.run_dir('cand', better_pages)
        report = self.report(base, cand, variant(effect=0.05), counters=())
        target = report['verdict']['targets'][0]
        self.assertGreater(target['mde'], target['effect'])
        self.assertFalse(target['decidable'])
        self.assertEqual(report['verdict']['value'], 'indecidible')
        self.assertIn('MDE above', report['verdict']['reasons'][0])

    def test_improvement_without_cost_is_mejora(self):
        base = self.run_dir('base', baseline_pages)
        cand = self.run_dir('cand', better_pages)
        report = self.report(base, cand, variant(effect=0.35), counters=())
        target = report['verdict']['targets'][0]
        self.assertEqual((target['delta'], target['status'], target['decidable']), (0.5, 1, True))
        self.assertEqual(target['discordant'], {'improved': 20, 'worsened': 0, 'rate': 0.5})
        self.assertLess(target['p_value'], 0.001)
        self.assertEqual(report['verdict']['value'], 'mejora')

    def test_improvement_with_more_tokens_is_mejora_con_coste(self):
        base = self.run_dir('base', baseline_pages)
        cand = self.run_dir('cand', better_pages)
        report = self.report(base, cand, variant(effect=0.35))
        self.assertTrue(report['verdict']['cost']['worse'])
        self.assertEqual(report['verdict']['value'], 'mejora_con_coste')

    def test_broken_guard_is_a_regression(self):
        base = self.run_dir('base', baseline_pages)
        cand = self.run_dir('cand', leaky_pages)
        report = self.report(base, cand, variant(effect=0.35), counters=())
        guard = report['verdict']['guards'][0]
        self.assertEqual((guard['metric'], guard['broken'], guard['delta']), ('false_answer_rate', True, 1.0))
        self.assertEqual(report['verdict']['value'], 'regresion')

    def test_failed_capture(self):
        base = self.run_dir('base', baseline_pages)
        cand = self.run_dir('cand', better_pages, status=lambda q: 'transport_error' if q.id == 'q03' else None)
        report = self.report(base, cand, variant(effect=0.35), counters=())
        self.assertEqual(report['verdict']['value'], 'captura_fallida')
        self.assertIn('transport_error', report['verdict']['reasons'][0])

    def test_single_arm(self):
        report = self.report(self.run_dir('base', baseline_pages), None)
        self.assertEqual(report['verdict']['value'], 'no_comparable')
        self.assertIsNone(report['provenance']['candidate'])
        useful = next(row for row in report['power'] if row['metric'] == 'useful_rate')
        self.assertAlmostEqual(useful['mde_at_reference_d']['0.08'], 2.8016 * (0.08 / N) ** 0.5, places=3)

    def test_decide_rules_directly(self):
        row = {'metric': 'useful_rate', 'delta': 0.1, 'ci95': [0.05, 0.15], 'mde': 0.2, 'effect': 0.1,
               'decidable': False, 'baseline': None, 'candidate': None}
        self.assertEqual(verdict.decide('quality', [verdict.target_row(row)])['value'], 'indecidible')
        worse = {**row, 'delta': -0.1, 'ci95': [-0.15, -0.05]}
        self.assertEqual(verdict.decide('quality', [verdict.target_row(worse)])['value'], 'regresion')
        flat = {**row, 'delta': 0.0, 'ci95': [-0.05, 0.05], 'mde': 0.05, 'decidable': True}
        cheaper = verdict.cost_summary({'metric': 'tokens_journey', 'delta': -50.0, 'ci95': [-60.0, -40.0]}, None)
        self.assertEqual(verdict.decide('quality', [verdict.target_row(flat)], cost=cheaper)['value'], 'solo_coste')
        self.assertEqual(verdict.decide('quality', [verdict.target_row(flat)])['value'], 'neutral')
        self.assertEqual(verdict.decide('quality', [], replay_misses=2)['value'], 'no_comparable')

    def test_delta_of_a_mean_uses_the_paired_bootstrap(self):
        base = score_run(self.run_dir('base', baseline_pages), ALL, unavailable())
        cand = score_run(self.run_dir('cand', better_pages), ALL, unavailable())
        row = delta(Pairing.of(base, cand), 'recall_at_1', b=200)
        self.assertEqual((row['method'], row['delta']), ('paired_bootstrap', 0.5))
        self.assertEqual(len(row['ci95']), 2)

    # --- report and Markdown ------------------------------------------------------------

    def test_report_sections_and_determinism(self):
        base = self.run_dir('base', baseline_pages)
        cand = self.run_dir('cand', better_pages)
        first = self.report(base, cand, variant(effect=0.35))
        second = self.report(base, cand, variant(effect=0.35))
        self.assertEqual(cachekey.canonical_json(first), cachekey.canonical_json(second))
        self.assertEqual([k for k in first if k in reports_sections()], list(reports_sections()))
        self.assertEqual(first['schema'], 'kmp.bench.report.v1')
        per_question = first['by_type']['singular_anchored']['per_question']['candidate']
        self.assertEqual(len(per_question), N)
        self.assertIn('tokens_per_useful', first['tokens']['arms']['candidate'])

    def test_markdown_comes_from_report_json_only(self):
        base = self.run_dir('base', baseline_pages)
        cand = self.run_dir('cand', better_pages)
        report = self.report(base, cand, variant(effect=0.35))
        text = markdown.render(report)
        stored = json.loads(cachekey.canonical_json(report))
        self.tmp.cleanup()  # the runs are gone: rendering may not need them
        self.assertEqual(markdown.render(stored), text)
        self.assertEqual(markdown.render(copy.deepcopy(stored)), text)
        for number, title in enumerate(markdown.TITLES, start=1):
            self.assertIn(f'## {number}. {title}', text)
        self.assertIn('**mejora_con_coste**', text)
        self.tmp = tempfile.TemporaryDirectory()

    def test_private_runs_keep_aggregates_only(self):
        base = self.run_dir('base', baseline_pages)
        cand = self.run_dir('cand', better_pages)
        summary = controls._public({'calls': 1, 'differences': [{'trace': 'q00~s0~r0'}]}, private=True)
        self.assertNotIn('differences', summary)
        report = self.report(base, cand, variant(effect=0.35))
        self.assertFalse(reports.mentions_private(report))

    def test_repeat_consistency_sets_the_rpc_id_aside(self):
        run = self.run_dir('base', baseline_pages, repeats=2)
        repeat = controls.repeat_consistency(run)
        self.assertEqual(repeat['parity_rate'], 1.0)
        self.assertEqual(repeat['calls'], len(ALL))

    def test_cli_compare_then_render(self):
        from .. import cli
        from ..domain.question import write_questions
        base = make_run(self.root, ALL, baseline_pages, name='base', binary='a' * 64)
        cand = make_run(self.root, ALL, better_pages, name='cand', binary='b' * 64)
        questions = Path(self.root) / 'questions.jsonl'
        write_questions(questions, ALL)
        toml = Path(self.root) / 'candidate.toml'
        toml.write_text('name = "candidate"\nclaim = "quality"\n'
                        'targets = [{ metric = "useful_rate", direction = "up", effect = 0.35 }]\n'
                        'guards = ["false_answer_rate"]\nn = 40\nstore = "shared"\njev = "off"\n'
                        '[binary]\npath = "target/release/kmp-mcp"\n')
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = cli.main(['compare', '--questions', str(questions), '--baseline', str(base),
                             '--candidate', str(cand), '--candidate-variant', str(toml),
                             '--bootstrap-b', '100', '--out', str(Path(self.root) / 'cache')])
        self.assertEqual(code, 0)
        printed = json.loads(out.getvalue())
        # tokens are counted only under tiktoken; then the longer proofs make the cost worse
        self.assertIn(printed['verdict'], ('mejora', 'mejora_con_coste'))
        report_json = Path(printed['report'])
        rendered = report_json.with_name('report.md').read_text(encoding='utf-8')
        report_json.with_name('report.md').unlink()
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(cli.main(['render', '--report', str(report_json)]), 0)
        self.assertEqual(report_json.with_name('report.md').read_text(encoding='utf-8'), rendered)


def reports_sections():
    return ('provenance', 'headlines', 'by_type', 'scale', 'latency_resources', 'tokens', 'jev_by_site',
            'controls', 'power', 'limitations', 'verdict')


if __name__ == '__main__':
    unittest.main()
