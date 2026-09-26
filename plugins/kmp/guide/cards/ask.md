Ask a semantic question with its exact about and a plain English `question`.
Preserve numbers, identifiers and acronyms; put the user's words in `asked_as`.

```json
{"about":"project:sample","question":"Why was cache retry chosen?","asked_as":"¿Por qué se eligió reintentar la caché?"}
```

Send this to `kmp_ask`. Expect stored citations or UNKNOWN, not a generated answer.
Inspect a claim you rely on. A question naming an identifier (`C6.4`, `#188`)
is answered only from memories that name it. `answer_status` says how it
settled; act on it once instead of asking again in other words:

- `answered`: answer from `because`. There is no continuation;
  `projection.more_on_request` counts proof past the cited core. Repeat with
  `budget.detail:"full"` only to audit it.
- `partial`: an enumeration; cite what was found and report `proof.missing`,
  already in the question's words. Complete `projection.next_action` if present.
- `unknown`: `unknown_reason` says what to do next.
  - `no_candidates`: nothing read here matched. Check the about; else say it is not stored.
  - `no_bearing`: memories matched words, none answers. Re-ask only with a term the user gave.
  - `anchor_absent_in_selection`: no memory read here names the identifier in `proof.missing`.
    Try its other spelling (`issue 188` for `#188`) or another about; never drop it.
  - `attribute_not_found`: memories name the identifier but not `proof.missing` beside it:
    the memory does not record that. Say so.
  - `out_of_window`: `proof.nearest_outside` lies outside the span; it is not
    evidence inside it. Widen the span only if the user's question allows.

An honest UNKNOWN can end the task; do not sweep the graph to force an answer.
For a semantic question with a time, supply `as_of` or `interval` and the right
axis. For “what changed”, “latest”, or a history, use temporal navigation.

More: `guide:kmp-agent:example:decision-history` and
`guide:kmp-agent:example:four-clocks`.
