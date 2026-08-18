# Recall projection contract

`kernel_ask` and `kernel_wake` expose a deterministic core-plus-prefix
projection at the MCP host boundary. The kernel remains responsible for
retrieval, temporal currentness, confidence, and ranking. The projection
gateway is responsible for preserving that selected core under a host-safe
serialized response budget and for making omitted proof recoverable.

This is a contract algorithm: the budget is known before the structured packet
is filled. It does not call an LLM and does not summarize evidence
generatively.

## Stable core

For Ask, the non-prunable core contains:

- `summary`, the citation-oriented `answer`, and every `because` ref selected
  by the kernel;
- the canonical `proof.evidence` bodies referenced by those citations;
- conflicts, supersessions, confidence, matched terms/relations, and graph
  frontier accounting.

For Wake, one item from each available state section and the evidence needed by
the retained causal spine form the core. `resume_cursor` remains untouched.

If long prose makes the core exceed `budget.max_bytes`, textual fields are
bounded while stable IDs, refs, claims, support targets, and evidence refs stay
intact. If even that reference envelope cannot fit, the call fails explicitly
and asks for a larger byte budget; it never falls through to an unrelated
single reason.

## Fixed expansion order and detail

Expansion items receive one total deterministic order before budgeting. The
order is independent of `budget.tokens`, `budget.max_bytes`, and page size:

1. semantic proof relations;
2. additional relevant evidence and Wake state;
3. support/bookkeeping relations;
4. structural relations and raw missing refs.

Stable serialized content breaks ties. The requested detail is a nested
fieldset:

- `compact`: stable core plus semantic proof/current state;
- `balanced` (default): compact plus additional evidence, support relations,
  open loops, next actions, and guardrails;
- `full`: balanced plus structural relations and raw missing refs.

Consequently, increasing a budget returns a longer prefix of the same order,
and `compact ⊆ balanced ⊆ full` for the same selected core.

## Budgets

`budget.max_bytes` is normative and defaults to 10,000. KMP measures the exact
compact JSON bytes it returns and exposes the stabilized measurement in
`projection.budget.used_bytes`. Every tool definition advertises the same
host-safe default through `_meta["anthropic/maxResultSizeChars"]`.

`budget.tokens` remains compatible with existing clients and guides prefix
planning using KMP's local cl100k estimator. It is explicitly advisory: a
cl100k count cannot prove a hard token limit for Claude-family or future host
models. The byte ceiling is the portable offline guarantee.

## Continuation

When expandable items remain, the response contains:

```json
{
  "projection": {
    "contract": "kmp.recall.projection.v1",
    "detail": "full",
    "budget": {
      "max_bytes": 10000,
      "used_bytes": 7342,
      "tokens_advisory": 2400
    },
    "page": {
      "offset": 0,
      "returned": 17,
      "total": 93,
      "has_more": true,
      "next_cursor": "opaque"
    },
    "sections": {},
    "next_action": "Repeat this recall with the returned cursor."
  }
}
```

Repeat the same tool with `page.cursor` set to `next_cursor`. The caller may
change only `page.entries`, `budget.tokens`, or `budget.max_bytes`. The cursor
binds a SHA-256 digest of the bound arguments and the complete normalized
selection; a changed query, scope, detail, answer policy, selected core, or
memory snapshot returns invalid cursor. Cursor lifetime is therefore the
lifetime of that byte-identical selection, with no hidden server session.

Combining each section's core once with `returned_on_page` items from every
page reconstructs the complete eligible proof without gaps or duplicates. Use
`detail: full` when “complete” must include structural and raw-detail sections.

## Accounting and complexity

`projection.sections` reports core, returned-on-page, eligible, and total
counts per section. `excluded_by_detail` distinguishes a fieldset choice from
pagination; `selection_omitted` reports an explicit `max_entries` cap.
`truncation` remains as a compatibility signal but no longer writes a
transport sentinel into `proof.missing`. Its page accounting separates items
returned on earlier pages from items still available after the current page;
the final continuation page tells the caller to combine pages rather than
pointing at a null cursor.

The gateway bounds oversized core prose with a logarithmic search, then
estimates each candidate item independently and serializes the final packet
once. It does not tokenize the complete shrinking packet once per omitted
item. Property regressions cover the 1,400 compact versus 1,800 balanced case,
byte-budget sweeps, nested detail, answer-in-context, byte-identical
determinism, cursor invalidation, full-proof reconstruction, and a 480-hop
complexity fixture.
