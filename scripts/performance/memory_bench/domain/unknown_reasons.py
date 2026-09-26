"""What a binary says about an UNKNOWN, translated to the bench's `unknown_reason` vocabulary.

Gold names the expected cause of an UNKNOWN in bench words (`gold.UNKNOWN_REASONS`);
a binary reports its own. `translate(structured)` returns both: the binary's native
reason, and the bench reason it maps to, or None when the native reason does not
determine one. `reason_accuracy` only reads mapped reasons; an unmapped one is
counted apart, never guessed.

A v0.23.0 `kmp_ask` UNKNOWN carries no reason field. What it does say
(`kmp-proto-mapping` `responses.rs`, the `UNANSWERED` branch):

| Native reason | How v0.23.0 shows it | Bench reason |
|---|---|---|
| `nearest_outside` | `proof.nearest_outside` names a match outside the span | `not_in_selection` |
| `nothing_retrieved` | `proof.missing` = `any stored memory for: <question>` | `no_evidence` |
| `retrieved_not_bearing` | `proof.missing` = `stored memory that bears on: <question>` | None (*) |

(*) anchor or attribute: the binary does not say which, so the reason stays unmapped.

A binary that states a reason (a `reason` / `unknown_reason` string on the answer or
its proof) is read first: a bench word is taken as is, and the aliases below map the
obvious synonyms. Anything else is native and unmapped.

The P4 ask gate states `unknown_reason` on the answer, in the contract's words:

| `unknown_reason` | Bench reason |
|---|---|
| `no_candidates` | `no_evidence` |
| `no_bearing` | None (*) |
| `out_of_window` | `not_in_selection` |
| `anchor_absent_in_selection` | `anchor_not_found` (**) |
| `attribute_not_found` | `attribute_not_found` |

(**) The gate reads only the selected abouts and span, so an anchor that lives in
another about (`anchor_in_other_about` in gold) is reported absent from the selection:
the reason is right for `anchor_not_found` and wrong for `anchor_in_other_about`, which
the gate cannot tell apart. Telling them apart is deferred to L6 (Tirso, 26 Sept 2026):
the lexical index reads an anchor's postings outside the selection without loading it.
"""
from . import refs
from .gold import UNKNOWN_REASONS

NEAREST_OUTSIDE = 'nearest_outside'
NOTHING_RETRIEVED = 'nothing_retrieved'
RETRIEVED_NOT_BEARING = 'retrieved_not_bearing'
NATIVE_UNSTATED = 'unstated'  # an UNKNOWN with none of the signals above

NOTHING_PREFIX = 'any stored memory for:'
NOT_BEARING_PREFIX = 'stored memory that bears on:'

# Native reason -> bench reason (None: not determined by what the binary said).
TRANSLATION = {
    NEAREST_OUTSIDE: 'not_in_selection',
    NOTHING_RETRIEVED: 'no_evidence',
    RETRIEVED_NOT_BEARING: None,
    NATIVE_UNSTATED: None,
}
ALIASES = {
    'anchor_absent': 'anchor_not_found',
    'identifier_not_found': 'anchor_not_found',
    'attribute_absent': 'attribute_not_found',
    'anchor_other_about': 'anchor_in_other_about',
    'cross_about_anchor': 'anchor_in_other_about',
    'outside_selection': 'not_in_selection',
    'not_then': 'not_in_selection',
    'nothing_retrieved': 'no_evidence',
    # The P4 ask gate (`unknown_reason` on the answer).
    'no_candidates': 'no_evidence',
    'out_of_window': 'not_in_selection',
    'anchor_absent_in_selection': 'anchor_not_found',
}


def _stated(structured):
    proof = structured.get('proof') if isinstance(structured, dict) else None
    for holder in (structured, proof):
        if not isinstance(holder, dict):
            continue
        for key in ('unknown_reason', 'reason'):
            value = holder.get(key)
            if isinstance(value, str) and value:
                return value
    return None


def native_reason(structured):
    """The binary's own reason for an UNKNOWN answer; None for an answer that is not UNKNOWN."""
    if not refs.is_unknown(structured):
        return None
    stated = _stated(structured)
    if stated is not None:
        return stated
    if refs.nearest_outside_ref(structured) is not None:
        return NEAREST_OUTSIDE
    proof = structured.get('proof') if isinstance(structured, dict) else None
    missing = proof.get('missing') if isinstance(proof, dict) else None
    for item in missing if isinstance(missing, list) else ():
        if isinstance(item, str) and item.startswith(NOTHING_PREFIX):
            return NOTHING_RETRIEVED
        if isinstance(item, str) and item.startswith(NOT_BEARING_PREFIX):
            return RETRIEVED_NOT_BEARING
    return NATIVE_UNSTATED


def to_bench(native):
    """Bench vocabulary for a native reason, or None when it does not determine one."""
    if native is None:
        return None
    if native in UNKNOWN_REASONS:
        return native
    if native in ALIASES:
        return ALIASES[native]
    return TRANSLATION.get(native)


def translate(structured):
    """(native reason, bench reason or None) of an answer; (None, None) when not UNKNOWN."""
    native = native_reason(structured)
    return native, to_bench(native)
