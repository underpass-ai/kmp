# ADR-016 — The consumer contract gains a record surface

Date: 2026-08-05. Status: accepted.

## Context

ADR-015 published `rehydration-memory-api` as the consumer contract, reads
only: `wake` and `ask`. Its doc said ingestion "has its own commands,
idempotency and provenance inside the kernel; a consumer reaches them through
the kernel's own surfaces" — which, for an embedding product, meant deep
imports of `rehydration-application` types. The first real consumer pipeline
(an embedding product applying its own durable outbox into the kernel) needs
to write through a published surface or the contract boundary is fiction.

## Decision

1. **A separate trait, not a bigger one.** `MemoryRecordApi` joins
   `MemoryRecallApi` in the same crate. `MemoryRecallApi` keeps its reads-only
   promise; a consumer that records asks for the write trait by name, and a
   reviewer sees the write dependency in its bounds. Both traits report the
   same `ApiCapabilities`; the capability name is `record`.

2. **Plain specs in, plain view out.** `MemoryRecordRequest` carries
   dimensions, entries, relations, evidence and provenance as owned plain
   types (`Memory*Spec`), deliberately smaller than the kernel's ingest
   command: no dry-run, no temporal coordinate refinements, and a relation
   subset (from, to, rel, class, why, confidence, sequence) until a consumer
   needs more. Relations joined the contract with their first consumer — the
   precedent candidate index publisher — exactly as this ADR intended; a
   relation may join entries of the record or refs the memory already holds,
   not evidence declared alongside, which the kernel resolves after
   relations. The answer is `RecordedMemoryView` — memory id, accepted
   counts, read-after-write readiness, warnings.

3. **Replay-idempotent by contract.** A retry with the same `idempotency_key`
   and the same content answers with the recorded outcome; the same key with
   different content is `Refused`. The kernel could not honour this before:
   the idempotency comparison hashed the *translated* changes, and translation
   consults existing state, so the same logical ingest translated differently
   after its own first apply and every replay read as a conflict. The event
   and the stored idempotent outcome now carry a **logical digest**, computed
   from the untranslated command (about + memory + provenance); when both
   sides hold one, it is the comparison. Records written before the field
   existed hold none and fall back to the old changes-hash comparison —
   stricter, never looser.

4. **Conflict is a refusal, not an outage.** On the record path,
   `PortError::Conflict` maps to `ApiError::Refused`: a reused key with
   different content will not succeed by being retried, and telling a
   consumer to wait would have it retry a contradiction forever. The recall
   path keeps its ports-as-environment reading.

## Consequences

- An at-least-once pipeline can apply the same record twice and store once,
  with no consumer-side string-matching on error reasons.
- The stored idempotency records grow an optional field; serde defaults keep
  every existing store readable, and old records simply keep the strict
  comparison.
- The conformance scenario that asserted "same-key retry must be rejected"
  asserted the deficiency, not a property; it now asserts the replay.
- `CONTRACT_VERSION` stays 1: capabilities were added, no existing meaning
  moved. Consumers detect `record` via `ApiCapabilities::supports`.
