# Last recorded is not verified today

Three fictional decisions are recorded on August 23. Later, one is replaced
and one reaches the end of its stated validity. The third receives nothing:
no check, no replacement and no end. A reader asking “is it still the
decision?” must distinguish these three states without inventing a fourth.

- **D1:** EXPORT-1 produces the nightly export as CSV because the finance importer only reads CSV.
- **R1:** RETRY-2 retries a failed upload 3 times.
- **L1:** LEASE-3 runs staging on host H1 from 2026-08-23 until 2026-09-30 00:00 UTC, when the hosting contract ends.
- **R2 (September 20):** RETRY-2 retries a failed upload 5 times, replacing R1, because the new upload gateway drops one request in four during deploys.

The writer chooses decision for each. R2 `supersedes` R1: the source replaces
the whole retry count and says why. L1 carries `valid_until` because its
source states the end. D1 carries neither: the source gives no end and nothing
later mentions it.

Use an isolated teaching store; the first Wake returns `not_found`. Send only
each envelope's arguments. `${...}` copies the exact value returned by the
named call. Source timestamps are UTC; KMP assigns the real ingestion clock,
which is later than every observation here and is not evidence of recency.

## 1. Record the three decisions

```json
{"tool":"kmp_wake","save_as":"initial","expect_error":"not_found","arguments":{"about":"example:guide:decision-currency","budget":{"detail":"compact","max_bytes":20000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "recorded",
  "arguments": {
    "about": "example:guide:decision-currency",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-decision-currency:recorded:v1",
    "labels": {"case": ["decision-currency"]},
    "occurred_at": "2026-08-23T10:00:00Z",
    "observed_at": "2026-08-23T10:00:00Z",
    "memories": [
      {"id": "d1", "kind": "decision",
       "summary": "D1: EXPORT-1 produces the nightly export as CSV because the finance importer only reads CSV.",
       "evidence": "Review log, 2026-08-23 10:00 UTC: D1 — EXPORT-1 nightly export is CSV; the finance importer only reads CSV."},
      {"id": "r1", "kind": "decision",
       "summary": "R1: RETRY-2 retries a failed upload 3 times.",
       "evidence": "Review log, 2026-08-23 10:00 UTC: R1 — RETRY-2 retries a failed upload 3 times."},
      {"id": "l1", "kind": "decision",
       "summary": "L1: LEASE-3 runs staging on host H1 from 2026-08-23 until 2026-09-30 00:00 UTC, when the hosting contract ends.",
       "evidence": "Review log, 2026-08-23 10:00 UTC: L1 — staging runs on H1 until 2026-09-30 00:00 UTC, when the hosting contract ends.",
       "valid_from": "2026-08-23T10:00:00Z", "valid_until": "2026-09-30T00:00:00Z"}
    ]
  }
}
```

## 2. Replace R1, with its reason

Read R1 before declaring that R2 replaces it.

```json
{"tool":"kmp_inspect","save_as":"retry_read","arguments":{"about":"example:guide:decision-currency","ref":"${recorded.local_refs.r1}","budget":{"max_bytes":22000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "replacement_review",
  "arguments": {
    "about": "example:guide:decision-currency",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-decision-currency:replacement:v1",
    "labels": {"case": ["decision-currency"]},
    "occurred_at": "2026-09-20T14:00:00Z",
    "observed_at": "2026-09-20T14:00:00Z",
    "read_context": {"inspected_refs": ["${recorded.local_refs.r1}"]},
    "memories": [
      {"id": "r2", "kind": "decision",
       "summary": "R2: RETRY-2 retries a failed upload 5 times, replacing R1, because the new upload gateway drops one request in four during deploys.",
       "evidence": "Review log, 2026-09-20 14:00 UTC: R2 replaces R1 — retry 5 times; the new upload gateway drops one request in four during deploys.",
       "connect_to": [
         {"ref": "${recorded.local_refs.r1}", "rel": "supersedes", "class": "evidential", "confidence": "high",
          "why": "R2 replaces the whole retry count of R1 because the new gateway drops more requests during deploys.",
          "evidence": "Review log, 2026-09-20 14:00 UTC: R2 replaces R1 — retry 5 times; the new upload gateway drops one request in four during deploys."}
       ]}
    ]
  }
}
```

This returns `needs_review` without writing. After reviewing the returned
context, resume the unchanged proposal:

```json
{"tool":"kmp_write_memory","save_as":"replacement","arguments":"${replacement_review.next_actions.0.arguments}"}
```

## 3. Read the three states

Wake marks R1 in `proof.superseded` with R2's reason and L1 in
`proof.expired` with its exclusive `valid_until`. D1 is in neither list.

```json
{"tool":"kmp_wake","save_as":"present","arguments":{"about":"example:guide:decision-currency","budget":{"max_bytes":60000}}}
```

The latest records, newest first. D1 keeps its own `observed_at`,
2026-08-23; no later entry mentions EXPORT-1.

```json
{"tool":"kmp_time","save_as":"latest","arguments":{"about":"example:guide:decision-currency","move":"rewind","from":{"time":"2026-10-04T00:00:00Z"},"axis":"observed","limit":{"entries":10},"budget":{"max_bytes":60000}}}
```

Stand before the replacement was known: R1 is not yet replaced and R2 is absent.

```json
{"tool":"kmp_time","save_as":"before_replacement","arguments":{"about":"example:guide:decision-currency","move":"goto","at":{"time":"2026-09-01T00:00:00Z"},"axis":"observed","budget":{"max_bytes":60000}}}
```

Stand after L1's end on the validity clock: L1 no longer holds and appears
only in `proof.expired`. D1 declares no validity interval, so this clock
says nothing about it; that is not evidence that D1 ended.

```json
{"tool":"kmp_time","save_as":"after_end","arguments":{"about":"example:guide:decision-currency","move":"goto","at":{"time":"2026-10-01T00:00:00Z"},"axis":"validity","budget":{"max_bytes":60000}}}
```

```json
{"tool":"kmp_ask","save_as":"answer","arguments":{"about":"example:guide:decision-currency","question":"Which format does the EXPORT-1 nightly export use?","budget":{"max_bytes":20000}}}
```

## 4. Write the answer

Asked on October 4 whether each decision is still the decision:

- **R1:** no. It was replaced by R2 on September 20 because the new gateway
  drops one request in four during deploys. R1 remains what was decided
  until then.
- **L1:** it ended on September 30, as its own source stated. Nothing
  replaced it; memory records no host for staging after that date.
- **D1:** the last recorded decision is CSV, observed 2026-08-23. Nothing
  recorded since verifies, replaces or ends it. Memory cannot say whether
  the export is still CSV today.

Do not answer D1 with “yes, it is still CSV”: the absence of a later record
is not confirmation. Do not answer “probably outdated”: six weeks of silence
is not evidence against it. The ingestion clock is today because the lesson
was replayed today; it is not a recent verification. To settle the present,
record a new observation of EXPORT-1 with its source.
