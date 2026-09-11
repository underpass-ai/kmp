# Declared structure, membership and contributions

Structural links express a declared organization; they do not prove outcomes.
KMP already creates its about, coordinate and evidence bookkeeping links.
The three structural links here instead connect user-level memories described
by sources: a plan and phase, a scope and rule, and a group and member. Do not
copy generated contains_entry, records or has_dimension links manually.

These are fictional sources interpreted by the LLM author. The JSON envelopes
are replay notation: send only `arguments`; copy every `${...}` binding from
its named native response. The isolated replay checks storage and navigation,
not independent LLM learning. Each write is a separate logical operation with
its own idempotency key. All timestamps are UTC; ingestion uses the actual
runtime clock. No source below states a validity interval.

```json
{"tool":"kmp_wake","save_as":"initial","expect_error":"not_found","arguments":{"about":"example:guide:structure-parts","budget":{"detail":"compact","max_bytes":20000}}}
```

## PLAN1 — contains a planned phase

PH1: PLAN-4 includes a checksum verification phase named PH1; this is a planned phase, not a passing test.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "phase",
  "arguments": {
    "about": "example:guide:structure-parts",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-structure-parts:phase:v1",
    "labels": {
      "case": [
        "structure-parts"
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
        "kind": "observation",
        "summary": "PH1: PLAN-4 includes a checksum verification phase named PH1; this is a planned phase, not a passing test.",
        "evidence": "PH1: PLAN-4 includes a checksum verification phase named PH1; this is a planned phase, not a passing test."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"phase_read","arguments":{"about":"example:guide:structure-parts","ref":"${phase.generated_refs.0}","budget":{"max_bytes":22000}}}
```

PLAN1: PLAN-4 consists of the checksum verification phase PH1 followed by packaging; the plan says nothing about whether either phase has completed.

contains/structural points from the containing plan to the named phase. Stored membership does not imply execution or causality.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "plan",
  "arguments": {
    "about": "example:guide:structure-parts",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-structure-parts:plan:v1",
    "labels": {
      "case": [
        "structure-parts"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:02:00Z",
    "observed_at": "2026-09-02T09:02:00Z",
    "read_context": {
      "inspected_refs": [
        "${phase.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "PLAN1: PLAN-4 consists of the checksum verification phase PH1 followed by packaging; the plan says nothing about whether either phase has completed.",
        "evidence": "PLAN1: PLAN-4 consists of the checksum verification phase PH1 followed by packaging; the plan says nothing about whether either phase has completed.",
        "connect_to": [
          {
            "ref": "${phase.generated_refs.0}",
            "rel": "contains",
            "class": "structural",
            "confidence": "high",
            "why": "PLAN1 explicitly lists PH1 as a phase of PLAN-4; this records organization only.",
            "evidence": "PLAN1: PLAN-4 consists of the checksum verification phase PH1 followed by packaging; the plan says nothing about whether either phase has completed."
          }
        ]
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"plan_read","arguments":{"about":"example:guide:structure-parts","ref":"${plan.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## MEMBER1 — member_of an explicit group

GROUP1: REVIEW-4 is the group named for the PLAN-4 review.

The group declaration supplies no connection to PH1, so it is intentionally unlinked.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "group",
  "arguments": {
    "about": "example:guide:structure-parts",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-structure-parts:group:v1",
    "labels": {
      "case": [
        "structure-parts"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:03:00Z",
    "observed_at": "2026-09-02T09:03:00Z",
    "options": {
      "strict": false
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "GROUP1: REVIEW-4 is the group named for the PLAN-4 review.",
        "evidence": "GROUP1: REVIEW-4 is the group named for the PLAN-4 review."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"group_read","arguments":{"about":"example:guide:structure-parts","ref":"${group.generated_refs.0}","budget":{"max_bytes":22000}}}
```

MEMBER1: The PLAN-4 roster lists Ivo as a member of REVIEW-4; it does not give Ivo deployment approval rights.

member_of/structural points from the membership record toward the group. It is not authorizes.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "member",
  "arguments": {
    "about": "example:guide:structure-parts",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-structure-parts:member:v1",
    "labels": {
      "case": [
        "structure-parts"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:04:00Z",
    "observed_at": "2026-09-02T09:04:00Z",
    "read_context": {
      "inspected_refs": [
        "${group.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "MEMBER1: The PLAN-4 roster lists Ivo as a member of REVIEW-4; it does not give Ivo deployment approval rights.",
        "evidence": "MEMBER1: The PLAN-4 roster lists Ivo as a member of REVIEW-4; it does not give Ivo deployment approval rights.",
        "connect_to": [
          {
            "ref": "${group.generated_refs.0}",
            "rel": "member_of",
            "class": "structural",
            "confidence": "high",
            "why": "MEMBER1 names Ivo in the REVIEW-4 roster and states no approval rights.",
            "evidence": "MEMBER1: The PLAN-4 roster lists Ivo as a member of REVIEW-4; it does not give Ivo deployment approval rights."
          }
        ]
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"member_read","arguments":{"about":"example:guide:structure-parts","ref":"${member.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## RULE1 — scoped_to the named environment

ENV1: STAGE-4 names the staging environment; PROD-4 is a separate production environment.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "environment",
  "arguments": {
    "about": "example:guide:structure-parts",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-structure-parts:environment:v1",
    "labels": {
      "case": [
        "structure-parts"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:05:00Z",
    "observed_at": "2026-09-02T09:05:00Z",
    "options": {
      "strict": false
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "ENV1: STAGE-4 names the staging environment; PROD-4 is a separate production environment.",
        "evidence": "ENV1: STAGE-4 names the staging environment; PROD-4 is a separate production environment."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"environment_read","arguments":{"about":"example:guide:structure-parts","ref":"${environment.generated_refs.0}","budget":{"max_bytes":22000}}}
```

RULE1: Keep review artifacts for 30 days in STAGE-4. This rule applies only to staging and states no production retention policy.

scoped_to/structural points from the rule to its explicit scope memory. A case label organizes this lesson; the source is what limits the rule to staging.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "rule",
  "arguments": {
    "about": "example:guide:structure-parts",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-structure-parts:rule:v1",
    "labels": {
      "case": [
        "structure-parts"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:06:00Z",
    "observed_at": "2026-09-02T09:06:00Z",
    "read_context": {
      "inspected_refs": [
        "${environment.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "constraint",
        "summary": "RULE1: Keep review artifacts for 30 days in STAGE-4. This rule applies only to staging and states no production retention policy.",
        "evidence": "RULE1: Keep review artifacts for 30 days in STAGE-4. This rule applies only to staging and states no production retention policy.",
        "connect_to": [
          {
            "ref": "${environment.generated_refs.0}",
            "rel": "scoped_to",
            "class": "structural",
            "confidence": "high",
            "why": "RULE1 limits its 30-day retention requirement to STAGE-4 and excludes a claim about production.",
            "evidence": "RULE1: Keep review artifacts for 30 days in STAGE-4. This rule applies only to staging and states no production retention policy."
          }
        ]
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"rule_read","arguments":{"about":"example:guide:structure-parts","ref":"${rule.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## BUS1 — component_of and contributes_to are different claims

BREAKDOWN1: CLAIM-4 lists a transport line and a hotel line as its two cost components.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "breakdown",
  "arguments": {
    "about": "example:guide:structure-parts",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-structure-parts:breakdown:v1",
    "labels": {
      "case": [
        "structure-parts"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:07:00Z",
    "observed_at": "2026-09-02T09:07:00Z",
    "options": {
      "strict": false
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "BREAKDOWN1: CLAIM-4 lists a transport line and a hotel line as its two cost components.",
        "evidence": "BREAKDOWN1: CLAIM-4 lists a transport line and a hotel line as its two cost components."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"breakdown_read","arguments":{"about":"example:guide:structure-parts","ref":"${breakdown.generated_refs.0}","budget":{"max_bytes":22000}}}
```

TOTAL1: The checked CLAIM-4 settlement totals 55 EUR: bus receipt BUS1 is 20 EUR and hotel receipt HOTEL1 is 35 EUR. These are two distinct receipts; no other charges are included.

The settlement supplies the operands and checked total: 20 EUR + 35 EUR = 55 EUR. The author verifies this arithmetic; KMP does not calculate it from a tag. The total is a separate record from the cost breakdown.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "total",
  "arguments": {
    "about": "example:guide:structure-parts",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-structure-parts:total:v1",
    "labels": {
      "case": [
        "structure-parts"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:08:00Z",
    "observed_at": "2026-09-02T09:08:00Z",
    "read_context": {
      "inspected_refs": [
        "${breakdown.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "derived_value",
        "summary": "TOTAL1: The checked CLAIM-4 settlement totals 55 EUR: bus receipt BUS1 is 20 EUR and hotel receipt HOTEL1 is 35 EUR. These are two distinct receipts; no other charges are included.",
        "evidence": "TOTAL1: The checked CLAIM-4 settlement totals 55 EUR: bus receipt BUS1 is 20 EUR and hotel receipt HOTEL1 is 35 EUR. These are two distinct receipts; no other charges are included.",
        "connect_to": [
          {
            "ref": "${breakdown.generated_refs.0}",
            "rel": "uses_background",
            "class": "evidential",
            "confidence": "high",
            "why": "TOTAL1 settles the two components described by BREAKDOWN1; the literal settlement supplies amounts and exclusions.",
            "evidence": "TOTAL1: The checked CLAIM-4 settlement totals 55 EUR: bus receipt BUS1 is 20 EUR and hotel receipt HOTEL1 is 35 EUR. These are two distinct receipts; no other charges are included."
          }
        ]
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"total_read","arguments":{"about":"example:guide:structure-parts","ref":"${total.generated_refs.0}","budget":{"max_bytes":22000}}}
```

## BUS1 — qualifies_as a category under stated criteria

CATEGORY1: CLAIM-4 category transport means a public-transport receipt for a work journey, with amount and receipt identity.

The category definition is independent of a total; do not join them merely to avoid an unlinked write.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "category",
  "arguments": {
    "about": "example:guide:structure-parts",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-structure-parts:category:v1",
    "labels": {
      "case": [
        "structure-parts"
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
        "kind": "constraint",
        "summary": "CATEGORY1: CLAIM-4 category transport means a public-transport receipt for a work journey, with amount and receipt identity.",
        "evidence": "CATEGORY1: CLAIM-4 category transport means a public-transport receipt for a work journey, with amount and receipt identity."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"category_read","arguments":{"about":"example:guide:structure-parts","ref":"${category.generated_refs.0}","budget":{"max_bytes":22000}}}
```

BUS1: Receipt BUS1 records a public bus work journey for 20 EUR. CLAIM-4 includes it as the transport component and as a 20 EUR contribution to TOTAL1, and classifies it under CATEGORY1.

component_of/evidential records BUS1's place in BREAKDOWN1.
contributes_to/evidential records its actual inclusion in TOTAL1.
qualifies_as/evidential records the explicitly evidenced category membership:
public transport, work journey, amount and receipt identity. Each edge has
its own rationale. A category match alone would not prove reimbursement or
make an excluded receipt contribute to a total. Do not emit all three merely
because an expense carries a `money` label.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "bus_review",
  "arguments": {
    "about": "example:guide:structure-parts",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-structure-parts:bus:v1",
    "labels": {
      "case": [
        "structure-parts"
      ],
      "agentic_process": [
        "capability-examples"
      ]
    },
    "occurred_at": "2026-09-02T09:10:00Z",
    "observed_at": "2026-09-02T09:10:00Z",
    "read_context": {
      "inspected_refs": [
        "${breakdown.generated_refs.0}",
        "${total.generated_refs.0}",
        "${category.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "BUS1: Receipt BUS1 records a public bus work journey for 20 EUR. CLAIM-4 includes it as the transport component and as a 20 EUR contribution to TOTAL1, and classifies it under CATEGORY1.",
        "evidence": "BUS1: Receipt BUS1 records a public bus work journey for 20 EUR. CLAIM-4 includes it as the transport component and as a 20 EUR contribution to TOTAL1, and classifies it under CATEGORY1.",
        "connect_to": [
          {
            "ref": "${breakdown.generated_refs.0}",
            "rel": "component_of",
            "class": "evidential",
            "confidence": "high",
            "why": "BUS1 is the transport component explicitly listed in BREAKDOWN1.",
            "evidence": "BUS1: Receipt BUS1 records a public bus work journey for 20 EUR. CLAIM-4 includes it as the transport component and as a 20 EUR contribution to TOTAL1, and classifies it under CATEGORY1."
          },
          {
            "ref": "${total.generated_refs.0}",
            "rel": "contributes_to",
            "class": "evidential",
            "confidence": "high",
            "why": "BUS1 is explicitly included as 20 EUR in TOTAL1, whose source also identifies HOTEL1 at 35 EUR.",
            "evidence": "BUS1: Receipt BUS1 records a public bus work journey for 20 EUR. CLAIM-4 includes it as the transport component and as a 20 EUR contribution to TOTAL1, and classifies it under CATEGORY1. TOTAL1: The checked CLAIM-4 settlement totals 55 EUR: bus receipt BUS1 is 20 EUR and hotel receipt HOTEL1 is 35 EUR. These are two distinct receipts; no other charges are included."
          },
          {
            "ref": "${category.generated_refs.0}",
            "rel": "qualifies_as",
            "class": "evidential",
            "confidence": "high",
            "why": "BUS1 meets the CATEGORY1 criteria through its identified receipt, amount, public transport and work purpose.",
            "evidence": "BUS1: Receipt BUS1 records a public bus work journey for 20 EUR. CLAIM-4 includes it as the transport component and as a 20 EUR contribution to TOTAL1, and classifies it under CATEGORY1. CATEGORY1: CLAIM-4 category transport means a public-transport receipt for a work journey, with amount and receipt identity."
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
{"tool":"kmp_write_memory","save_as":"bus","arguments":"${bus_review.next_actions.0.arguments}"}
```

```json
{"tool":"kmp_inspect","save_as":"bus_read","arguments":{"about":"example:guide:structure-parts","ref":"${bus.generated_refs.0}","budget":{"max_bytes":22000}}}
```

```json
{"tool":"kmp_trace","save_as":"structure_proof","arguments":{"about":"example:guide:structure-parts","from":"${plan.generated_refs.0}","to":"${phase.generated_refs.0}","budget":{"max_bytes":20000}}}
```

```json
{"tool":"kmp_trace","save_as":"contribution_proof","arguments":{"about":"example:guide:structure-parts","from":"${bus.generated_refs.0}","to":"${total.generated_refs.0}","budget":{"max_bytes":20000}}}
```

## Read and review the limit

The inspector must preserve all six named relations and their endpoints. The scene emphasizes semantic relations; inspect the outgoing lists to check structural links rather than treating an absent explanatory arc as lost memory. The internal bookkeeping edges are different objects. No source grants Ivo approval, extends RULE1 to production or asserts PH1 passed.

```json
{"tool":"kmp_forward","save_as":"records","arguments":{"about":"example:guide:structure-parts","from":{"time":"2026-09-02T08:00:00Z"},"axis":"observed","limit":{"entries":100},"budget":{"max_bytes":50000}}}
```

```json
{"tool":"kmp_view_open","save_as":"view","arguments":{"about":"example:guide:structure-parts"}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"frame","arguments":{"view_id":"${view.view_id}","expected_revision":"${view.view_revision}","idempotency_key":"guide-structure-parts:frame:v1","focus":{"time_range":{"from":"2026-09-02T08:55:00Z","to":"2026-09-02T10:00:00Z","axis":"observed"}},"selection":"${bus.generated_refs.0}","projection":{"semantic_zoom":"moment","labels":[{"key":"case","op":"in","values":["structure-parts"]}]},"explanation":"Review the selected source and its typed relations"}}
```

```json
{"tool":"kmp_view_get_state","save_as":"view_state","arguments":{"view_id":"${view.view_id}"}}
```
