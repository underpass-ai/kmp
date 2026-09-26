"""BT10 scoring: synthetic answers of known outcome through a sealed run directory."""
import tempfile
import unittest

from ...token_harness.application.measure import measure_journey
from ...token_harness.domain.representation import RepresentationId
from ..application import aggregate, jev_cost, tokens as tokens_module
from ..application.run_data import load_run
from ..application.score import direction_correct, read_routes, score_run
from ..domain import unknown_reasons
from ..domain.gold import PathGold
from .run_fixture import WordCounter, answer, facet, make_run, question, ref

ENUM = question('q-enum', 'enumerative_anchored', {
    'answerable': 'KNOWN', 'shape': 'enumerative',
    'facets': [facet('decision', [1]), facet('constraint', [2]), facet('status')],
    'wrong_subject': [ref(9)]}, anchors=['C6.4'])
PARTIAL = question('q-partial', 'enumerative_anchored', {
    'answerable': 'KNOWN', 'shape': 'enumerative',
    'facets': [facet('decision', [1]), facet('constraint', [2]), facet('status', [3])]}, anchors=['C6.4'])
MISSED = question('q-missed', 'singular_anchored', {
    'answerable': 'KNOWN', 'shape': 'singular', 'facets': [facet('valve', [3])]}, anchors=['#83'])
NEAR_MISS = question('q-near-miss', 'near_miss_attribute', {
    'answerable': 'UNKNOWN', 'shape': 'singular', 'facets': [facet('namespace', related=[1])],
    'unknown_reason': 'attribute_not_found', 'absent_terms': ['Kubernetes']}, anchors=['#188'])
ABSENT = question('q-absent', 'anchor_absent', {
    'answerable': 'UNKNOWN', 'shape': 'singular', 'facets': [facet('fact')],
    'unknown_reason': 'anchor_not_found', 'absent_terms': ['#288']}, anchors=['#288'])
NOT_THEN = question('q-not-then', 'interval_scoped', {
    'answerable': 'UNKNOWN', 'shape': 'singular', 'facets': [facet('valve', related=[7])],
    'nearest_outside': ref(7), 'unknown_reason': 'not_in_selection'},
    interval={'start': '2026-03-01T00:00:00Z', 'end': '2026-04-01T00:00:00Z'})
AS_OF = question('q-as-of', 'as_of_historical', {
    'answerable': 'KNOWN', 'shape': 'singular', 'facets': [facet('engine', [10], [11])]},
    as_of={'time': '2026-01-02T00:00:00Z'}, source={'future_refs': ref(11)})
PATH = question('q-path', 'path_between', {
    'answerable': 'KNOWN', 'path': {'from': ref(20), 'to': ref(22), 'steps': [ref(20), ref(21), ref(22)],
                                    'required': [ref(21)], 'accept_edges': [], 'accept_alternatives': True,
                                    'directed': False}},
    tool='kmp_curate', arguments={'mode': 'paths', 'from': ref(20), 'to': ref(22)},
    source={'undeclared_edges': f'{ref(21)}>{ref(22)}'})
WAKE = question('q-wake', 'wake_resume', {'answerable': 'KNOWN', 'wake_required': [ref(30), ref(31)]},
                tool='kmp_wake', arguments={'intent': 'resume'})
QUESTIONS = (ENUM, PARTIAL, MISSED, NEAR_MISS, ABSENT, NOT_THEN, AS_OF, PATH, WAKE)


def hop(a, b, declared):
    return {'from': {'ref': f'entry:{ref(a)}'}, 'to': {'ref': ref(b)}, 'rel': 'causes', 'declared': declared}


def pages(q, sample=0):
    return {
        'q-enum': [answer([1, 5])],
        'q-partial': [answer([1, 2], missing=['status of the rollout'], decision='PARTIAL')],
        'q-missed': [answer(unknown=True, evidence=[], missing=['any stored memory for: valve'])],
        'q-near-miss': [answer(unknown=True, evidence=[1], missing=['stored memory that bears on: ns'])],
        'q-absent': [answer([5], confidence='high')],
        'q-not-then': [answer(unknown=True, evidence=[], nearest=7, missing=['stored memory that bears on: x'])],
        'q-as-of': [answer([10, 11], confidence='medium')],
        'q-path': [{'paths': [{'hops': [hop(20, 21, True), hop(21, 22, False)]}], 'projection': {}}],
        'q-wake': [{'wake': {'current_state': [{'ref': f'entry:{ref(30)}'}]}, 'projection': {}},
                   {'proof': {'evidence': [{'id': 'detail:x', 'supports': [ref(31)]}]}, 'projection': {}}],
    }[q.id]


def jev(q):
    if q.tool != 'kmp_ask':
        return []
    return [{'site': 'rerank', 'questions': 3, 'requests': 1, 'input_tokens': 400, 'source': 'cassette_hit',
             'us': 120}]


class ScoreRunTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.data = load_run(make_run(cls.tmp.name, QUESTIONS, pages, jev=jev))
        cls.counters = [WordCounter('o200k_base'), WordCounter('cl100k_base')]
        cls.tokens = tokens_module.measure_run(cls.data, cls.counters)
        cls.scores = {s.question_id: s for s in score_run(cls.data, QUESTIONS, cls.tokens)}

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_answer_and_partial_are_separate(self):
        enum, partial = self.scores['q-enum'], self.scores['q-partial']
        self.assertEqual((enum.outcome, enum.values['answer_correct'], enum.values['partial_useful']),
                         ('answer_correct', True, False))
        self.assertAlmostEqual(enum.values['facet_coverage'], 0.5)
        self.assertEqual((partial.outcome, partial.values['answer_correct'], partial.values['partial_useful']),
                         ('partial_useful', False, True))
        scores = list(self.scores.values())
        # positives: enum and as_of ANSWER correct, partial PARTIAL useful, missed a false UNKNOWN
        self.assertEqual(aggregate.compute(scores, 'answer_correct_rate')['value'], 2 / 4)
        self.assertEqual(aggregate.compute(scores, 'partial_useful_rate')['value'], 1 / 4)
        self.assertEqual(aggregate.compute(scores, 'useful_rate')['value'], 3 / 4)
        self.assertEqual(aggregate.compute(scores, 'useful_rate')['method'], 'wilson')

    def test_false_unknown_both_definitions(self):
        both = aggregate.false_unknown_both(list(self.scores.values()))
        ask = 7  # every kmp_ask question
        self.assertEqual((both['scorecard']['value'], both['scorecard']['n']), (1 / ask, ask))
        self.assertEqual((both['given_known']['value'], both['given_known']['n']), (1 / 4, 4))

    def test_negatives_and_reasons(self):
        near, absent, not_then = self.scores['q-near-miss'], self.scores['q-absent'], self.scores['q-not-then']
        self.assertEqual((near.outcome, near.native_reason, near.reason),
                         ('abstain_correct', 'retrieved_not_bearing', None))
        self.assertEqual(near.values['reason_mapped'], False)
        self.assertNotIn('reason_correct', near.values)
        self.assertEqual((absent.outcome, absent.values['false_answer']), ('false_answer', True))
        self.assertEqual((not_then.reason, not_then.values['reason_correct']), ('not_in_selection', True))
        self.assertTrue(not_then.values['nearest_outside_correct'])
        self.assertEqual(self.scores['q-missed'].reason, 'no_evidence')
        scores = list(self.scores.values())
        self.assertEqual(aggregate.compute(scores, 'false_answer_rate')['value'], 1 / 3)
        self.assertEqual(aggregate.compute(scores, 'reason_accuracy')['value'], 1.0)
        self.assertEqual(aggregate.compute(scores, 'reason_mapped_rate')['value'], 1 / 2)

    def test_future_refs_from_the_generator(self):
        as_of = self.scores['q-as-of']
        self.assertEqual((as_of.values['future_leak'], as_of.values['as_of_correct']), (True, False))

    def test_paths_use_undeclared_edges(self):
        path = self.scores['q-path']
        self.assertTrue(path.values['path_found'])
        self.assertTrue(path.values['path_found_needs_proposal'])
        self.assertNotIn('path_found_declared_reachable', path.values)
        self.assertEqual(path.counts['proposed_hops_right'], (1, 1))
        self.assertEqual(path.counts['undeclared_edge_recovery'], (1, 1))
        self.assertTrue(path.values['direction_correct'])

    def test_wake_obligations_and_load(self):
        wake = self.scores['q-wake']
        self.assertEqual((wake.values['wake_obligations_met'], wake.values['task_ready_after_rpc']), (True, 2.0))
        self.assertEqual(wake.pages, 2)
        load = aggregate.agent_load([wake])
        self.assertEqual(load['pages']['max'], 2)
        row = wake.tokens['o200k_base']
        self.assertEqual(row['to_task_ready'], row['journey'])
        self.assertLess(row['first_page'], row['journey'])

    def test_confidence_levels(self):
        table = aggregate.confidence_table(list(self.scores.values()))
        self.assertEqual(table['by_level']['high']['n'], 3)  # enum, partial and the false answer
        self.assertEqual(table['by_level']['high']['value'], 2 / 3)
        self.assertFalse(table['high_certified'])

    def test_tokens_equal_token_harness_measure(self):
        for trace in self.data.traces:
            measured = measure_journey(trace, self.counters, list(tokens_module.RepresentationId), 1 << 26)
            for encoding in ('o200k_base', 'cl100k_base'):
                expected = measured['totals'][encoding][RepresentationId.JSON_COMPACT_LEXICAL_V1.value]['all']
                self.assertEqual(self.tokens.of(trace.journey, encoding).total, expected['tokens'])
        self.assertGreater(self.tokens.startup['o200k_base'], 0)

    @unittest.skipUnless(tokens_module.try_load_counters()[0], 'needs tiktoken==0.14.0 (uv run --with)')
    def test_tiktoken_counts_equal_token_harness_measure(self):
        counters, _ = tokens_module.try_load_counters()
        counted = tokens_module.measure_run(self.data, counters)
        for trace in self.data.traces:
            measured = measure_journey(trace, counters, list(tokens_module.RepresentationId), 1 << 26)
            for counter in counters:
                encoding = counter.identity.encoding
                expected = measured['totals'][encoding][RepresentationId.JSON_COMPACT_LEXICAL_V1.value]['all']
                self.assertEqual(counted.of(trace.journey, encoding).total, expected['tokens'])

    def test_tokens_per_useful(self):
        scores = list(self.scores.values())
        value, reason = aggregate.tokens_per_useful(scores)
        positives = [s for s in scores if 'useful' in s.values]
        total = sum(s.tokens['o200k_base']['journey'] for s in positives)
        useful = sum(1 for s in positives if s.values['useful'])
        self.assertEqual(useful, 3)  # the as_of answer cites a related (not wrong-subject) entry too
        self.assertEqual((value, reason), (total / useful, None))

    def test_without_tiktoken_tokens_are_null_with_reason(self):
        scores = score_run(self.data, QUESTIONS, tokens_module.unavailable())
        value, reason = aggregate.tokens_per_useful(scores)
        self.assertIsNone(value)
        self.assertEqual(aggregate.compute(scores, 'tokens_journey', reason='not counted')['absent_reason'],
                         'not counted')

    def test_jev_cost_per_site(self):
        table = jev_cost.jev_by_site(self.data)
        rerank = table['sites']['rerank']
        self.assertEqual((rerank['evaluations'], rerank['input_tokens'], rerank['letter']), (7, 2800, 'p'))
        self.assertAlmostEqual(rerank['usd'], 2800 * 0.042 / 1e6)
        self.assertIsNone(rerank['remote_us'])

    def test_jev_cost_is_null_with_reason_without_telemetry(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = load_run(make_run(tmp, QUESTIONS, pages))
            table = jev_cost.jev_by_site(run)
            self.assertIsNone(table['sites'])
            self.assertIn('predates BT03', table['absent_reason'])
            self.assertIsNone(jev_cost.per_question_usd(run))


class ReadingTest(unittest.TestCase):
    def test_unknown_reason_translation(self):
        self.assertEqual(unknown_reasons.translate({'answer': 'x'}), (None, None))
        stated = {'answer': 'UNKNOWN', 'proof': {'unknown_reason': 'anchor_absent'}}
        self.assertEqual(unknown_reasons.translate(stated), ('anchor_absent', 'anchor_not_found'))
        bench = {'answer': 'UNKNOWN', 'reason': 'attribute_not_found'}
        self.assertEqual(unknown_reasons.translate(bench), ('attribute_not_found', 'attribute_not_found'))
        self.assertEqual(unknown_reasons.translate({'answer': 'UNKNOWN', 'proof': {}}), ('unstated', None))

    def test_trace_routes_and_direction(self):
        gold = PathGold(ref(1), ref(3), (ref(1), ref(2), ref(3)), (), (), True, True)
        structured = {'trace': [{'from': ref(1), 'to': ref(2)}, {'from': ref(2), 'to': ref(3)}]}
        [(route, hops)] = read_routes('kmp_trace', structured)
        self.assertEqual(route, [ref(1), ref(2), ref(3)])
        self.assertTrue(all(declared for _, _, declared in hops))
        self.assertTrue(direction_correct(gold, route))
        self.assertFalse(direction_correct(gold, [ref(3), ref(2), ref(1)]))
        self.assertIsNone(direction_correct(gold, [ref(1), ref(9)]))


if __name__ == '__main__':
    unittest.main()
