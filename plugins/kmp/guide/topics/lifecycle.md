## Undoing is a write, not a delete

There is no delete, on purpose: a memory that can be quietly edited is a
memory nobody can trust as evidence later. A decision that stopped being right
is reverted by recording that it was — a new entry saying what is true now,
connected with `supersedes` (evidential) carrying **why** the earlier one
stopped holding, and the evidence that showed it.

Write the new entry so it stands on its own. "Reverting X" is not a summary;
someone reading it a year from now may never see X. And never write a
supersession without a reason — a supersession with no why is a deletion with
extra steps, and it destroys the one thing the record was for.

What this buys is visible only if you show it: after reverting, the current
answer leads with the new state, and `kmp_rewind` to before the reversal
still returns the old decision with the evidence it had. Both are true, at
different times, and that is the whole point of keeping a log.

A replaced entry comes back **marked**. `kmp_wake` and `kmp_ask` carry
`proof.superseded`, one line per entry that a later one replaced:

```json
"superseded": [
  {
    "ref": "project:kmp:decision:usar-almacen-anterior",
    "superseded_by": "project:kmp:decision:usar-sqlite",
    "why": "two agent hosts need to share the store"
  }
]
```

It is kept apart from `proof.conflicts` on purpose. `contradicts` says two
entries disagree and both may still be live — the tension is the information.
`supersedes` says one replaced the other: no tension, a lifecycle, and the
older entry is history rather than advice. Read the older one as what was
true then, not as what to do now.

Validity expiry is a different lifecycle. Time-cursor moves on the explicit
`validity` axis exclude every interval whose `valid_until` is at or before the
instant. A `goto` returns only intervals that hold then
(`valid_from <= cursor < valid_until`); `rewind`, `near` and `forward` keep
their directional placement without reviving ended intervals. Ref-cursor
historical moves can deliberately return an entry whose interval ended; those
entries appear in `proof.expired` with their exclusive `valid_until`. Expiry
is kept apart from `proof.superseded` because a lease or constraint can simply
end without another entry replacing it.
