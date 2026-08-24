Undo a decision in memory the way an append-only log undoes things: by
recording that it was undone, never by removing it.

Tell me what you are about to revert and confirm the ref before writing —
reverting the wrong entry is not undoable by a second revert, it just adds a
second wrong entry to a permanent record.

`kernel_inspect` the target first: that it exists, what it said, and whether
something already supersedes it.

Then one `kernel_write_memory` with `intent: "record_decision"` and a
`connect_to` of `rel: "supersedes"`, `class: "evidential"`. The summary says
what is true now — not "reverting X", because the entry has to stand on its
own for whoever reads it later. The `why` on the relation says why the earlier
decision stopped being right; ask me for it if I have not said. A supersession
without a reason is a deletion with extra steps.

Then show me nothing was lost, rather than telling me:

1. `kernel_ask` about the subject — the answer now leads with the new state.
2. `kernel_rewind` from before the reversal — the old decision is still there,
   with the evidence it had.

Report both: "the memory now says X; as of <before> it said Y, and here is why
that changed."

Never look for a delete. There isn't one, deliberately. If I want an entry to
stop being findable rather than stop being current, that is a different
operation with different consequences — say so and stop.

Known limit worth saying if it matters: a superseded entry is not yet marked
as such in wake or ask output. The supersession shows on the relation, through
inspect, and by rewinding — which is why you show me both states.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
