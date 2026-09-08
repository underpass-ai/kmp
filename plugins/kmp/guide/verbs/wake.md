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
`scope`, and a filter on `dimensions` is only as good as the labels you know
exist, because a selection hides what it misses rather than ranking it low.
The current about comes first, then by use. The most used labels are the
first expansion the packet fills; the rest page after the causal spine, and
`truncation` says what did not fit — raise `budget.max_bytes` or follow the
cursor when you need the whole catalogue.
