## Share the view, not just the answer

A request such as “Show me the memory behind this decision” has two lanes.
First retrieve the memory that answers the question and inspect or trace the
evidence the answer relies on. Then open that same about once with
`kmp_view_open` and move the shared ChronoLoom view with
`kmp_view_apply_intent`. The visual lane frames the retrieved evidence; it does
not replace retrieval, and the view is not itself proof.

Declare meaning, never screen coordinates: clock, range, semantic zoom,
dimensions, relation classes, selection or trace. Pass the view's current
`revision` as `expected_revision` on every intent. If another participant has
moved the view and the revision conflicts, get the current state and rebase
the intent on it instead of retrying blind. Do not call `kmp_view_open` again
to navigate an already-open view.

Before continuing the conversation after a shared-view interaction, call
`kmp_view_get_state`. The person may have clicked, filtered, panned or undone
the agent's last intent, and the next answer must continue from what both are
actually looking at.

## The viewer, when it offers itself

`kmp_write_memory` sometimes comes back with a `viewer` block:

```json
"viewer": {
  "url": "http://127.0.0.1:7317/",
  "tell_the_user": "Their memory is now a graph they can open: …"
}
```

That is the session saying it is already serving this store as a graph — the
same abouts, the same typed relations, the same evidence, in a window instead
of in JSON. It appears on the **first memory a session writes**, because that
is the first moment there is anything to look at, and it does not appear
again.

Pass the link on when it appears. It is the one thing a human can look at
without learning any of this, and most people never find out it exists.

VERB show. Retrieval comes first; then use kmp_view_open once, kmp_view_apply_intent to declare semantic focus, and kmp_view_get_state before continuing. Do not send pixels or reopen to navigate. Minimum input: exact about, then a content-derived intent key and current view revision. Expect: a shared ChronoLoom state and capability URL. Next: rebase on a revision conflict or continue from the person's latest state. ChronoLoom renders one plane per about, with a real 3D orbit or flat camera and one exact time window. projection.abouts names at most five additional abouts to compare. Selection, focus refs, trace endpoints, dimensions and label values are checked against the primary and those additional abouts. A selection-only intent retains existing layers; supplying projection replaces the whole projection, so repeat abouts when those layers should remain. Comparing abouts does not declare a semantic link between their memories. projection.labels uses ordinary label predicates, not privileged lanes. Human takeover preserves the frame, advances its revision and retains undo of the semantic move. Read the new revision before continuing; the actor panel describes provenance, not an exclusive lock or a live-agent connection.
