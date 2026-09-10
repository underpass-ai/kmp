Write separate facts, with source evidence and labels that make them navigable.
Pass your `context_id`; KMP supplies your persistent agent name as actor.
Without a context, supply actor explicitly as in this standalone example. A label can hold several values; reuse a known
key/value before inventing another. A shared account is not a person's alias.

Source R1: “On 2026-09-01 at 09:00 UTC, we observed the cache fail.”
One `kmp_write_memory` example:

```json
{"about":"project:sample","actor":"writer","idempotency_key":"sample-r1","observed_at":"2026-09-01T09:00:00Z","occurred_at":"2026-09-01T09:00:00Z","labels":{"component":["cache"],"source":["R1"]},"memories":[{"id":"failure","kind":"observation","summary":"The cache failed.","evidence":"R1: On 2026-09-01 at 09:00 UTC, we observed the cache fail."}]}
```

Expect `accepted:true`, generated refs and a receipt. Copy the refs. Acceptance
validates the packet, not fidelity to the source. Check the stored clocks and
evidence. On rejection no part of the batch is committed: read all feedback,
repair from the source, and retry. Replay an accepted packet with its same key.

Read returned `relations` as `from -> rel -> to` (`@id` uses `local_refs`).
In `connect_to`, the containing memory is `from`; `ref` is `to`:

- approval -> `authorizes` -> permitted action (not proof it ran);
- claim/outcome -> `verified_by` -> actual verifying check;
- exclusion record -> `excluded_from` -> total/set;
- corrected fact -> `corrects` -> earlier fact;
- replacement -> `supersedes` -> **entire** old memory, marking it SUPERSEDED.

For a partial correction, preserve unrelated facts; do not supersede their
whole report. Omit unknown `valid_from`/`valid_until`; observation is not onset.

For a decision, use its own source and connect it to the observation with the
relation's own why and evidence. Separate facts with different validity. Never
invent an onset, alias, permission or proof to satisfy the schema.

More: `guide:kmp-agent:example:first-decision`,
`guide:kmp-agent:example:semantic-batch`,
`guide:kmp-agent:example:dimensional-memberships`.
