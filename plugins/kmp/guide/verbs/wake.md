## Recover before you re-derive

Once the route is open and the work continues something earlier, the first
move is:

```
kmp_wake { about: "project:kmp" }
```

`kmp_wake` returns a compact wake packet: where the work stood, what was
decided, what is open. Call it **before** reading files to reconstruct
context you may already have stored. Abouts are stable ids, conventionally
`project:<name>` or `incident:<id>`.

A missing embedded about returns `not_found`, not a successful empty packet.
Distinguish deliberately new work from an unexpected missing about or wrong
store. A bounded wake can have no evidence within its selection while the
about still holds memory elsewhere in time.

Read `scope` with a historical wake. `scope.selection` names the proof,
causal spine, guardrails and resume cursor that stand on the requested time
selection. `scope.context` names the summary, state lines, open loops,
semantic next actions and label catalogue from the about context.
`context_time: unbounded` makes that context's wider time range explicit.
`scope.dimensions` returns the applied filters, with explicit mode and scope defaults.
The catalogue lists all labels of the retained entries, including labels
under other keys. `scope.request`
identifies the intent supplied by the caller.

For example, if C1 arrived at 08:00 and D1 at 09:00, an observed wake over
[08:00, 09:00) proves C1. D1 may appear in the context state; it is outside
that historical selection. Filtering component=warehouse also filters the
context to warehouse entries. If those entries arrived at 12:00, the same
08:00–09:00 window has empty proof and a null resume cursor while the context
and its labels still describe the warehouse. `proof.abouts_empty_in_selection`
identifies the empty historical selection. The budget-proof lesson executes
both cases.

Wake also hands back `resume_cursor` — the newest coordinate the packet
covers. Carry it, and the next question ("what changed since I looked?") is
one call:

```
kmp_forward { about: "project:kmp", from: <the resume_cursor> }
```

The kernel does not remember where each reader got to, deliberately: that
would be state about the reader rather than about the work. The bookmark is
yours to hold. `resume_cursor` is `null` when nothing in the packet carries a
temporal coordinate.

Wake also returns `labels`: the catalogue of the about — every label its
entries stand in, as `key` (the dimension kind: `task`, `agentic_process`,
`agentic_episode`, or any kind an ingest declared) and `value` (the scope
id), with how many entries stand in it and when one was last observed. Read
it before you write: a label that already exists is the one to reuse in
`labels`, and a filter on `dimensions` is only as good as the labels you know
exist, because a selection hides what it misses rather than ranking it low.
The current about comes first, then by use. The most used labels are the
first expansion the packet fills; the rest page after the causal spine, and
`truncation` says what did not fit — raise `budget.max_bytes` or follow the
cursor when you need the whole catalogue.

Execute `projection.next_action` as the next native call, copying its `tool`
and complete `arguments`. It preserves role, intent, dimensions, clock and
time selection. When `core_text_shortened` is true, the call omits
`page.cursor` and proposes a sufficient allowance: discard the partial
reconstruction and restart to recover the full core. Otherwise append only
the expansion beyond each section's `core` count. Stop when `next_action`
is null; `has_more=false` alone only finishes expansion. Detail and selection
caps still qualify coverage. If the allowance is unavailable, keep the result
partial. A cursor conflict supplies `feedback[].action` to start a fresh read;
never combine pages from different selections. The budget-proof lesson
executes both restoration and continuation against its stored sources.
