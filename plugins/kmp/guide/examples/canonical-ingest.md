# Compile and replay a canonical memory packet

Use kmp_write_memory for ordinary semantic writing. kmp_ingest is the lower
boundary for a canonical packet whose refs, coordinates, relations and evidence
are already known. This example deliberately inspects a compiled preview to
teach that boundary; it does not add a preview to every ordinary write.

These are fictional sources interpreted by the LLM author. The JSON envelopes
are replay notation: send only `arguments`; copy every `${...}` binding from
its named native response. The isolated replay checks storage and navigation,
not independent LLM learning. Each write is a separate logical operation with
its own idempotency key. All timestamps are UTC; ingestion uses the actual
runtime clock. No source below states a validity interval.

```json
{"tool":"kmp_wake","save_as":"initial","expect_error":"not_found","arguments":{"about":"example:guide:canonical-ingest","budget":{"detail":"compact","max_bytes":20000}}}
```

## C1 — establish and inspect the packet dependency

C1: STORE-5 requires the journal to accept writes without a network.

```json
{"tool":"kmp_write_memory","save_as":"constraint","arguments":{"about":"example:guide:canonical-ingest","intent":"record_observation","actor":"guide-writer","source_kind":"human","idempotency_key":"guide-canonical-ingest:constraint:v1","scope":{"process":"capability-examples"},"labels":{"case": ["canonical-ingest"]},"occurred_at":"2026-09-02T09:01:00Z","observed_at":"2026-09-02T09:01:00Z","current":{"kind":"constraint","summary":"C1: STORE-5 requires the journal to accept writes without a network.","evidence":"C1: STORE-5 requires the journal to accept writes without a network."}}}
```

```json
{"tool":"kmp_inspect","save_as":"constraint_read","arguments":{"about":"example:guide:canonical-ingest","ref":"${constraint.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## D1 — compile a preview without committing it

D1: STORE-5 chooses SQLite because C1 requires journal writes without a network.

The author supplies the decision and its justified chosen_because relation.
The explicit dry run exposes the canonical packet for inspection. Copy its
returned memory, provenance, label policy and logical idempotency key; the
following lookup must show that D1 has not been written yet. The packet still
depends on C1 in this isolated store: it is not a portable bundle of every
referenced object. A recipient missing C1 needs that dependency first.

```json
{"tool":"kmp_write_memory","save_as":"preview","arguments":{"about":"example:guide:canonical-ingest","intent":"record_decision","actor":"guide-writer","source_kind":"human","idempotency_key":"guide-canonical-ingest:decision:v1","scope":{"process":"capability-examples"},"labels":{"case": ["canonical-ingest"]},"occurred_at":"2026-09-02T09:02:00Z","observed_at":"2026-09-02T09:03:00Z","current":{"kind":"decision","summary":"D1: STORE-5 chooses SQLite because C1 requires journal writes without a network.","evidence":"D1: STORE-5 chooses SQLite because C1 requires journal writes without a network."},"connect_to":[{"ref":"${constraint.generated_refs.0}","rel":"chosen_because","class":"motivational","confidence":"high","why":"D1 explicitly chooses SQLite for the offline-write requirement recorded by C1.","evidence":"D1: STORE-5 chooses SQLite because C1 requires journal writes without a network."}],"read_context":{"inspected_refs":["${constraint.generated_refs.0}"]},"options":{"dry_run":true}}}
```

```json
{"tool":"kmp_inspect","save_as":"not_committed","expect_error":"not_found","arguments":{"about":"example:guide:canonical-ingest","ref":"${preview.generated_refs.0}"}}
```

## Commit exactly the reviewed packet

Set only the preview execution flag dry_run to false. The semantic content is the returned packet, with no rewritten text, reconstructed ref or guessed clock. Replay with the same logical key; a repeated import must not add a second D1.

```json
{"tool":"kmp_ingest","save_as":"ingested","arguments":{"about":"${preview.ingest_preview.about}","idempotency_key":"${preview.ingest_preview.idempotency_key}","label_policy":"${preview.ingest_preview.label_policy}","memory":"${preview.ingest_preview.memory}","provenance":"${preview.ingest_preview.provenance}","dry_run":false}}
```

```json
{"tool":"kmp_ingest","save_as":"ingest_retry","arguments":{"about":"${preview.ingest_preview.about}","idempotency_key":"${preview.ingest_preview.idempotency_key}","label_policy":"${preview.ingest_preview.label_policy}","memory":"${preview.ingest_preview.memory}","provenance":"${preview.ingest_preview.provenance}","dry_run":false}}
```

```json
{"tool":"kmp_inspect","save_as":"preview_read","arguments":{"about":"example:guide:canonical-ingest","ref":"${preview.generated_refs.0}","budget":{"max_bytes":22000}}}
```

```json
{"tool":"kmp_trace","save_as":"packet_proof","arguments":{"about":"example:guide:canonical-ingest","from":"${preview.generated_refs.0}","to":"${constraint.generated_refs.0}","budget":{"max_bytes":20000}}}
```

## Read and review the limit

Inspect D1 and follow chosen_because to C1. The source text, event/observation times and link proof must match the compiled packet. D1 is a decision, not evidence that SQLite passed a test. A canonical replay checks representation and idempotency; validation by the writer does not remove the need for truthful source interpretation.

```json
{"tool":"kmp_forward","save_as":"records","arguments":{"about":"example:guide:canonical-ingest","from":{"time":"2026-09-02T08:00:00Z"},"axis":"observed","limit":{"entries":100},"budget":{"max_bytes":50000}}}
```

```json
{"tool":"kmp_view_open","save_as":"view","arguments":{"about":"example:guide:canonical-ingest"}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"frame","arguments":{"view_id":"${view.view_id}","expected_revision":"${view.view_revision}","idempotency_key":"guide-canonical-ingest:frame:v1","focus":{"time_range":{"from":"2026-09-02T08:55:00Z","to":"2026-09-02T10:00:00Z","axis":"observed"}},"selection":"${preview.generated_refs.0}","projection":{"semantic_zoom":"moment","labels":[{"key":"case","op":"in","values":["canonical-ingest"]}]},"explanation":"Review the selected source and its typed relations"}}
```

```json
{"tool":"kmp_view_get_state","save_as":"view_state","arguments":{"view_id":"${view.view_id}"}}
```
