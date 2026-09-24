"""Oracle checks on small packets, including annex A.17 negative controls 1 and 2."""
import unittest

from ..oracle.calls import tool_calls
from ..oracle.checks import (cited_refs, disguised_refs, historical_actions, merged_identical_bodies,
                             support_bookkeeping, unresolved_refs)
from ..oracle.completeness import selected_packet_complete
from ..oracle.contracts import contract
from ..oracle.journey import evaluate_journey
from ..oracle.packet import build_packet
from ..oracle.relations import names_relation
from .oracle_fixtures import (BODY, EVIDENCE_ID, MEMORY, RECORD, SCENARIO, SUMMARY, claim_v1, claim_v2,
                              evidence, journey_events, trace_of, wake_page)

V1, V2 = contract('kmp.wake_claim.v1'), contract('kmp.wake_claim.v2')


def packet_of(*pages, arguments=None):
    return build_packet(tool_calls(trace_of(journey_events(list(pages), arguments))))


def judge(pages, arguments=None, adapter=V2, scenario=SCENARIO):
    return evaluate_journey(scenario, trace_of(journey_events(pages, arguments)), RECORD, {}, None, adapter)


class EvidenceIdentityTest(unittest.TestCase):
    def test_prose_in_v1_ref_is_unresolved_and_disguised(self):
        packet = packet_of(wake_page([claim_v1(BODY)], [evidence()]))
        cited = cited_refs(packet, V1)
        self.assertEqual(unresolved_refs(packet, cited), [BODY])
        self.assertEqual(disguised_refs(packet, cited, SCENARIO.stored_bodies), [BODY])

    def test_v2_ids_that_resolve_in_the_same_packet_pass(self):
        packet = packet_of(wake_page([claim_v2()], [evidence()]))
        cited = cited_refs(packet, V2)
        self.assertEqual((unresolved_refs(packet, cited), disguised_refs(packet, cited, ())), ([], []))

    def test_an_id_resolved_on_a_later_page_of_the_packet_counts(self):
        first = wake_page([claim_v2()], [], has_more=True,
                          next_action={'tool': 'kmp_wake', 'arguments': {}})
        pages = [first, wake_page([], [evidence()])]
        arguments = [{'about': 'fixture:t'}, {'about': 'fixture:t', 'page': {'cursor': 'kmp1:1:x'}}]
        packet = packet_of(*pages, arguments=arguments)
        self.assertEqual(unresolved_refs(packet, cited_refs(packet, V2)), [])

    def test_unknown_id_is_unresolved_unless_declared_missing(self):
        packet = packet_of(wake_page([claim_v2(['detail:gone'])], [evidence()]))
        self.assertEqual(unresolved_refs(packet, cited_refs(packet, V2)), ['detail:gone'])
        packet = packet_of(wake_page([claim_v2(['detail:gone'])], [evidence()], missing=['detail:gone']))
        self.assertEqual(unresolved_refs(packet, cited_refs(packet, V2)), [])

    def test_contract_adapters_refuse_the_other_version(self):
        self.assertTrue(V1.violations(claim_v2()))
        self.assertTrue(V2.violations(claim_v1()))
        self.assertEqual(V2.violations({'claim': 'anchor -> label', 'because': 'structural'}), [])
        with self.assertRaises(KeyError):
            contract('kmp.wake_claim.v9')

    def test_equal_bodies_of_two_sources_cited_as_one_unit_are_detected(self):
        other = evidence(identifier='detail:evidence:other:current', owner='evidence:other:current')
        hop = {'from': 'a', 'to': 'b', 'evidence_refs': [EVIDENCE_ID, 'detail:evidence:other:current']}
        packet = packet_of(wake_page([], [evidence(), other], path=[hop]))
        self.assertEqual(merged_identical_bodies(packet), [BODY])


class StateTest(unittest.TestCase):
    def test_supports_lines_in_state_are_counted(self):
        line = 'Relationship evidence:x --supports--> ' + MEMORY + ' [evidential]'
        packet = packet_of(wake_page([], [], state=[line, line + ' again']))
        self.assertEqual(len(support_bookkeeping(packet)), 2)

    def test_relation_named_as_next_action_is_historical(self):
        packet = packet_of(wake_page([], [], next_actions=['depends_on → fixture:x', 'Ship the fix']))
        self.assertEqual(historical_actions(packet), ['depends_on → fixture:x'])
        self.assertIsNone(names_relation('ship_it → x'))

    def test_displacement_needs_both_bookkeeping_and_a_missing_memory(self):
        line = 'Relationship evidence:x --supports--> y'
        result = judge([wake_page([claim_v2()], [evidence()], state=[line])])
        self.assertEqual(result['metrics']['support_displacement_count'], 1)
        self.assertFalse(result['quality_pass'])


class CompletenessTest(unittest.TestCase):
    PAGES = [wake_page([claim_v2()], [], has_more=True,
                       next_action={'tool': 'kmp_wake', 'arguments': {'page': {'cursor': 'c'}}}),
             wake_page([], [evidence()])]
    ARGS = [{'about': 'fixture:t'}, {'about': 'fixture:t', 'page': {'cursor': 'c'}}]

    def test_full_journey_passes(self):
        result = judge(self.PAGES, self.ARGS)
        self.assertTrue(result['quality_pass'], result['quality_failures'])
        self.assertEqual(result['metrics']['first_supported_unit']['after_rpc'], 2)
        self.assertEqual(result['metrics']['mandatory_continuations'], 1)

    def test_removing_a_page_fails_completeness_even_though_it_is_smaller(self):  # A.17 #1
        result = judge(self.PAGES[:1], self.ARGS[:1])
        self.assertFalse(result['completeness']['selected_packet_complete'])
        self.assertFalse(result['quality_pass'])

    def test_changed_negation_or_source_fails_the_obligation(self):  # A.17 #2
        negated = evidence(text=BODY.replace('moves', 'does not move'))
        self.assertFalse(judge([wake_page([], [negated])])['completeness']['task_obligations_satisfied'])
        unsourced = evidence(source='')
        self.assertFalse(judge([wake_page([], [unsourced])])['completeness']['task_obligations_satisfied'])

    def test_restart_discards_the_partial_reconstruction(self):
        shortened = wake_page([], [evidence()], state=['(observation) The backup…'], shortened=True,
                              next_action={'tool': 'kmp_wake', 'arguments': {'about': 'fixture:t'}})
        packet = packet_of(shortened, wake_page([], [], state=[]))
        self.assertEqual(packet.pages, 1)
        self.assertEqual(packet.state, [])
        self.assertTrue(selected_packet_complete(packet, False))

    def test_shortened_final_page_is_not_complete(self):
        packet = packet_of(wake_page([], [evidence()], shortened=True))
        self.assertFalse(selected_packet_complete(packet, False))

    def test_summary_state_is_required(self):
        result = judge([wake_page([], [evidence()], state=['(observation) something else'])])
        unmet = [o['text'] for o in result['obligations'] if not o['satisfied']]
        self.assertEqual(unmet, [SUMMARY])


if __name__ == '__main__':
    unittest.main()
