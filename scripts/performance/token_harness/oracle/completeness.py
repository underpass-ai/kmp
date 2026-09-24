"""Closure of a read or a write, split so `has_more=false` never stands for all of it."""


def transport_complete(trace, calls, journey_record):
    return (not trace.failures and not any(call.failed for call in calls)
            and journey_record.get('outcome', {}).get('status') == 'completed'
            and journey_record.get('exit_code') == 0 and bool(calls))


def selected_packet_complete(packet, is_write):
    final = packet.final
    if is_write:
        return final.get('accepted') is True
    projection = final.get('projection') or {}
    sections = projection.get('sections') or {}
    return (bool(projection) and not projection.get('next_action')
            and (projection.get('page') or {}).get('has_more') is False
            and projection.get('core_text_shortened') is False
            and all((section or {}).get('remaining', 0) == 0 for section in sections.values()))


def requested_scope_exhausted(packet, is_write):
    """Informational: orientation may be satisfied without exhausting the about."""
    if is_write:
        return None
    projection = packet.final.get('projection') or {}
    return projection.get('excluded_by_detail') == 0 and projection.get('selection_omitted') == 0
