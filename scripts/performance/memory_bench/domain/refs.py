"""Refs as a reader judges them: the memory, not the envelope it arrived in.

`normalize` is a line-for-line port of `strip_prefix` in
crates/kmp-testkit/src/bin/retrieval_kmp_scorecard.rs (lines 334-340 at
v0.23.0): strip one leading `entry:`, otherwise one leading `detail:`,
otherwise keep the value. Exactly one prefix goes, never both, so
`entry:detail:x` becomes `detail:x`. `evidence_refs`, `cited_refs` and
`is_unknown` port how the same scorecard reads a `kmp_ask` answer, so a bench
metric and a Rust scorecard column read the same refs from the same response.
"""
import re

ENTRY_PREFIX = 'entry:'
DETAIL_PREFIX = 'detail:'
UNKNOWN = 'UNKNOWN'


def normalize(value):
    """retrieval_kmp_scorecard.rs `strip_prefix`, byte for byte."""
    if value.startswith(ENTRY_PREFIX):
        return value[len(ENTRY_PREFIX):]
    if value.startswith(DETAIL_PREFIX):
        return value[len(DETAIL_PREFIX):]
    return value


def _field(value, key):
    """serde_json indexing: a missing key or a non-object reads as null."""
    return value.get(key) if isinstance(value, dict) else None


def memory_ref(item):
    """`memory_ref`: the normalized `id` of an evidence item, or None."""
    identifier = _field(item, 'id')
    return normalize(identifier) if isinstance(identifier, str) else None


def evidence_refs(structured):
    """`proof.evidence[].id` in response order, normalized; non-string ids skipped."""
    items = _field(_field(structured, 'proof'), 'evidence')
    if not isinstance(items, list):
        return []
    return [ref for ref in (memory_ref(item) for item in items) if ref is not None]


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
