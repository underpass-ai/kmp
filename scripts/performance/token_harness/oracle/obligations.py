"""Task obligations of a scenario, judged against the packet and the fixture's refs."""
from ..scenarios.model import ObligationKind as K


def _strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from _strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from _strings(item)


def _state_surfaces(packet):
    """What a resume packet presents as state: context lines, actions, loops, summary."""
    return [line for line in packet.state + packet.next_actions + packet.open_loops + [packet.summary]
            if isinstance(line, str)]


def _bodies(packet, text):
    return [item for item in packet.evidence_items
            if isinstance(item, dict) and item.get('text') == text]


def _needles(obligation, refs):
    return [obligation.text] + ([refs[obligation.local_id]] if obligation.local_id in refs else [])


def state_hits(obligation, packet, refs):
    """Raw fact: state surfaces that name the excluded memory (reported as a figure)."""
    return [line for line in _state_surfaces(packet)
            if any(needle in line for needle in _needles(obligation, refs))]


def selection_hits(obligation, packet, refs):
    return [text for text in packet.selection_strings
            if any(needle in text for needle in _needles(obligation, refs))]


def judge(obligation, packet, refs, readback):
    """True, False, or None when the evidence to judge it was never captured."""
    kind = obligation.kind
    if kind is K.STATE_MENTIONS:
        return any(obligation.text in line for line in packet.state if isinstance(line, str))
    if kind is K.STATE_EXCLUDES:
        needles = [obligation.text] + ([refs[obligation.local_id]] if obligation.local_id in refs else [])
        if obligation.local_id and obligation.local_id not in refs:
            return None  # fixture ref not recorded: cannot judge exclusion by identity
        return not any(needle in line for line in _state_surfaces(packet) for needle in needles)
    if kind is K.AS_OF_EXCLUDES:
        if obligation.local_id and obligation.local_id not in refs:
            return None
        if packet.state_is_unbounded_context:
            return not selection_hits(obligation, packet, refs)
        return not state_hits(obligation, packet, refs) and not selection_hits(obligation, packet, refs)
    if kind is K.EVIDENCE_BODY:
        return any(item.get('id') and item.get('source') for item in _bodies(packet, obligation.text))
    if kind is K.DISTINCT_PROVENANCE:
        items = _bodies(packet, obligation.text)
        ids = {item.get('id') for item in items if item.get('id')}
        sources = {item.get('source') for item in items if item.get('source')}
        return len(ids) >= obligation.count and len(sources) >= obligation.count
    if kind is K.WRITE_ACCEPTED:
        final = packet.final
        return (final.get('accepted') is True and bool((final.get('receipt') or {}).get('ref'))
                and obligation.local_id in (final.get('local_refs') or {}))
    if kind is K.READBACK_BODY:
        if readback is None:
            return None
        return any(obligation.text in text for text in _strings(readback))
    raise ValueError(f'unhandled obligation {kind}')


def judge_all(scenario, packet, refs, readback, include_readback=True):
    rows = []
    for obligation in scenario.obligations:
        if obligation.kind is K.READBACK_BODY and not include_readback:
            continue
        rows.append({**obligation.as_dict(), 'satisfied': judge(obligation, packet, refs, readback)})
    return rows


def all_satisfied(rows):
    return bool(rows) and all(row['satisfied'] is True for row in rows)
