"""Judge one measured journey: metrics, completeness split and a single quality verdict."""
from .calls import tool_calls
from .checks import (contract_violations, cited_refs, disguised_refs, historical_actions,
                     merged_identical_bodies, support_bookkeeping, unresolved_refs, usable_unit)
from .completeness import requested_scope_exhausted, selected_packet_complete, transport_complete
from .obligations import all_satisfied, judge_all
from .packet import build_packet
from ..scenarios.model import ObligationKind as K


def _first_unit(calls, scenario, is_write):
    for call in calls:
        if is_write:
            final = call.structured
            if final.get('accepted') is True and (final.get('receipt') or {}).get('ref'):
                return call
        elif any(usable_unit(item, scenario.unit_bodies)
                 for item in (call.structured.get('proof') or {}).get('evidence') or []):
            return call
    return None


def _task_ready(calls, scenario, refs):
    for index in range(1, len(calls) + 1):
        packet = build_packet(calls[:index])
        if all_satisfied(judge_all(scenario, packet, refs, None, include_readback=False)):
            return calls[index - 1]
    return None


def _point(call):
    return {'after_rpc': call.ordinal, 'event_index': call.event_index} if call else None


def evaluate_journey(scenario, trace, record, refs, readback, adapter):
    calls = tool_calls(trace)
    is_write = scenario.journey.tool == 'kmp_write_memory'
    packet = build_packet(calls)
    refs = {**refs, **(packet.final.get('local_refs') or {})} if is_write else refs
    cited = cited_refs(packet, adapter)
    unresolved = unresolved_refs(packet, cited)
    disguised = disguised_refs(packet, cited, scenario.stored_bodies)
    bookkeeping = support_bookkeeping(packet)
    obligations = judge_all(scenario, packet, refs, readback)
    state_missing = [row for row in obligations if row['kind'] == K.STATE_MENTIONS.value
                     and row['satisfied'] is not True]
    actions = historical_actions(packet)
    violations = contract_violations(packet, adapter)
    merged = merged_identical_bodies(packet)
    completeness = {
        'transport_complete': transport_complete(trace, calls, record),
        'selected_packet_complete': selected_packet_complete(packet, is_write),
        'requested_scope_exhausted': requested_scope_exhausted(packet, is_write),
        'task_obligations_satisfied': all_satisfied(obligations)}
    metrics = {
        'first_supported_unit': _point(_first_unit(calls, scenario, is_write)),
        'task_ready': _point(_task_ready(calls, scenario, refs)),
        'unresolved_evidence_reference_count': len(unresolved),
        'evidence_body_disguised_as_ref_count': len(disguised),
        'support_bookkeeping_in_state_count': len(bookkeeping),
        'support_displacement_count': len(state_missing) if bookkeeping else 0,
        'historical_relation_as_action_count': len(actions),
        'contract_violation_count': len(violations),
        'identical_body_merged_citation_count': len(merged),
        'cited_reference_count': len({ref for _, ref in cited}),
        'rpc_calls': len(calls),
        'selection_restarts': sum(1 for c in calls[1:] if not (c.arguments.get('page') or {}).get('cursor')
                                  and not c.arguments.get('continuation')) if not is_write else 0,
        'mandatory_continuations': max(packet.pages - 1, 0)}
    failures = [name for name, ok in completeness.items()
                if name != 'requested_scope_exhausted' and ok is not True]
    failures += [name for name in ('unresolved_evidence_reference_count',
                                   'evidence_body_disguised_as_ref_count', 'support_displacement_count',
                                   'historical_relation_as_action_count', 'contract_violation_count',
                                   'identical_body_merged_citation_count')
                 if metrics[name]]
    return {'journey': trace.journey, 'case_id': scenario.case_id, 'goal_id': scenario.goal_id,
            'budget': record.get('budget'), 'selection': calls[0].arguments if calls else None,
            'contract': adapter.contract_id, 'metrics': metrics, 'completeness': completeness,
            'obligations': obligations, 'quality_pass': not failures, 'quality_failures': failures,
            'detail': {'unresolved_refs': unresolved[:20], 'disguised_refs': disguised[:20],
                       'support_lines_in_state': bookkeeping[:10], 'historical_actions': actions,
                       'contract_violations': sorted(set(violations))[:10],
                       'driver_outcome': record.get('outcome')}}
