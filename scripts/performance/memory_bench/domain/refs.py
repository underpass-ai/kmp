"""Refs as a reader judges them: the memory, not the envelope it arrived in.

`normalize` is a line-for-line port of `memory_ref::normalize` in
crates/kmp-testkit/src/memory_ref.rs, the rule retrieval_kmp_scorecard.rs
reads answers with. It removes two layers of address:

1. the envelope: one leading `entry:`, otherwise one leading `detail:`,
   otherwise nothing. Exactly one prefix goes, never both, so
   `entry:detail:x` becomes `detail:x`;
2. the evidence node: what is left, when it starts with `evidence:`, is the
   evidence of the entry written after that prefix, and cites that entry. The
   node's own suffix goes with it: `:current`, or `:relation:<n>` where `<n>`
   is decimal digits or 16 hex digits (the forms `kmp_ask` returns). Guide
   evidence has no suffix: `evidence:guide:kmp-agent:verb:time` cites
   `guide:kmp-agent:verb:time`.

So `detail:evidence:project:x:entry:decision:d:current` and
`detail:evidence:project:x:entry:decision:d:relation:1` both cite
`project:x:entry:decision:d`, as `entry:project:x:entry:decision:d` does
(decision of 28 Sept 2026; before it, BENCH_VERSION v2, evidence citations
counted as refs of their own and never matched a judged entry).
`retrieved` keeps every returned memory as it arrives, repeats included: a
memory that comes with its evidence counts twice (decision of 28 Sept 2026,
BENCH_VERSION v4; v3 kept each memory once).
`metric_parity.json` pins both ports to the same tables (`refs`, `retrieved`).

`evidence_refs`, `cited_refs` and `is_unknown` port how the same scorecard
reads a `kmp_ask` answer, so a bench metric and a Rust scorecard column read
the same refs from the same response.
"""
import re

ENTRY_PREFIX = 'entry:'
DETAIL_PREFIX = 'detail:'
EVIDENCE_PREFIX = 'evidence:'
CURRENT_SUFFIX = ':current'
RELATION_MARK = ':relation:'
UNKNOWN = 'UNKNOWN'
_RELATION_ORDINAL = re.compile(r'[0-9]+|[0-9a-f]{16}')


def _envelope(value):
    """One leading `entry:`, else one leading `detail:`, else the value."""
    if value.startswith(ENTRY_PREFIX):
        return value[len(ENTRY_PREFIX):]
    if value.startswith(DETAIL_PREFIX):
        return value[len(DETAIL_PREFIX):]
    return value


def _evidence_subject(node):
    """The entry an `evidence:` node is evidence of, or None for any other ref."""
    if not node.startswith(EVIDENCE_PREFIX):
        return None
    subject = node[len(EVIDENCE_PREFIX):]
    if subject.endswith(CURRENT_SUFFIX):
        return subject[:-len(CURRENT_SUFFIX)]
    head, mark, ordinal = subject.rpartition(RELATION_MARK)
    if mark and _RELATION_ORDINAL.fullmatch(ordinal):
        return head
    return subject


def normalize(value):
    """memory_ref.rs `normalize`, byte for byte: the entry a returned ref cites."""
    node = _envelope(value)
    subject = _evidence_subject(node)
    return node if subject is None or not subject else subject


def _field(value, key):
    """serde_json indexing: a missing key or a non-object reads as null."""
    return value.get(key) if isinstance(value, dict) else None


def memory_ref(item):
    """`memory_ref`: the normalized `id` of an evidence item, or None."""
    identifier = _field(item, 'id')
    return normalize(identifier) if isinstance(identifier, str) else None


def retrieved(ids):
    """memory_ref.rs `retrieved`: normalized, in response order, repeats kept."""
    return [normalize(identifier) for identifier in ids]


def evidence_refs(structured):
    """`proof.evidence[].id` in response order, normalized, repeats kept; non-string ids skipped."""
    items = _field(_field(structured, 'proof'), 'evidence')
    if not isinstance(items, list):
        return []
    return retrieved(identifier for identifier in (_field(item, 'id') for item in items)
                     if isinstance(identifier, str))


def cited_refs(structured):
    """`because[].ref` normalized, as the scorecard's BTreeSet: sorted and unique."""
    items = _field(structured, 'because')
    if not isinstance(items, list):
        return ()
    refs = {normalize(ref) for ref in (_field(item, 'ref') for item in items) if isinstance(ref, str)}
    return tuple(sorted(refs))  # Rust String order is UTF-8 byte order, i.e. code point order


def is_unknown(structured):
    return _field(structured, 'answer') == UNKNOWN


def nearest_outside_ref(structured):
    """`proof.nearest_outside.ref` as the scorecard reads it (not normalized there either)."""
    ref = _field(_field(_field(structured, 'proof'), 'nearest_outside'), 'ref')
    return ref if isinstance(ref, str) else None


_WHITESPACE = re.compile(r'\s')


def is_canonical(ref):
    """A gold ref: non-empty, no whitespace, and already what `normalize` would return."""
    return isinstance(ref, str) and bool(ref) and not _WHITESPACE.search(ref) and normalize(ref) == ref


def owned_by(ref, about):
    """KMP's ownership rule for entry ids: a safe descendant beginning with `<about>:`."""
    return ref.startswith(about + ':') and len(ref) > len(about) + 1
