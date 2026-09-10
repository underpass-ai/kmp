Ask a semantic question with its exact about and a plain English `question`.
Preserve numbers, identifiers and acronyms; put the user's words in `asked_as`.

```json
{"about":"project:sample","question":"Why was cache retry chosen?","asked_as":"¿Por qué se eligió reintentar la caché?"}
```

Send this to `kmp_ask`. Expect stored citations or UNKNOWN, not a generated answer.
Complete `projection.next_action` before judging partial proof. Inspect a claim
you rely on. An honest UNKNOWN can end the task; do not sweep the graph merely
to force a different answer.

For a semantic question with a time, supply `as_of` or `interval` and the right
axis. For “what changed”, “latest”, or a history, use temporal navigation.
`proof.missing`, `nearest_outside` and explicit omissions say what the kernel
actually established. An outside match is not evidence inside the requested span.

More: `guide:kmp-agent:example:decision-history` and
`guide:kmp-agent:example:four-clocks`.
