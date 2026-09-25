"""Pre-registered scoring rules (BENCH_SPEC section 6), boundary cases first."""
import unittest

from ..domain import scoring_rules as r
from ..domain.gold import Gold

A = 'synth:mono-a000'


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
        partial = r.Observed.from_structured({'answer': 'text', 'decision': 'PARTIAL'})
        self.assertEqual(partial.decision, r.PARTIAL)

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
        self.assertEqual(r.rules_sha256(), r.rules_sha256())
        self.assertRegex(r.rules_sha256(), r'^[0-9a-f]{64}$')


if __name__ == '__main__':
    unittest.main()
