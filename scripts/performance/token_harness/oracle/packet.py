"""Reconstruct the selected packet a consumer holds after a prefix of read calls.

A call without `page.cursor` or `continuation` starts (or restarts) the
selection: the server's restart action after shortened core prose asks the
consumer to discard the partial reconstruction, so only pages since the last
start form the packet. Pages then merge by identity: evidence by id, state and
action lines by exact text, claims and path hops by their whole content. A
continuation marked `projection.core_reused` carries only new items; the core
(summary, scope, answer) stays the one its first page delivered.
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
    selection_strings: list = field(default_factory=list)  # every string under scope.selection
    context_unbounded: list = field(default_factory=list)  # per page: state declared unbounded context

    @property
    def evidence_ids(self):
        return set(self.evidence)

    @property
    def state_is_unbounded_context(self):
        """Every page of the packet declares current_state as time-unbounded context."""
        return bool(self.context_unbounded) and all(self.context_unbounded)

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


def _strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from _strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from _strings(item)


def _at(structured, dotted):
    value = structured
    for part in dotted.split('.'):
        value = value.get(part) if isinstance(value, dict) else None
    return value


def core_reused(structured):
    """The page is a continuation that omits the stable core (kmp.recall.projection.v2)."""
    return (structured.get('projection') or {}).get('core_reused') is True


def declares_unbounded_state(structured):
    """The page says current_state is about context whose time is not bounded by the selection."""
    scope = structured.get('scope') or {}
    return ('wake.current_state' in (scope.get('context') or [])
            and scope.get('context_time') == 'unbounded')


def selection_paths(structured):
    return (structured.get('scope') or {}).get('selection') or []


def selection_strings(structured, paths=None):
    """Strings under the selection paths: the page's own, or (for a
    continuation without a core) those its first page declared."""
    paths = selection_paths(structured) if paths is None else paths
    return [text for path in paths if isinstance(path, str) for text in _strings(_at(structured, path))]


def build_packet(calls):
    starts = [index for index, call in enumerate(calls) if is_start(call.arguments)]
    selected = calls[starts[-1]:] if starts else calls
    packet, seen = Packet(), {}
    declared_paths = []
    for call in selected:
        structured = call.structured
        packet.pages += 1
        packet.final = structured
        # An incremental continuation (`projection.core_reused`) carries no
        # core, scope included: the declaration on the page that carried it
        # still governs the items this page adds.
        if core_reused(structured):
            packet.selection_strings += selection_strings(structured, declared_paths)
        else:
            declared_paths = selection_paths(structured)
            packet.selection_strings += selection_strings(structured)
            packet.context_unbounded.append(declares_unbounded_state(structured))
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
