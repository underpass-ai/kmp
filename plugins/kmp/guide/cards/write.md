Write separate facts, each with its source evidence and labels that make it
navigable. A label can hold several values; reuse a known key/value (the wake
label catalogue) before inventing another. Omit `actor` to be recorded under
the MCP client's name.

Source R1: “On 2026-09-01 at 09:00 UTC, we observed the cache fail.”

```json
{"about":"project:sample","idempotency_key":"sample-r1","observed_at":"2026-09-01T09:00:00Z","occurred_at":"2026-09-01T09:00:00Z","labels":{"component":["cache"],"source":["R1"]},"memories":[{"id":"failure","kind":"observation","summary":"The cache failed.","evidence":"R1: On 2026-09-01 at 09:00 UTC, we observed the cache fail."}]}
```

Expect `accepted:true`, refs and a receipt; copy the refs. Acceptance validates
the packet, not fidelity to the source. On rejection nothing is committed: read
every `feedback` item, repair from the source and retry. Replay an accepted
packet with its same key.

Clocks: omit `observed_at` for KMP's exact ingestion time; supply it only when
the source gives another. Omit unknown `occurred_at`, `valid_from` and
`valid_until`; observation is never onset. Root clocks are record defaults; a
record's null clears one. Keep a later check out of an earlier event's summary.

A link (`connect_to`, `relations`) is taught by topic `relations`. Rich links
return `status:needs_review`, `accepted:false`: nothing written. Review the
returned neighborhood, then send the returned `continuation` (alone, or with
the same `about`/`actor`) or correct the proposal. That review is yours, not a
request for human approval. Never invent a date, alias or proof to pass.

More: `guide:kmp-agent:example:first-decision`,
`guide:kmp-agent:example:event-separation`.
