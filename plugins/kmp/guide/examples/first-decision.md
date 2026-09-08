# First decision: remember its reason and retrieve its proof

Two fictional source lines are enough for this first case:

- **C1:** START-1 requires the journal to accept writes without a network.
- **D1:** START-1 chooses SQLite because C1 requires journal writes without a network.

The author chooses constraint for C1 and decision for D1. The relation from
D1 to C1 is chosen_because/motivational: the source states the reason. It does
not prove that SQLite has passed an offline test.

Use an isolated teaching store. For this deliberately new about, the first
Wake returns `not_found`; recover real existing work before writing. Send only each envelope's arguments.
The `${...}` notation means copy the exact value returned by the named call.
These source timestamps are UTC; KMP assigns the real ingestion clock.

## 1. Recover and store the requirement

Read the requirement before writing a decision that refers to it.

```json
{"tool":"kmp_wake","save_as":"initial","expect_error":"not_found","arguments":{"about":"example:guide:first-decision","budget":{"detail":"compact","max_bytes":20000}}}
```

```json
{"tool":"kmp_write_memory","save_as":"constraint","arguments":{"about":"example:guide:first-decision","intent":"record_observation","actor":"guide-writer","source_kind":"human","idempotency_key":"guide-first-decision:constraint:v1","scope":{"process":"capability-examples"},"labels":{"case":"first-decision"},"occurred_at":"2026-09-02T09:01:00Z","observed_at":"2026-09-02T09:01:00Z","current":{"kind":"constraint","summary":"C1: START-1 requires the journal to accept writes without a network.","evidence":"C1: START-1 requires the journal to accept writes without a network."}}}
```

```json
{"tool":"kmp_inspect","save_as":"constraint_read","arguments":{"about":"example:guide:first-decision","ref":"${constraint.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## 2. Store the decision with its reason

why explains why D1 is linked to C1; evidence keeps the source that proves that reason. One logical write uses one idempotency key.

```json
{"tool":"kmp_write_memory","save_as":"decision","arguments":{"about":"example:guide:first-decision","intent":"record_decision","actor":"guide-writer","source_kind":"human","idempotency_key":"guide-first-decision:decision:v1","scope":{"process":"capability-examples"},"labels":{"case":"first-decision"},"occurred_at":"2026-09-02T09:02:00Z","observed_at":"2026-09-02T09:03:00Z","current":{"kind":"decision","summary":"D1: START-1 chooses SQLite because C1 requires journal writes without a network.","evidence":"D1: START-1 chooses SQLite because C1 requires journal writes without a network."},"connect_to":[{"ref":"${constraint.generated_refs.0}","rel":"chosen_because","class":"motivational","confidence":"high","why":"D1 explicitly chooses SQLite for the offline-write requirement recorded by C1.","evidence":"D1: START-1 chooses SQLite because C1 requires journal writes without a network."}],"read_context":{"inspected_refs":["${constraint.generated_refs.0}"]}}}
```

```json
{"tool":"kmp_inspect","save_as":"decision_read","arguments":{"about":"example:guide:first-decision","ref":"${decision.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## 3. Retrieve the answer and audit it

Ask should cite D1. Trace should contain D1→C1 with the exact reason and evidence above. The answer is the stored source, not a newly inferred database recommendation.

```json
{"tool":"kmp_ask","save_as":"answer","arguments":{"about":"example:guide:first-decision","question":"Why does START-1 choose SQLite for the journal?","budget":{"max_bytes":20000}}}
```

```json
{"tool":"kmp_trace","save_as":"packet_proof","arguments":{"about":"example:guide:first-decision","from":"${decision.generated_refs.0}","to":"${constraint.generated_refs.0}","budget":{"max_bytes":20000}}}
```

## 4. Check the same two memories visually

ChronoLoom should show constraint C1, decision D1 and their motivational relation. Select D1, then follow its outgoing chosen_because link to C1. A view move does not write another memory. The negative limit is explicit: these sources do not establish that an offline test passed.

```json
{"tool":"kmp_forward","save_as":"records","arguments":{"about":"example:guide:first-decision","from":{"time":"2026-09-02T08:00:00Z"},"axis":"observed","limit":{"entries":100},"budget":{"max_bytes":50000}}}
```

```json
{"tool":"kmp_view_open","save_as":"view","arguments":{"about":"example:guide:first-decision"}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"frame","arguments":{"view_id":"${view.view_id}","expected_revision":"${view.view_revision}","idempotency_key":"guide-first-decision:frame:v1","focus":{"time_range":{"from":"2026-09-02T08:55:00Z","to":"2026-09-02T10:00:00Z","axis":"observed"}},"selection":"${decision.generated_refs.0}","projection":{"semantic_zoom":"moment","labels":[{"key":"case","op":"in","values":["first-decision"]}]},"explanation":"Review the selected source and its typed relations"}}
```

```json
{"tool":"kmp_view_get_state","save_as":"view_state","arguments":{"view_id":"${view.view_id}"}}
```
