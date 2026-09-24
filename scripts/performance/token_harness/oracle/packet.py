"""Reconstruct the selected packet a consumer holds after a prefix of read calls.

A call without `page.cursor` or `continuation` starts (or restarts) the
selection: the server's restart action after shortened core prose asks the
consumer to discard the partial reconstruction, so only pages since the last
start form the packet. Pages then merge by identity: evidence by id, state and
action lines by exact text, claims and path hops by their whole content.
"""
from dataclasses import dataclass, field
import json


def _key(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False)


def is_start(arguments):
    page = arguments.get('page') or {}
    return not page.get('cursor') and not arguments.get('continuation')


@dataclass
class Packet:
    pages: int = 0
    evidence: dict = field(default_factory=dict)  # id -> item (first delivery kept)
    evidence_items: list = field(default_factory=list)  # every delivered item, for identity checks
    state: list = field(default_factory=list)
    next_actions: list = field(default_factory=list)
    open_loops: list = field(default_factory=list)
    summary: str = ''
    claims: list = field(default_factory=list)
    path: list = field(default_factory=list)
    because: list = field(default_factory=list)
    missing: list = field(default_factory=list)
    final: dict = field(default_factory=dict)

    @property
    def evidence_ids(self):
        return set(self.evidence)

    @property
    def declared_missing(self):
        declared = set()
        for item in self.missing:
            if isinstance(item, str):
                declared.add(item)
            elif isinstance(item, dict):
                declared.update(v for k, v in item.items() if k in ('id', 'ref') and isinstance(v, str))
        return declared


def _extend_unique(target, values, seen):
    for value in values or []:
        key = _key(value)
        if key not in seen:
            seen.add(key)
            target.append(value)


def build_packet(calls):
    starts = [index for index, call in enumerate(calls) if is_start(call.arguments)]
    selected = calls[starts[-1]:] if starts else calls
    packet, seen = Packet(), {}
    for call in selected:
        structured = call.structured
        packet.pages += 1
        packet.final = structured
        wake = structured.get('wake') or {}
        proof = structured.get('proof') or {}
        for item in proof.get('evidence') or []:
            packet.evidence_items.append(item)
            if isinstance(item, dict) and isinstance(item.get('id'), str):
                packet.evidence.setdefault(item['id'], item)
        for name in ('state', 'next_actions', 'open_loops', 'claims', 'path', 'because', 'missing'):
            source = {'state': wake.get('current_state'), 'next_actions': wake.get('next_actions'),
                      'open_loops': wake.get('open_loops'), 'claims': wake.get('causal_spine'),
                      'path': proof.get('path'), 'because': structured.get('because'),
                      'missing': proof.get('missing')}[name]
            _extend_unique(getattr(packet, name), source, seen.setdefault(name, set()))
        if isinstance(structured.get('summary'), str):
            packet.summary = structured['summary']
    return packet
