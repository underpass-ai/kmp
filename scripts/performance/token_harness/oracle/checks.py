"""Evidence-identity and state checks over one reconstructed packet (annex A.11.2)."""
import re

from .relations import names_relation

IDENTIFIER = re.compile(r'^\S+$')


def cited_refs(packet, adapter):
    """(location, ref) for every id the packet cites as evidence."""
    cited = []
    for claim in packet.claims:
        if isinstance(claim, dict):
            cited += [('wake.causal_spine', ref) for ref in adapter.refs(claim)]
    for hop in packet.path:
        if isinstance(hop, dict):
            cited += [('proof.path', ref) for ref in hop.get('evidence_refs') or []
                      if isinstance(ref, str)]
    for item in packet.because:
        if isinstance(item, dict) and isinstance(item.get('ref'), str):
            cited.append(('because', item['ref']))
    return cited


def contract_violations(packet, adapter):
    return [problem for claim in packet.claims if isinstance(claim, dict)
            for problem in adapter.violations(claim)]


def unresolved_refs(packet, cited):
    known = packet.evidence_ids | packet.declared_missing
    return sorted({ref for _, ref in cited if ref not in known})


def disguised_refs(packet, cited, stored_bodies):
    """Ref-field values that are a stored body, or are not identifiers at all."""
    bodies = set(stored_bodies) | {item.get('text') for item in packet.evidence_items
                                   if isinstance(item, dict) and isinstance(item.get('text'), str)}
    return sorted({ref for _, ref in cited if ref in bodies or not IDENTIFIER.match(ref)})


def merged_identical_bodies(packet):
    """Hops or claims citing two distinct sources that hold the same text as one unit.

    An evidence object belongs to the node its `supports` names; two objects with
    equal text on different nodes are different units (annex A.11.1).
    """
    merged = []
    groups = [hop.get('evidence_refs') or [] for hop in packet.path if isinstance(hop, dict)]
    groups += [claim.get('evidence_refs') or [] for claim in packet.claims if isinstance(claim, dict)
               and isinstance(claim.get('evidence_refs'), list)]
    for refs in groups:
        by_text = {}
        for ref in refs:
            item = packet.evidence.get(ref)
            if isinstance(item, dict) and isinstance(item.get('text'), str):
                owner = tuple(item.get('supports') or [ref])
                by_text.setdefault(item['text'], set()).add(owner)
        merged += [text for text, owners in by_text.items() if len(owners) > 1]
    return merged


def support_bookkeeping(packet):
    return [line for line in packet.state if isinstance(line, str) and '--supports-->' in line]


def historical_actions(packet):
    return [line for line in packet.next_actions if isinstance(line, str) and names_relation(line)]


def usable_unit(item, unit_bodies):
    """An evidence object with identity, provenance and the exact stored body."""
    return (isinstance(item, dict) and item.get('text') in unit_bodies
            and bool(item.get('id')) and bool(item.get('source')))
