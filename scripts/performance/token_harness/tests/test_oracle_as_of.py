"""A dated wake: context declared unbounded moves the as_of check to the selection."""
import unittest

from ..oracle.contracts import contract
from ..oracle.journey import evaluate_journey
from ..scenarios.model import JourneySpec, Obligation, ObligationKind as K, Scenario
from .oracle_fixtures import (BODY, RECORD, SUMMARY, UNBOUNDED_SCOPE, claim_v2, evidence,
                              journey_events, trace_of, wake_page)

LATER = 'Lift the freeze.'
LATER_REF = 'fixture:t:entry:decision:lift'
DATED = Scenario(
    case_id='t04', goal_id='dated_test', about='fixture:t', description='unit-test dated wake',
    fixture=(), journey=JourneySpec('kmp_wake', {'about': 'fixture:t', 'as_of': {'time': 'x'}}),
    obligations=(Obligation(K.STATE_MENTIONS, SUMMARY), Obligation(K.EVIDENCE_BODY, BODY),
                 Obligation(K.AS_OF_EXCLUDES, LATER, 'lift')),
    unit_bodies=(BODY,), stored_bodies=(SUMMARY, BODY, LATER))
STATE = ['(observation) ' + SUMMARY, '(decision) ' + LATER]


def judge(page):
    trace = trace_of(journey_events([page]), 't04-b4096')
    return evaluate_journey(DATED, trace, RECORD, {'lift': LATER_REF}, None,
                            contract('kmp.wake_claim.v2'))


def excluded(result):
    return next(o['satisfied'] for o in result['obligations'] if o['kind'] == 'as_of_excludes')


class AsOfTest(unittest.TestCase):
    def test_later_memory_in_declared_unbounded_context_passes_and_is_reported(self):
        result = judge(wake_page([claim_v2()], [evidence()], state=STATE, scope=UNBOUNDED_SCOPE))
        self.assertTrue(excluded(result))
        self.assertTrue(result['quality_pass'], result['quality_failures'])
        self.assertEqual(result['metrics']['post_as_of_memory_in_state_count'], 1)
        self.assertTrue(result['metrics']['state_declared_unbounded_context'])

    def test_later_memory_in_the_selection_fails_even_when_context_is_declared(self):
        later = evidence(text=LATER, identifier='detail:' + LATER_REF, source=LATER_REF)
        result = judge(wake_page([claim_v2()], [evidence(), later], state=STATE[:1],
                                 scope=UNBOUNDED_SCOPE))
        self.assertFalse(excluded(result))
        self.assertFalse(result['quality_pass'])

    def test_later_ref_in_a_causal_spine_claim_fails(self):
        claim = {'claim': LATER_REF + ' -> x', 'because': 'why', 'evidence_refs': []}
        result = judge(wake_page([claim_v2(), claim], [evidence()], state=STATE[:1],
                                 scope=UNBOUNDED_SCOPE))
        self.assertFalse(excluded(result))

    def test_undeclared_state_must_exclude_the_later_memory(self):
        result = judge(wake_page([claim_v2()], [evidence()], state=STATE))
        self.assertFalse(excluded(result))
        self.assertFalse(result['metrics']['state_declared_unbounded_context'])
        self.assertFalse(result['quality_pass'])

    def test_context_bounded_in_time_is_not_a_declaration(self):
        bounded = {**UNBOUNDED_SCOPE, 'context_time': 'as_of'}
        self.assertFalse(excluded(judge(wake_page([claim_v2()], [evidence()], state=STATE,
                                                  scope=bounded))))


def incremental(items=(), state=()):
    """A kmp.recall.projection.v2 continuation: new items only, no core."""
    page = {'projection': {'core_reused': True, 'next_action': None, 'core_text_shortened': False,
                           'page': {'has_more': False}, 'excluded_by_detail': 0,
                           'selection_omitted': 0, 'sections': {'proof.evidence': {'remaining': 0}}}}
    if items:
        page['proof'] = {'evidence': list(items)}
    if state:
        page['wake'] = {'current_state': list(state)}
    return page


def judge_pages(pages):
    first = {'about': 'fixture:t', 'budget': {'max_bytes': 4096}}
    rest = [{**first, 'page': {'cursor': f'kmp1:{n}:h'}} for n in range(1, len(pages))]
    trace = trace_of(journey_events(pages, [first] + rest), 't04-b4096')
    return evaluate_journey(DATED, trace, RECORD, {'lift': LATER_REF}, None,
                            contract('kmp.wake_claim.v2'))


class IncrementalContinuationTest(unittest.TestCase):
    def test_a_continuation_without_a_core_keeps_the_first_page_declaration(self):
        pending = wake_page([claim_v2()], [evidence()], state=STATE[:1], has_more=True,
                            scope=UNBOUNDED_SCOPE, next_action={'tool': 'kmp_wake'})
        result = judge_pages([pending, incremental(state=STATE[1:])])
        self.assertTrue(excluded(result))
        self.assertTrue(result['metrics']['state_declared_unbounded_context'])
        self.assertTrue(result['quality_pass'], result['quality_failures'])

    def test_a_later_memory_in_a_continuation_selection_still_fails(self):
        later = evidence(text=LATER, identifier='detail:' + LATER_REF, source=LATER_REF)
        pending = wake_page([claim_v2()], [evidence()], state=STATE[:1], has_more=True,
                            scope=UNBOUNDED_SCOPE, next_action={'tool': 'kmp_wake'})
        result = judge_pages([pending, incremental(items=[later])])
        self.assertFalse(excluded(result))
        self.assertFalse(result['quality_pass'])


if __name__ == '__main__':
    unittest.main()
