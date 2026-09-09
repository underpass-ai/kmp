# Preference, decision and explicit policy change

A preference is a stated choice, not a compulsory constraint. A later policy
change does not rewrite that preference. This case teaches `preference`,
`record_delta`, the generated `semantic_delta`, and its `semantic_delta_from`
relation to the explicitly read old policy.

These are fictional sources interpreted by the LLM author. The JSON envelopes
are replay notation: send only `arguments`; copy every `${...}` binding from
its named native response. The isolated replay checks storage and navigation,
not independent LLM learning. Each write is a separate logical operation with
its own idempotency key. All timestamps are UTC; ingestion uses the actual
runtime clock. No source below states a validity interval.

```json
{"tool":"kmp_wake","save_as":"initial","expect_error":"not_found","arguments":{"about":"example:guide:preference-delta","budget":{"detail":"compact","max_bytes":20000}}}
```

## P1 — preference with a stated exception

P1: For NOTIFY-2, I prefer one daily digest of normal events. Urgent alerts may interrupt; this is a preference, not a mandatory limit.

Store preference because the speaker states a preference and permits exceptions. Do not turn the daily digest into a constraint that forbids urgent alerts.

```json
{"tool":"kmp_write_memory","save_as":"preference","arguments":{"about":"example:guide:preference-delta","intent":"record_observation","actor":"guide-writer","source_kind":"human","idempotency_key":"guide-preference-delta:preference:v1","scope":{"process":"capability-examples"},"labels":{"case": ["preference-delta"]},"occurred_at":"2026-09-02T09:01:00Z","observed_at":"2026-09-02T09:01:00Z","current":{"kind":"preference","summary":"P1: For NOTIFY-2, I prefer one daily digest of normal events. Urgent alerts may interrupt; this is a preference, not a mandatory limit.","evidence":"P1: For NOTIFY-2, I prefer one daily digest of normal events. Urgent alerts may interrupt; this is a preference, not a mandatory limit."}}}
```

```json
{"tool":"kmp_inspect","save_as":"preference_read","arguments":{"about":"example:guide:preference-delta","ref":"${preference.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## D1 — a decision motivated by the preference

D1: POL-2 adopts one daily digest for normal NOTIFY-2 events because of preference P1. Urgent alerts still interrupt.

The source states its reason, so chosen_because has motivational class. The decision is distinct from the preference it adopts.

```json
{"tool":"kmp_write_memory","save_as":"old_policy","arguments":{"about":"example:guide:preference-delta","intent":"record_observation","actor":"guide-writer","source_kind":"human","idempotency_key":"guide-preference-delta:old_policy:v1","scope":{"process":"capability-examples"},"labels":{"case": ["preference-delta"]},"occurred_at":"2026-09-02T09:02:00Z","observed_at":"2026-09-02T09:02:00Z","current":{"kind":"decision","summary":"D1: POL-2 adopts one daily digest for normal NOTIFY-2 events because of preference P1. Urgent alerts still interrupt.","evidence":"D1: POL-2 adopts one daily digest for normal NOTIFY-2 events because of preference P1. Urgent alerts still interrupt."},"connect_to":[{"ref":"${preference.generated_refs.0}","rel":"chosen_because","class":"motivational","confidence":"high","why":"D1 explicitly adopts the daily digest because of P1; P1 still permits urgent alerts.","evidence":"D1: POL-2 adopts one daily digest for normal NOTIFY-2 events because of preference P1. Urgent alerts still interrupt."}],"read_context":{"inspected_refs":["${preference.generated_refs.0}"]}}}
```

```json
{"tool":"kmp_inspect","save_as":"old_policy_read","arguments":{"about":"example:guide:preference-delta","ref":"${old_policy.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## D2 — record_delta creates an explicit before and after

D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate.

The first connect_to target is the inspected old policy D1. This matters:
record_delta generates a second entry of kind semantic_delta, links the new
current entry to that delta with updates_state/causal, and links the delta to
the first target with semantic_delta_from/causal. Supply from/to as states,
not as guessed refs. The canonical delta text is generated from those fields;
its evidence remains D2. Do not add those same two generated edges manually.
Supersedes points from the new decision to D1; P1 remains a preference.

```json
{"tool":"kmp_write_memory","save_as":"new_policy","arguments":{"about":"example:guide:preference-delta","intent":"record_delta","actor":"guide-writer","source_kind":"human","idempotency_key":"guide-preference-delta:new_policy:v1","scope":{"process":"capability-examples"},"labels":{"case": ["preference-delta"]},"occurred_at":"2026-09-02T09:03:00Z","observed_at":"2026-09-02T09:03:00Z","current":{"kind":"decision","summary":"D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate.","evidence":"D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate."},"connect_to":[{"ref":"${old_policy.generated_refs.0}","rel":"supersedes","class":"evidential","confidence":"high","why":"D2 explicitly replaces D1 for normal NOTIFY-2 events while retaining the urgent-alert exception.","evidence":"D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate."}],"read_context":{"inspected_refs":["${old_policy.generated_refs.0}"]},"semantic_delta":{"from":"Normal NOTIFY-2 events use one daily digest.","to":"Normal NOTIFY-2 events use immediate notices.","why":"D2 replaces the default because operations requested immediate updates; the urgent-alert policy is unchanged.","evidence":"D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate."}}}
```

```json
{"tool":"kmp_inspect","save_as":"new_policy_read","arguments":{"about":"example:guide:preference-delta","ref":"${new_policy.generated_refs.0}","budget":{"max_bytes":22000}}}
```

```json
{"tool":"kmp_inspect","save_as":"new_policy_delta_read","arguments":{"about":"example:guide:preference-delta","ref":"${new_policy.generated_refs.1}","budget":{"max_bytes":22000}}}
```

```json
{"tool":"kmp_trace","save_as":"delta_proof","arguments":{"about":"example:guide:preference-delta","from":"${new_policy.generated_refs.1}","to":"${old_policy.generated_refs.0}","budget":{"max_bytes":20000}}}
```

```json
{"tool":"kmp_wake","save_as":"past","arguments":{"about":"example:guide:preference-delta","as_of":{"time":"2026-09-02T09:02:30Z"},"axis":"observed","budget":{"detail":"compact","max_bytes":20000}}}
```

```json
{"tool":"kmp_wake","save_as":"present","arguments":{"about":"example:guide:preference-delta","as_of":{"time":"2026-09-02T09:04:00Z"},"axis":"observed","budget":{"detail":"compact","max_bytes":20000}}}
```

## Read and review the limit

The past packet must not contain D2 or its generated delta. The present packet marks D1 superseded, while P1 stays a preference. Inspect the delta text and its outgoing semantic_delta_from edge in ChronoLoom. The renderer may show a current record beside a historical scene: use the native bounded packet for historical claims.

```json
{"tool":"kmp_forward","save_as":"records","arguments":{"about":"example:guide:preference-delta","from":{"time":"2026-09-02T08:00:00Z"},"axis":"observed","limit":{"entries":100},"budget":{"max_bytes":50000}}}
```

```json
{"tool":"kmp_view_open","save_as":"view","arguments":{"about":"example:guide:preference-delta"}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"frame","arguments":{"view_id":"${view.view_id}","expected_revision":"${view.view_revision}","idempotency_key":"guide-preference-delta:frame:v1","focus":{"time_range":{"from":"2026-09-02T08:55:00Z","to":"2026-09-02T10:00:00Z","axis":"observed"}},"selection":"${new_policy.generated_refs.0}","projection":{"semantic_zoom":"moment","labels":[{"key":"case","op":"in","values":["preference-delta"]}]},"explanation":"Review the selected source and its typed relations"}}
```

```json
{"tool":"kmp_view_get_state","save_as":"view_state","arguments":{"view_id":"${view.view_id}"}}
```
