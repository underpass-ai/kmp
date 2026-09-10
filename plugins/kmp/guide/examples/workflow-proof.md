# Dependencies, authorization, causes and measured constraints

Each relation below has its own source. Authorization permits a named action;
a dependency says execution requires something; triggers requires evidence of
an actual cause. Numerical compliance and matching a selection requirement
are constraint relations, even though each carries evidence.

These are fictional sources interpreted by the LLM author. The JSON envelopes
are replay notation: send only `arguments`; copy every `${...}` binding from
its named native response. The isolated replay checks storage and navigation,
not independent LLM learning. Each write is a separate logical operation with
its own idempotency key. All timestamps are UTC; ingestion uses the actual
runtime clock. No source below states a validity interval.

```json
{"tool":"kmp_wake","save_as":"initial","expect_error":"not_found","arguments":{"about":"example:guide:workflow-proof","budget":{"detail":"compact","max_bytes":20000}}}
```

## R1 — a mandatory bound

R1: In LAT-3 the measured response latency must be at most 100 ms.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "limit",
  "arguments": {
    "about": "example:guide:workflow-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-workflow-proof:limit:v1",
    "labels": {
      "case": [
        "workflow-proof"
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
        "kind": "constraint",
        "summary": "R1: In LAT-3 the measured response latency must be at most 100 ms.",
        "evidence": "R1: In LAT-3 the measured response latency must be at most 100 ms."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"limit_read","arguments":{"about":"example:guide:workflow-proof","ref":"${limit.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## B1 — violates_constraint

B1: LAT-3 build A measured 150 ms for the response under the R1 test conditions.

150 ms exceeds the mandatory 100 ms bound. This proves failure for build A under these conditions; it does not identify its cause.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "slow_review",
  "arguments": {
    "about": "example:guide:workflow-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-workflow-proof:slow:v1",
    "labels": {
      "case": [
        "workflow-proof"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:02:00Z",
    "observed_at": "2026-09-02T09:02:00Z",
    "read_context": {
      "inspected_refs": [
        "${limit.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "B1: LAT-3 build A measured 150 ms for the response under the R1 test conditions.",
        "evidence": "B1: LAT-3 build A measured 150 ms for the response under the R1 test conditions.",
        "connect_to": [
          {
            "ref": "${limit.generated_refs.0}",
            "rel": "violates_constraint",
            "class": "constraint",
            "confidence": "high",
            "why": "B1 measures 150 ms under R1 conditions, exceeding the required maximum of 100 ms.",
            "evidence": "B1: LAT-3 build A measured 150 ms for the response under the R1 test conditions."
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
{"tool":"kmp_write_memory","save_as":"slow","arguments":"${slow_review.next_actions.0.arguments}"}
```

```json
{"tool":"kmp_inspect","save_as":"slow_read","arguments":{"about":"example:guide:workflow-proof","ref":"${slow.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## G1 — satisfies_constraint

G1: LAT-3 build B measured 80 ms for the response under the same R1 test conditions.

80 ms meets the 100 ms maximum for this measurement. Do not claim that every future request is below the bound.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "fast_review",
  "arguments": {
    "about": "example:guide:workflow-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-workflow-proof:fast:v1",
    "labels": {
      "case": [
        "workflow-proof"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:03:00Z",
    "observed_at": "2026-09-02T09:03:00Z",
    "read_context": {
      "inspected_refs": [
        "${limit.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "G1: LAT-3 build B measured 80 ms for the response under the same R1 test conditions.",
        "evidence": "G1: LAT-3 build B measured 80 ms for the response under the same R1 test conditions.",
        "connect_to": [
          {
            "ref": "${limit.generated_refs.0}",
            "rel": "satisfies_constraint",
            "class": "constraint",
            "confidence": "high",
            "why": "G1 measures 80 ms under R1 conditions, within the required maximum of 100 ms.",
            "evidence": "G1: LAT-3 build B measured 80 ms for the response under the same R1 test conditions."
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
{"tool":"kmp_write_memory","save_as":"fast","arguments":"${fast_review.next_actions.0.arguments}"}
```

```json
{"tool":"kmp_inspect","save_as":"fast_read","arguments":{"about":"example:guide:workflow-proof","ref":"${fast.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## R2 and C1 — matches_requirement

R2: For the LAT-3 review, select a candidate with local checksum verification and no network requirement.

R2 is an independent selection predicate; storing it does not create a causal link to R1. The explicit unlinked write preserves that limit.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "requirement",
  "arguments": {
    "about": "example:guide:workflow-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-workflow-proof:requirement:v1",
    "labels": {
      "case": [
        "workflow-proof"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:04:00Z",
    "observed_at": "2026-09-02T09:04:00Z",
    "options": {
      "strict": false
    },
    "memories": [
      {
        "id": "current",
        "kind": "constraint",
        "summary": "R2: For the LAT-3 review, select a candidate with local checksum verification and no network requirement.",
        "evidence": "R2: For the LAT-3 review, select a candidate with local checksum verification and no network requirement."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"requirement_read","arguments":{"about":"example:guide:workflow-proof","ref":"${requirement.generated_refs.0}","budget":{"max_bytes":22000}}}
```

C1: Candidate Cedar provides local checksum verification without network access; these two checked properties meet the LAT-3 review requirements R2.

Use matches_requirement/constraint for those named properties, not same_entity_as: a candidate is not the requirement itself.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "candidate_review",
  "arguments": {
    "about": "example:guide:workflow-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-workflow-proof:candidate:v1",
    "labels": {
      "case": [
        "workflow-proof"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:05:00Z",
    "observed_at": "2026-09-02T09:05:00Z",
    "read_context": {
      "inspected_refs": [
        "${requirement.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "C1: Candidate Cedar provides local checksum verification without network access; these two checked properties meet the LAT-3 review requirements R2.",
        "evidence": "C1: Candidate Cedar provides local checksum verification without network access; these two checked properties meet the LAT-3 review requirements R2.",
        "connect_to": [
          {
            "ref": "${requirement.generated_refs.0}",
            "rel": "matches_requirement",
            "class": "constraint",
            "confidence": "high",
            "why": "C1 has both properties required by R2: local checksum verification and operation without a network.",
            "evidence": "C1: Candidate Cedar provides local checksum verification without network access; these two checked properties meet the LAT-3 review requirements R2."
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
{"tool":"kmp_write_memory","save_as":"candidate","arguments":"${candidate_review.next_actions.0.arguments}"}
```

```json
{"tool":"kmp_inspect","save_as":"candidate_read","arguments":{"about":"example:guide:workflow-proof","ref":"${candidate.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## OP1 and A1 — authorizes

OP1: RLS-3 proposes deploying build B to staging after checksum verification. This is a planned operation; it has not run and no approval is recorded yet.

A planned step can be remembered as an observation of the plan. It is not a success_path. It is independent of the LAT-3 measurements unless a source links them.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "operation",
  "arguments": {
    "about": "example:guide:workflow-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-workflow-proof:operation:v1",
    "labels": {
      "case": [
        "workflow-proof"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:06:00Z",
    "observed_at": "2026-09-02T09:06:00Z",
    "options": {
      "strict": false
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "OP1: RLS-3 proposes deploying build B to staging after checksum verification. This is a planned operation; it has not run and no approval is recorded yet.",
        "evidence": "OP1: RLS-3 proposes deploying build B to staging after checksum verification. This is a planned operation; it has not run and no approval is recorded yet."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"operation_read","arguments":{"about":"example:guide:workflow-proof","ref":"${operation.generated_refs.0}","budget":{"max_bytes":22000}}}
```

A1: Marta explicitly approves OP1 for RLS-3 staging only, conditional on checksum verification. A1 does not approve production.

The approval is the source of authorizes and OP1 is its target. A1 permits a bounded action; the relation cannot enlarge that scope or prove execution.
Read the triple as `approval -> authorizes -> operation`. Reversing it says
the operation grants permission to its approval. A later execution can depend
on that approval, but `verified_by` must point to a check that actually verifies
the claimed outcome, not to permission or to the earlier failure.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "approval_review",
  "arguments": {
    "about": "example:guide:workflow-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-workflow-proof:approval:v1",
    "labels": {
      "case": [
        "workflow-proof"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:07:00Z",
    "observed_at": "2026-09-02T09:07:00Z",
    "read_context": {
      "inspected_refs": [
        "${operation.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "A1: Marta explicitly approves OP1 for RLS-3 staging only, conditional on checksum verification. A1 does not approve production.",
        "evidence": "A1: Marta explicitly approves OP1 for RLS-3 staging only, conditional on checksum verification. A1 does not approve production.",
        "connect_to": [
          {
            "ref": "${operation.generated_refs.0}",
            "rel": "authorizes",
            "class": "motivational",
            "confidence": "high",
            "why": "A1 explicitly permits OP1 in staging, with checksum verification required and production excluded.",
            "evidence": "A1: Marta explicitly approves OP1 for RLS-3 staging only, conditional on checksum verification. A1 does not approve production."
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
{"tool":"kmp_write_memory","save_as":"approval","arguments":"${approval_review.next_actions.0.arguments}"}
```

```json
{"tool":"kmp_inspect","save_as":"approval_read","arguments":{"about":"example:guide:workflow-proof","ref":"${approval.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## EX1 — depends_on

EX1: RLS-3 staging deployment OP1 completed after its checksum check. The execution log names A1 as the approval required before this deployment could run.

The execution source states a required dependency on A1, so depends_on is causal. Completion is separately evidenced by EX1; it is not inferred from the approval.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "execution_review",
  "arguments": {
    "about": "example:guide:workflow-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-workflow-proof:execution:v1",
    "labels": {
      "case": [
        "workflow-proof"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:08:00Z",
    "observed_at": "2026-09-02T09:08:00Z",
    "read_context": {
      "inspected_refs": [
        "${approval.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "success_path",
        "summary": "EX1: RLS-3 staging deployment OP1 completed after its checksum check. The execution log names A1 as the approval required before this deployment could run.",
        "evidence": "EX1: RLS-3 staging deployment OP1 completed after its checksum check. The execution log names A1 as the approval required before this deployment could run.",
        "connect_to": [
          {
            "ref": "${approval.generated_refs.0}",
            "rel": "depends_on",
            "class": "causal",
            "confidence": "high",
            "why": "EX1 records A1 as the required approval for this completed staging deployment.",
            "evidence": "EX1: RLS-3 staging deployment OP1 completed after its checksum check. The execution log names A1 as the approval required before this deployment could run."
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
{"tool":"kmp_write_memory","save_as":"execution","arguments":"${execution_review.next_actions.0.arguments}"}
```

```json
{"tool":"kmp_inspect","save_as":"execution_read","arguments":{"about":"example:guide:workflow-proof","ref":"${execution.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## AL1 and S1 — triggers

AL1: The LAT-3 alert log records that the latency alarm fired for sample S1.

Record the alert first because its notice arrived first. No source has yet established a cause from any previous measurement, so this is intentionally unlinked.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "alert",
  "arguments": {
    "about": "example:guide:workflow-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-workflow-proof:alert:v1",
    "labels": {
      "case": [
        "workflow-proof"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:09:00Z",
    "observed_at": "2026-09-02T09:09:00Z",
    "options": {
      "strict": false
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "AL1: The LAT-3 alert log records that the latency alarm fired for sample S1.",
        "evidence": "AL1: The LAT-3 alert log records that the latency alarm fired for sample S1."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"alert_read","arguments":{"about":"example:guide:workflow-proof","ref":"${alert.generated_refs.0}","budget":{"max_bytes":22000}}}
```

S1: The LAT-3 rule log identifies sample S1 at 160 ms as the threshold event that triggered alert AL1 when the 100 ms boundary was exceeded.

The later received rule log proves S1 triggered AL1. A shared timestamp alone would justify at most sequence, not triggers. Here the source names the exact alert and executed rule.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "sample_review",
  "arguments": {
    "about": "example:guide:workflow-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-workflow-proof:sample:v1",
    "labels": {
      "case": [
        "workflow-proof"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:08:30Z",
    "observed_at": "2026-09-02T09:10:00Z",
    "read_context": {
      "inspected_refs": [
        "${alert.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "S1: The LAT-3 rule log identifies sample S1 at 160 ms as the threshold event that triggered alert AL1 when the 100 ms boundary was exceeded.",
        "evidence": "S1: The LAT-3 rule log identifies sample S1 at 160 ms as the threshold event that triggered alert AL1 when the 100 ms boundary was exceeded.",
        "connect_to": [
          {
            "ref": "${alert.generated_refs.0}",
            "rel": "triggers",
            "class": "causal",
            "confidence": "high",
            "why": "The S1 rule log identifies this threshold event as the cause of AL1, not merely an earlier sample.",
            "evidence": "S1: The LAT-3 rule log identifies sample S1 at 160 ms as the threshold event that triggered alert AL1 when the 100 ms boundary was exceeded."
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
{"tool":"kmp_write_memory","save_as":"sample","arguments":"${sample_review.next_actions.0.arguments}"}
```

```json
{"tool":"kmp_inspect","save_as":"sample_read","arguments":{"about":"example:guide:workflow-proof","ref":"${sample.generated_refs.0}","budget":{"max_bytes":22000}}}
```

```json
{"tool":"kmp_trace","save_as":"authorization_proof","arguments":{"about":"example:guide:workflow-proof","from":"${approval.generated_refs.0}","to":"${operation.generated_refs.0}","budget":{"max_bytes":20000}}}
```

```json
{"tool":"kmp_trace","save_as":"cause_proof","arguments":{"about":"example:guide:workflow-proof","from":"${sample.generated_refs.0}","to":"${alert.generated_refs.0}","budget":{"max_bytes":20000}}}
```

## Read and review the limit

Inspect B1 and G1 against the same R1, then A1→OP1, EX1→A1 and S1→AL1. Their relation classes are constraint, motivational and causal respectively. No record authorizes production or attributes B1 to a cause. A positive measurement is scoped to its test conditions.

```json
{"tool":"kmp_forward","save_as":"records","arguments":{"about":"example:guide:workflow-proof","from":{"time":"2026-09-02T08:00:00Z"},"axis":"observed","limit":{"entries":100},"budget":{"max_bytes":50000}}}
```

```json
{"tool":"kmp_view_open","save_as":"view","arguments":{"about":"example:guide:workflow-proof"}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"frame","arguments":{"view_id":"${view.view_id}","expected_revision":"${view.view_revision}","idempotency_key":"guide-workflow-proof:frame:v1","focus":{"time_range":{"from":"2026-09-02T08:55:00Z","to":"2026-09-02T10:00:00Z","axis":"observed"}},"selection":"${approval.generated_refs.0}","projection":{"semantic_zoom":"moment","labels":[{"key":"case","op":"in","values":["workflow-proof"]}]},"explanation":"Review the selected source and its typed relations"}}
```

```json
{"tool":"kmp_view_get_state","save_as":"view_state","arguments":{"view_id":"${view.view_id}"}}
```
