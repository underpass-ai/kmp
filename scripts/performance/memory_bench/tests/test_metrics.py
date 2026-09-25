"""Metrics: parity with retrieval_scorecard.rs through the shared fixture, and bench-only metrics."""
import json
import re
from pathlib import Path
import unittest

from ..domain import metrics as m
from ..domain.gold import Facet, PathGold

ROOT = Path(__file__).resolve().parents[4]
PARITY = ROOT / 'crates/kmp-testkit/judged/metric_parity.json'
SCORECARD_RS = ROOT / 'crates/kmp-testkit/src/retrieval_scorecard.rs'


def outcome(case):
    return m.RetrievalOutcome.of(case['judged'], case['retrieved'], case['cited'], case['unknown'],
                                 case['used_bytes'], case['elapsed_millis'])


class ParityFixtureTest(unittest.TestCase):
    """The same file `cargo test -p kmp-testkit --test metric_parity` reads."""

    @classmethod
    def setUpClass(cls):
        cls.fixture = json.loads(PARITY.read_text(encoding='utf-8'))
        cls.tolerance = cls.fixture['tolerance']

    def test_every_case(self):
        self.assertGreaterEqual(len(self.fixture['cases']), 10)
        for case in self.fixture['cases']:
            scored, expected = outcome(case), case['expected']
            with self.subTest(case=case['name']):
                for name, value in (('recall_at_1', scored.recall_at(1)), ('recall_at_5', scored.recall_at(5)),
                                    ('recall_at_10', scored.recall_at(10)),
                                    ('reciprocal_rank', scored.reciprocal_rank()),
                                    ('ndcg_at_10', scored.ndcg_at(10))):
                    self.assertAlmostEqual(value, expected[name], delta=self.tolerance, msg=name)
                self.assertIs(scored.answer_cites_judged(), expected['answer_cites_judged'])
                self.assertIs(scored.is_false_unknown(), expected['is_false_unknown'])
                self.assertIs(scored.has_complete_support_at(5), expected['has_complete_support_at_5'])

    def test_scorecard(self):
        card = m.RetrievalScorecard.score(outcome(case) for case in self.fixture['cases'])
        expected = self.fixture['scorecard']
        self.assertEqual(card.cases, expected['cases'])
        for name, value in expected.items():
            if name != 'cases':
                self.assertAlmostEqual(getattr(card, name), value, delta=self.tolerance, msg=name)

    def test_quality_columns_follow_the_rust_order(self):
        source = SCORECARD_RS.read_text(encoding='utf-8')
        body = source[source.index('pub fn quality_columns'):]
        body = body[:body.index('\n    }\n')]
        self.assertEqual(tuple(re.findall(r'\("([a-z_0-9]+)"', body)), m.QUALITY_COLUMNS)


class PortedByHandTest(unittest.TestCase):
    """Values worked out by hand, independent of the generated fixture."""

    def test_rank_and_gain(self):
        third = m.RetrievalOutcome.of(['a'], ['x', 'y', 'a'])
        self.assertEqual(third.recall_at(1), 0.0)
        self.assertEqual(third.recall_at(5), 1.0)
        self.assertAlmostEqual(third.reciprocal_rank(), 1 / 3)
        self.assertAlmostEqual(third.ndcg_at(10), 0.5)  # 1/log2(4) over an ideal of 1

    def test_rust_quirks_are_kept(self):
        duplicated = m.RetrievalOutcome.of(['a'], ['a', 'a'])
        self.assertEqual(duplicated.recall_at(5), 1.0)
        self.assertGreater(duplicated.ndcg_at(10), 1.0)  # nDCG does not de-duplicate in Rust
        self.assertEqual(m.RetrievalOutcome.of([], ['a']).recall_at(1), 0.0)
        self.assertFalse(m.RetrievalOutcome.of([], []).has_complete_support_at(10))

    def test_citing_is_stricter_than_retrieving(self):
        self.assertFalse(m.RetrievalOutcome.of(['a'], ['a']).answer_cites_judged())
        self.assertTrue(m.RetrievalOutcome.of(['a'], ['a'], ['a']).answer_cites_judged())

    def test_empty_collection_scores_zero(self):
        card = m.RetrievalScorecard.score([])
        self.assertEqual((card.cases, card.recall_at_5, card.ndcg_at_10), (0, 0.0, 0.0))


class RateTest(unittest.TestCase):
    def test_rate(self):
        self.assertEqual(m.Rate.of([True, False, None, True]), m.Rate(2, 3))
        self.assertIsNone(m.Rate(0, 0).value)
        self.assertAlmostEqual(m.Rate(1, 4).value, 0.25)
        with self.assertRaises(ValueError):
            m.Rate(3, 2)
        self.assertIsNone(m.mean_or_none([None]))
        self.assertEqual(m.mean_or_none([1.0, None, 0.0]), 0.5)


class ChainTest(unittest.TestCase):
    def test_full_chain(self):
        self.assertTrue(m.full_chain_recovered_at(['a', 'b'], ['b', 'x', 'a'], 3))
        self.assertFalse(m.full_chain_recovered_at(['a', 'b'], ['b', 'x', 'a'], 2))
        self.assertFalse(m.full_chain_recovered_at([], ['a'], 5))
        rate = m.full_chain_recovery_at_k([(['a', 'b'], ['a', 'b']), (['a', 'c'], ['a'])], 5)
        self.assertEqual(rate, m.Rate(1, 2))


def path(**kw):
    base = dict(start='s', end='t', steps=('s', 'm', 't'), required=(), accept_edges=(),
                accept_alternatives=True, directed=True)
    base.update(kw)
    return PathGold(**base)


class PathTest(unittest.TestCase):
    def test_reference_route_is_found(self):
        self.assertTrue(m.path_found(path(), ['s', 'm', 't']))
        self.assertEqual(m.step_precision(path(), ['s', 'm', 't']), 1.0)

    def test_wrong_ends_and_empty_routes(self):
        self.assertFalse(m.path_found(path(), ['m', 't']))
        self.assertFalse(m.path_found(path(), ['s', 'm']))
        self.assertFalse(m.path_found(path(), ['s']))
        self.assertIsNone(m.step_precision(path(), ['s']))

    def test_alternatives_need_accepted_hops(self):
        gold = path(accept_edges=(('s', 'n'), ('n', 't')))
        self.assertTrue(m.path_found(gold, ['s', 'n', 't']))
        self.assertFalse(m.path_found(gold, ['s', 'z', 't']))
        self.assertEqual(m.step_precision(gold, ['s', 'z', 't']), 0.0)
        self.assertEqual(m.step_precision(gold, ['s', 'n', 'z']), 0.5)
        self.assertFalse(m.path_found(path(accept_alternatives=False, accept_edges=(('s', 'n'), ('n', 't'))),
                                      ['s', 'n', 't']))

    def test_required_and_direction(self):
        self.assertFalse(m.path_found(path(required=('m',), accept_edges=(('s', 't'),)), ['s', 't']))
        self.assertFalse(m.path_found(path(), ['s', 'm', 't'][::-1]))
        undirected = path(start='t', end='s', steps=('t', 'm', 's'), directed=True)
        self.assertTrue(m.path_found(undirected, ['t', 'm', 's']))
        self.assertTrue(m.path_found(path(directed=False, start='s', end='t'), ['s', 'm', 't']))
        loose = path(directed=False, steps=('t', 'm', 's'), start='t', end='s')
        self.assertEqual(m.step_precision(loose, ['s', 'm', 't']), 1.0)

    def test_open_path(self):
        open_gold = path(end=None)
        self.assertEqual(m.path_target(open_gold), 't')
        self.assertTrue(m.path_found(open_gold, ['s', 'm', 't']))
        edges_only = path(end=None, steps=(), accept_edges=(('s', 'a'), ('a', 'b')))
        self.assertIsNone(m.path_target(edges_only))
        self.assertTrue(m.path_found(edges_only, ['s', 'a', 'b']))
        self.assertFalse(m.path_found(edges_only, ['s', 'b']))


class TemporalTest(unittest.TestCase):
    def test_stale_and_future(self):
        self.assertEqual(m.stale_serve_rate([(['old'], ['new']), (['old'], ['old', 'new'])]), m.Rate(1, 2))
        self.assertEqual(m.future_leak_rate([(['later'], ['now']), (['later'], ['later'])]), m.Rate(1, 2))
        self.assertTrue(m.as_of_correct(['then'], ['later'], ['then']))
        self.assertFalse(m.as_of_correct(['then'], ['later'], ['then', 'later']))
        self.assertFalse(m.as_of_correct(['then'], [], ['other']))
        self.assertEqual(m.as_of_accuracy([(['a'], [], ['a']), (['a'], ['b'], ['b'])]), m.Rate(1, 2))


class DecisionTest(unittest.TestCase):
    def test_distractor(self):
        self.assertEqual(m.distractor_in_core_rate([(['d'], ['a']), (['d'], ['d']), ([], ['a'])]), m.Rate(1, 3))

    def test_false_unknown_has_two_definitions(self):
        result = m.false_unknown_rate([(True, True), (True, False), (False, True), (False, False)])
        self.assertEqual(result.scorecard, m.Rate(1, 4))
        self.assertEqual(result.given_known, m.Rate(1, 2))

    def test_false_answer_by_type(self):
        rates = m.false_answer_rate_by_type([('anchor_absent', True), ('anchor_absent', False),
                                             ('near_miss_attribute', False)])
        self.assertEqual(list(rates), ['anchor_absent', 'near_miss_attribute'])
        self.assertEqual(rates['anchor_absent'], m.Rate(1, 2))

    def test_facet_coverage(self):
        facets = (Facet('a', ('r1',), (), True), Facet('b', ('r2', 'r3'), (), True), Facet('c', (), (), False))
        self.assertEqual(m.facet_coverage(facets, ['r3']), 0.5)
        self.assertEqual(m.facet_coverage(facets, ['r1', 'r2']), 1.0)
        self.assertIsNone(m.facet_coverage((Facet('c', (), (), False),), ['r1']))
        self.assertEqual(m.useful_rate([True, False, True, True]), m.Rate(3, 4))

    def test_precision_by_confidence(self):
        rates = m.precision_by_confidence([('high', True), ('high', False), ('low', True)])
        self.assertEqual(rates, {'high': m.Rate(1, 2), 'low': m.Rate(1, 1)})

    def test_aurc(self):
        self.assertIsNone(m.aurc([]).aurc)
        perfect = m.aurc([('high', True), ('medium', True), ('low', True)])
        self.assertEqual(perfect.aurc, 0.0)
        # Distinct scores: AURC = mean risk of the top-i prefixes = (0 + 1/2 + 1/3) / 3.
        ranked = m.aurc([(0.9, True), (0.5, False), (0.1, True)])
        self.assertAlmostEqual(ranked.aurc, (0 + 1 / 2 + 1 / 3) / 3)
        self.assertEqual(ranked.points[-1], (1.0, 1 / 3))
        # Ties are one step: high holds 1 right + 1 wrong, then low 1 right.
        tied = m.aurc([('high', True), ('high', False), ('low', True)])
        self.assertEqual(tied.points, ((2 / 3, 0.5), (1.0, 1 / 3)))
        self.assertAlmostEqual(tied.aurc, 2 / 3 * 0.5 + 1 / 3 * (1 / 3))
        # A wrong answer at high confidence costs more than at low.
        self.assertGreater(m.aurc([('high', False), ('low', True)]).aurc,
                           m.aurc([('high', True), ('low', False)]).aurc)


if __name__ == '__main__':
    unittest.main()
