# Preference, decision and explicit policy change

A preference is a stated choice, not a compulsory constraint. A later policy
change does not rewrite that preference. This case teaches `preference`,
an explicit `semantic_delta` member and its `semantic_delta_from`
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
{
  "tool": "kmp_write_memory",
  "save_as": "preference",
  "arguments": {
    "about": "example:guide:preference-delta",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-preference-delta:preference:v1",
    "labels": {
      "case": [
        "preference-delta"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:01:00Z",
    "observed_at": "2026-09-02T09:01:00Z",
    "memories": [
      {
        "id": "current",
        "kind": "preference",
        "summary": "P1: For NOTIFY-2, I prefer one daily digest of normal events. Urgent alerts may interrupt; this is a preference, not a mandatory limit.",
        "evidence": "P1: For NOTIFY-2, I prefer one daily digest of normal events. Urgent alerts may interrupt; this is a preference, not a mandatory limit."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"preference_read","arguments":{"about":"example:guide:preference-delta","ref":"${preference.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## D1 — a decision motivated by the preference

D1: POL-2 adopts one daily digest for normal NOTIFY-2 events because of preference P1. Urgent alerts still interrupt.

The source states its reason, so chosen_because has motivational class. The decision is distinct from the preference it adopts.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "old_policy_review",
  "arguments": {
    "about": "example:guide:preference-delta",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-preference-delta:old_policy:v1",
    "labels": {
      "case": [
        "preference-delta"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:02:00Z",
    "observed_at": "2026-09-02T09:02:00Z",
    "read_context": {
      "inspected_refs": [
        "${preference.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "D1: POL-2 adopts one daily digest for normal NOTIFY-2 events because of preference P1. Urgent alerts still interrupt.",
        "evidence": "D1: POL-2 adopts one daily digest for normal NOTIFY-2 events because of preference P1. Urgent alerts still interrupt.",
        "connect_to": [
          {
            "ref": "${preference.generated_refs.0}",
            "rel": "chosen_because",
            "class": "motivational",
            "confidence": "high",
            "why": "D1 explicitly adopts the daily digest because of P1; P1 still permits urgent alerts.",
            "evidence": "D1: POL-2 adopts one daily digest for normal NOTIFY-2 events because of preference P1. Urgent alerts still interrupt."
          }
        ]
      }
    ]
  }
}
```

This returns `needs_review` without writing. Review the stored/proposed context
and link directions against the sources above; expand relevant omissions.
Resume this unchanged teaching proposal only after that review:

```json
{"tool":"kmp_write_memory","save_as":"old_policy","arguments":"${old_policy_review.next_actions.0.arguments}"}
```

```json
{"tool":"kmp_inspect","save_as":"old_policy_read","arguments":{"about":"example:guide:preference-delta","ref":"${old_policy.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## D2 — declare the before and after in the packet

D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate.

Declare the decision and the change as two members of memories. Write the
before, after and reason in the semantic_delta member's summary, with D2 as
its evidence. Declare both links explicitly in connect_to: the new decision
uses updates_state/causal toward @semantic_delta, and the delta uses
semantic_delta_from/causal toward the inspected old policy D1. KMP resolves
the local reference and validates the declared packet; it does not author
this delta text or infer these two relations. Supersedes points from the new
decision to D1; P1 remains a preference.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "new_policy_review",
  "arguments": {
    "about": "example:guide:preference-delta",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-preference-delta:new_policy:v1",
    "labels": {
      "case": [
        "preference-delta"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:03:00Z",
    "observed_at": "2026-09-02T09:03:00Z",
    "read_context": {
      "inspected_refs": [
        "${old_policy.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate.",
        "evidence": "D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate.",
        "connect_to": [
          {
            "ref": "${old_policy.generated_refs.0}",
            "rel": "supersedes",
            "class": "evidential",
            "confidence": "high",
            "why": "D2 explicitly replaces D1 for normal NOTIFY-2 events while retaining the urgent-alert exception.",
            "evidence": "D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate."
          },
          {
            "ref": "@semantic_delta",
            "rel": "updates_state",
            "class": "causal",
            "why": "D2 replaces the default because operations requested immediate updates; the urgent-alert policy is unchanged.",
            "evidence": "D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate.",
            "confidence": "high"
          }
        ]
      },
      {
        "id": "semantic_delta",
        "kind": "semantic_delta",
        "summary": "From: Normal NOTIFY-2 events use one daily digest.\nTo: Normal NOTIFY-2 events use immediate notices.\nWhy: D2 replaces the default because operations requested immediate updates; the urgent-alert policy is unchanged.",
        "evidence": "D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate.",
        "connect_to": [
          {
            "ref": "${old_policy.generated_refs.0}",
            "rel": "semantic_delta_from",
            "class": "causal",
            "why": "D2 replaces the default because operations requested immediate updates; the urgent-alert policy is unchanged.",
            "evidence": "D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate.",
            "confidence": "high"
          }
        ]
      }
    ]
  }
}
```

This returns `needs_review` without writing. Review the stored/proposed context
and link directions against the sources above; expand relevant omissions.
Resume this unchanged teaching proposal only after that review:

```json
{"tool":"kmp_write_memory","save_as":"new_policy","arguments":"${new_policy_review.next_actions.0.arguments}"}
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

The past packet must not contain D2 or its declared delta. The present packet marks D1 superseded, while P1 stays a preference. Inspect the delta text and its outgoing semantic_delta_from edge in ChronoLoom. The renderer may show a current record beside a historical scene: use the native bounded packet for historical claims.

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
