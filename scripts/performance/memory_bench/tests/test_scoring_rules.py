"""Pre-registered scoring rules (BENCH_SPEC section 6), boundary cases first."""
import unittest

from ..domain import scoring_rules as r
from ..domain.gold import Gold

A = 'synth:mono-a000'
# SHA-256 of domain/scoring_rules.py as pre-registered; see test_rules_hash_is_stable.
RULES_SHA256 = '322dc58a9a4e2a4038a7f7f2525ce8dd29377e23ddeb8dc76c2ccb117c31f53b'


def ref(n):
    return f'{A}:e{n:07d}'


def facet(name, answers=(), words=None):
    return {'name': name, 'words': words, 'answers': [ref(n) for n in answers],
            'related': [], 'answerable_facet': bool(answers)}


ENUMERATIVE = Gold.from_dict({'answerable': 'KNOWN', 'shape': 'enumerative',
                              'facets': [facet('decision', [1]), facet('constraint', [2], 'the hard limit'),
                                         facet('status', [3])],
                              'wrong_subject': [ref(9)]})
SINGULAR = Gold.from_dict({'answerable': 'KNOWN', 'shape': 'singular', 'facets': [facet('valve', [1])]})
NEGATED = Gold.from_dict({'answerable': 'KNOWN', 'shape': 'singular', 'facets': [facet('valve', [1])],
                          'excluded_refs': [ref(4)]})
NEGATIVE_FACETS = [{'name': 'kubernetes', 'answers': [], 'related': [], 'answerable_facet': False}]
NEGATIVE = Gold.from_dict({'answerable': 'UNKNOWN', 'shape': 'singular',
                           'facets': [{'name': 'kubernetes', 'answers': [], 'related': [],
                                       'answerable_facet': False}],
                           'unknown_reason': 'attribute_not_found', 'absent_terms': ['Kubernetes', '#288']})


def seen(decision, cited=(), missing=(), reason=None):
    return r.Observed(decision, tuple(ref(n) for n in cited), tuple(missing), reason)


class PositiveTest(unittest.TestCase):
    def test_answer_correct_needs_an_answer_and_no_wrong_subject(self):
        ok = r.score(ENUMERATIVE, seen(r.ANSWER, [1, 5]))
        self.assertEqual((ok.outcome, ok.useful, ok.false_unknown), (r.ANSWER_CORRECT, True, False))
        self.assertAlmostEqual(ok.facet_coverage, 1 / 3)
        wrong_subject = r.score(ENUMERATIVE, seen(r.ANSWER, [1, 9]))
        self.assertEqual(wrong_subject.outcome, r.ANSWER_WRONG)
        self.assertTrue(wrong_subject.distractor_cited)
        self.assertEqual(r.score(ENUMERATIVE, seen(r.ANSWER, [5])).outcome, r.ANSWER_WRONG)
        self.assertEqual(r.score(ENUMERATIVE, seen(r.ANSWER, [])).outcome, r.ANSWER_WRONG)

    def test_excluded_refs_void_an_answer(self):
        self.assertEqual(r.score(NEGATED, seen(r.ANSWER, [1])).outcome, r.ANSWER_CORRECT)
        self.assertEqual(r.score(NEGATED, seen(r.ANSWER, [1, 4])).outcome, r.ANSWER_WRONG)

    def test_partial_on_a_singular_question_is_an_error(self):
        result = r.score(SINGULAR, seen(r.PARTIAL, [1], ['nothing']))
        self.assertEqual((result.outcome, result.useful), (r.PARTIAL_ON_SINGULAR, False))

    def test_partial_useful_on_enumerative(self):
        result = r.score(ENUMERATIVE, seen(r.PARTIAL, [1], ['status of the rollout']))
        self.assertEqual((result.outcome, result.useful), (r.PARTIAL_USEFUL, True))

    def test_partial_that_calls_a_covered_facet_missing_is_wrong(self):
        by_name = r.score(ENUMERATIVE, seen(r.PARTIAL, [1, 2], ['Constraint']))
        self.assertEqual(by_name.outcome, r.PARTIAL_WRONG)
        by_words = r.score(ENUMERATIVE, seen(r.PARTIAL, [2], ['The  hard-limit of C6.4']))
        self.assertEqual(by_words.outcome, r.PARTIAL_WRONG)
        # Naming a facet the core does NOT cover is exactly what PARTIAL is for.
        self.assertEqual(r.score(ENUMERATIVE, seen(r.PARTIAL, [2], ['decision'])).outcome, r.PARTIAL_USEFUL)

    def test_partial_needs_right_citations(self):
        self.assertEqual(r.score(ENUMERATIVE, seen(r.PARTIAL, [9, 1], ['status'])).outcome, r.PARTIAL_WRONG)
        self.assertEqual(r.score(ENUMERATIVE, seen(r.PARTIAL, [], ['status'])).outcome, r.PARTIAL_WRONG)

    def test_unknown_on_a_positive_is_a_false_unknown_with_its_reason(self):
        result = r.score(SINGULAR, seen(r.UNKNOWN, missing=['valve'], reason='no_evidence'))
        self.assertEqual((result.outcome, result.false_unknown, result.useful), (r.FALSE_UNKNOWN, True, False))
        self.assertIn('no_evidence', result.detail)
        self.assertIsNone(result.false_answer)


class NegativeTest(unittest.TestCase):
    def test_unknown_is_a_correct_abstention_and_its_reason_is_checked(self):
        right = r.score(NEGATIVE, seen(r.UNKNOWN, reason='attribute_not_found'))
        self.assertEqual((right.outcome, right.false_answer, right.reason_correct), (r.ABSTAIN_CORRECT, False, True))
        self.assertFalse(r.score(NEGATIVE, seen(r.UNKNOWN, reason='anchor_not_found')).reason_correct)
        self.assertIsNone(r.score(NEGATIVE, seen(r.UNKNOWN)).reason_correct)
        self.assertIsNone(right.useful)

    def test_partial_abstains_only_when_it_names_what_is_absent(self):
        naming = r.score(NEGATIVE, seen(r.PARTIAL, [1], ['no kubernetes deployment for this']))
        self.assertEqual(naming.outcome, r.ABSTAIN_CORRECT)
        anchor = r.score(NEGATIVE, seen(r.PARTIAL, [1], ['issue #288']))
        self.assertEqual(anchor.outcome, r.ABSTAIN_CORRECT)
        silent = r.score(NEGATIVE, seen(r.PARTIAL, [1], ['owner']))
        self.assertEqual((silent.outcome, silent.false_answer), (r.FALSE_ANSWER, True))
        self.assertEqual(r.score(NEGATIVE, seen(r.PARTIAL, [1], [])).outcome, r.FALSE_ANSWER)

    def test_citing_a_forbidden_ref_is_a_false_answer_whatever_the_decision(self):
        """Rust `GuardedDecision::is_false_answer`: `cited_forbidden` alone makes it false."""
        guarded = Gold.from_dict({'answerable': 'UNKNOWN', 'shape': 'singular', 'facets': NEGATIVE_FACETS,
                                  'absent_terms': ['Kubernetes'], 'wrong_subject': [ref(7)],
                                  'excluded_refs': [ref(8)]})
        partial = r.score(guarded, seen(r.PARTIAL, [7], ['no kubernetes here']))
        self.assertEqual((partial.outcome, partial.false_answer), (r.FALSE_ANSWER, True))
        self.assertIn(ref(7), partial.detail)
        unknown = r.score(guarded, seen(r.UNKNOWN, [8]))
        self.assertEqual((unknown.outcome, unknown.false_answer), (r.FALSE_ANSWER, True))
        self.assertEqual(r.score(guarded, seen(r.UNKNOWN, [1])).outcome, r.ABSTAIN_CORRECT)
        self.assertEqual(r.score(guarded, seen(r.PARTIAL, [1], ['kubernetes'])).outcome, r.ABSTAIN_CORRECT)

    def test_answer_on_a_negative_is_a_false_answer(self):
        result = r.score(NEGATIVE, seen(r.ANSWER, [1]))
        self.assertEqual((result.outcome, result.false_answer, result.positive), (r.FALSE_ANSWER, True, False))


class ObservedTest(unittest.TestCase):
    def test_reads_a_v023_answer(self):
        unknown = r.Observed.from_structured({'answer': 'UNKNOWN', 'because': [],
                                              'proof': {'missing': ['#288', {'term': 'x'}], 'confidence': 'low'}})
        self.assertEqual((unknown.decision, unknown.missing[0], unknown.confidence), (r.UNKNOWN, '#288', 'low'))
        self.assertEqual(unknown.missing[1], '{"term": "x"}')
        answer = r.Observed.from_structured({'answer': 'text', 'because': [{'ref': 'entry:' + ref(1)}]})
        self.assertEqual((answer.decision, answer.cited, answer.missing), (r.ANSWER, (ref(1),), ()))
        partial = r.Observed.from_structured({'answer': 'PARTIAL', 'because': [],
                                              'proof': {'missing': ['the cache engine']}})
        self.assertEqual((partial.decision, partial.missing), (r.PARTIAL, ('the cache engine',)))

    def test_partial_is_read_from_the_answer_field_like_rust(self):
        """`AskVerdict::read` (retrieval_scorecard.rs) reads `answer`; a `decision` field is not
        part of the contract and must not turn an answer into a PARTIAL."""
        stray = r.Observed.from_structured({'answer': 'text', 'decision': 'PARTIAL'})
        self.assertEqual(stray.decision, r.ANSWER)
        negative = Gold.from_dict({'answerable': 'UNKNOWN', 'shape': 'singular', 'facets': NEGATIVE_FACETS,
                                   'absent_terms': ['cache']})
        read = r.Observed.from_structured({'answer': 'PARTIAL', 'because': [],
                                           'proof': {'missing': ['the cache engine']}})
        self.assertEqual(r.score(negative, read).outcome, r.ABSTAIN_CORRECT)

    def test_reads_the_gate_status_before_the_answer_field(self):
        """A P4 binary states `answer_status`; its PARTIAL keeps the citations in `answer`."""
        partial = r.Observed.from_structured({'answer': 'entry:a', 'answer_status': 'partial',
                                              'because': [{'ref': 'entry:' + ref(1)}],
                                              'proof': {'missing': ['rollback'], 'confidence': 'medium'}})
        self.assertEqual((partial.decision, partial.cited, partial.missing), (r.PARTIAL, (ref(1),), ('rollback',)))
        unknown = r.Observed.from_structured({'answer': 'UNKNOWN', 'answer_status': 'unknown',
                                              'unknown_reason': 'attribute_not_found'})
        self.assertEqual(unknown.decision, r.UNKNOWN)
        answered = r.Observed.from_structured({'answer': 'PARTIAL', 'answer_status': 'answered'})
        self.assertEqual(answered.decision, r.ANSWER)

    def test_rejects_unknown_decisions(self):
        with self.assertRaises(ValueError):
            r.Observed('MAYBE')

    def test_names_folds_case_and_separators(self):
        self.assertTrue(r.names(['Hard_Limit reached'], 'hard-limit'))
        self.assertFalse(r.names(['anything'], ''))
        self.assertFalse(r.names([], 'x'))

    def test_confidence_correct(self):
        self.assertTrue(r.confidence_correct(r.score(SINGULAR, seen(r.ANSWER, [1]))))
        self.assertTrue(r.confidence_correct(r.score(NEGATIVE, seen(r.UNKNOWN))))
        self.assertFalse(r.confidence_correct(r.score(NEGATIVE, seen(r.ANSWER, [1]))))

    def test_rules_hash_is_stable(self):
        """The pre-registered rules are frozen by hash: editing scoring_rules.py must be a
        deliberate act that updates this constant (and SCHEMAS.md §0's BENCH_VERSION rule)."""
        self.assertEqual(r.rules_sha256(), RULES_SHA256)


if __name__ == '__main__':
    unittest.main()
