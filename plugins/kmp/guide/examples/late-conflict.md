# Worked example: conflicting assignments and a record received late

Two reports name different sole operators for the same shift. Keep the claims
and their provenance. A later receipt is not a reason to choose a winner.
Only the signed record and the explicit review decision below resolve this
particular conflict. KMP preserves the interpretation and its proof; the LLM
must decide whether the sources justify it.

This is a fictional teaching history for `example:guide:late-conflict`.
In a writer evaluation, give the LLM only the sources available at each step,
the installed guide and the live tools. Do not provide future sources, reader
questions or expected answers. Replaying these authored calls exercises the
native contract; it does not measure a writer LLM learning to use KMP.

## Sources, clocks and choices

All times are UTC. SHIFT-6 is the September 1 interval [08:00, 12:00),
component journal, environment field. The reports assert the assignment for
that entire interval, so their `occurred_at` is the asserted shift start.
The signed record uses the same event time. This does not turn a report into
a verified fact. P1 and D1 are review decisions: their event time is when the
review took place, not the earlier shift. Each `observed_at` is the explicit
receipt time below; ingestion happens at execution and must not be backdated.
The shift interval stays in the evidence. These report memories have no
invented validity interval: they are claims about a completed shift, not
currently active assignments that expire when the shift ends.

| Source | Event time | Receipt time | Literal source |
| --- | --- | --- | --- |

| R1, dispatch report | 2026-09-01T08:00:00Z | 2026-09-02T09:00:00Z | R1 asserts that Maya was the sole responsible operator for SHIFT-6, journal in field, on September 1 from 08:00 to 12:00 UTC. R1 provides no signed duty record. |

| R2, service desk report | 2026-09-01T08:00:00Z | 2026-09-03T09:00:00Z | R2 asserts that Zoe was the sole responsible operator for SHIFT-6, journal in field, on September 1 from 08:00 to 12:00 UTC. R2 provides no signed duty record. |

| P1, review decision | 2026-09-03T10:00:00Z | 2026-09-03T10:00:00Z | P1: We cannot determine the sole responsible operator for SHIFT-6 from R1 and R2. Their Maya and Zoe assignments conflict for the same interval and component. Keep both claims and request the signed duty record; receiving R2 later does not establish its truth. |

| L1, signed duty record received late | 2026-09-01T08:00:00Z | 2026-09-05T09:00:00Z | L1, signed by Maya and Zoe: Maya alone held operational responsibility for SHIFT-6, journal in field, on September 1 from 08:00 to 12:00 UTC. Zoe was standby only. Both signatures attest the full interval. This record answers the request in P1; it was received on September 5 at 09:00 UTC. |

| D1, final review decision | 2026-09-05T09:05:00Z | 2026-09-05T09:05:00Z | D1: Confirm Maya as the sole responsible operator for SHIFT-6 on September 1 from 08:00 to 12:00 UTC, based on signed record L1. Replace the R2 assignment to Zoe and the unresolved P1 decision. Preserve R1, R2 and P1 as historical evidence; this conclusion became known on September 5 at 09:05 UTC. |

R1, R2 and L1 are observations of attributed source statements. P1 and D1 are
decisions actually stated by the reviewer; the writer is not inventing them.
`contradicts` relates the mutually exclusive assignments inside R1 and R2,
not the fact that both reports were received. Confidence in that contradiction
does not express confidence that either assignment is true.

The process and task identify this teaching workflow. Reuse the component,
environment and shift labels from the initial catalogue. They narrow retrieval
but establish neither operator identity nor truth. No `same_entity_as` is
justified between Maya and Zoe, and no synonym rule can choose the operator.

## Call notation and the first report

Read the brief `guide/AGENT.md`, the relevant indexed verbs, and the lifecycle,
relations and scope topics. Reuse those bodies while they remain in context.
Each JSON block names a tool, its arguments and a local `save_as` binding.
Send only `arguments`. `${first.generated_refs.0}` means copy the exact ref
returned earlier; never send a placeholder or build your own ref. The replay
resolves these bindings and records the actual native requests and responses.
An unexpected error or incomplete page stops it; continue a real partial page
with its cursor before claiming a complete result.

An isolated empty about returns the expected `not_found`. If it exists instead,
inspect the existing memory and labels; never bypass strict linking rules.

```json
{
  "tool": "kmp_wake",
  "save_as": "initial",
  "arguments": {
    "about": "example:guide:late-conflict",
    "budget": {
      "max_bytes": 20000,
      "detail": "full"
    }
  },
  "expect_error": "not_found"
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "first",
  "arguments": {
    "about": "example:guide:late-conflict",
    "actor": "guide-writer",
    "idempotency_key": "guide-late-conflict:first:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "environment": [
        "field"
      ],
      "shift": [
        "SHIFT-6"
      ],
      "agentic_process": [
        "duty-review"
      ],
      "task": [
        "responsibility-review"
      ]
    },
    "occurred_at": "2026-09-01T08:00:00Z",
    "observed_at": "2026-09-02T09:00:00Z",
    "source_kind": "human",
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "R1 asserts that Maya was the sole responsible operator for SHIFT-6, journal in field, on September 1 from 08:00 to 12:00 UTC. R1 provides no signed duty record.",
        "summary_en": "Who handled SHIFT-6? Dispatch report R1 names Maya as the only journal operator in field on September 1, 08:00 to 12:00 UTC, without a signed roster.",
        "evidence": "R1, dispatch report; received 2026-09-02T09:00:00Z: R1 asserts that Maya was the sole responsible operator for SHIFT-6, journal in field, on September 1 from 08:00 to 12:00 UTC. R1 provides no signed duty record."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "first_read",
  "arguments": {
    "about": "example:guide:late-conflict",
    "ref": "${first.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_view_open",
  "save_as": "opened",
  "arguments": {
    "about": "example:guide:late-conflict"
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "initial_frame",
  "arguments": {
    "expected_revision": "${opened.view_revision}",
    "idempotency_key": "guide-late-conflict:view-initial:v1",
    "explanation": "Inspect initial evidence for SHIFT-6 on the observed clock",
    "focus": {
      "time_range": {
        "axis": "observed",
        "from": "2026-09-01T00:00:00Z",
        "to": "2026-09-03T00:00:00Z"
      }
    },
    "projection": {
      "semantic_zoom": "moment",
      "dimensions": [
        "shift"
      ],
      "labels": [
        {
          "key": "shift",
          "op": "in",
          "values": [
            "SHIFT-6"
          ]
        }
      ],
      "relation_classes": [
        "evidential",
        "motivational"
      ]
    },
    "selection": "${first.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "initial_state",
  "arguments": {}
}
```

The initial view contains one attributed report. It is not a confirmed roster.

## Preserve disagreement and the provisional decision

R2 arrives a day later. The same component, shift and interval make these
assignments incompatible. Inspect the earlier target before linking, and
reuse the catalogue. Do not add `supersedes`: neither source withdraws R1.

```json
{
  "tool": "kmp_wake",
  "save_as": "before_second",
  "arguments": {
    "about": "example:guide:late-conflict",
    "budget": {
      "max_bytes": 120000,
      "detail": "full"
    }
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "second_review",
  "arguments": {
    "about": "example:guide:late-conflict",
    "actor": "guide-writer",
    "idempotency_key": "guide-late-conflict:second:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "environment": [
        "field"
      ],
      "shift": [
        "SHIFT-6"
      ],
      "agentic_process": [
        "duty-review"
      ],
      "task": [
        "responsibility-review"
      ]
    },
    "occurred_at": "2026-09-01T08:00:00Z",
    "observed_at": "2026-09-03T09:00:00Z",
    "source_kind": "human",
    "read_context": {
      "inspected_refs": [
        "${first.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "R2 asserts that Zoe was the sole responsible operator for SHIFT-6, journal in field, on September 1 from 08:00 to 12:00 UTC. R2 provides no signed duty record.",
        "summary_en": "Who handled SHIFT-6? Service desk report R2 names Zoe as the only journal operator in field on September 1, 08:00 to 12:00 UTC, without a signed roster.",
        "evidence": "R2, service desk report; received 2026-09-03T09:00:00Z: R2 asserts that Zoe was the sole responsible operator for SHIFT-6, journal in field, on September 1 from 08:00 to 12:00 UTC. R2 provides no signed duty record.",
        "connect_to": [
          {
            "ref": "${first.generated_refs.0}",
            "rel": "contradicts",
            "class": "evidential",
            "confidence": "high",
            "why": "R1 and R2 assign sole responsibility to different people for the same SHIFT-6 interval, component and environment; both claims cannot be true under those conditions.",
            "evidence": "R1 names Maya alone; R2 names Zoe alone. Both specify SHIFT-6, journal in field, September 1 from 08:00 to 12:00 UTC."
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
{"tool":"kmp_write_memory","save_as":"second","arguments":"${second_review.next_actions.0.arguments}"}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "second_read",
  "arguments": {
    "about": "example:guide:late-conflict",
    "ref": "${second.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "pending_review",
  "arguments": {
    "about": "example:guide:late-conflict",
    "actor": "guide-writer",
    "idempotency_key": "guide-late-conflict:pending:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "environment": [
        "field"
      ],
      "shift": [
        "SHIFT-6"
      ],
      "agentic_process": [
        "duty-review"
      ],
      "task": [
        "responsibility-review"
      ]
    },
    "occurred_at": "2026-09-03T10:00:00Z",
    "observed_at": "2026-09-03T10:00:00Z",
    "source_kind": "human",
    "read_context": {
      "inspected_refs": [
        "${first.generated_refs.0}",
        "${second.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "P1: We cannot determine the sole responsible operator for SHIFT-6 from R1 and R2. Their Maya and Zoe assignments conflict for the same interval and component. Keep both claims and request the signed duty record; receiving R2 later does not establish its truth.",
        "summary_en": "Which SHIFT-6 operator can be confirmed? P1 leaves Maya versus Zoe unresolved because R1 and R2 conflict for the same component and interval without signed proof. The reviewer keeps both reports and requests a signed roster instead of trusting the later receipt.",
        "evidence": "P1, review decision; received 2026-09-03T10:00:00Z: P1: We cannot determine the sole responsible operator for SHIFT-6 from R1 and R2. Their Maya and Zoe assignments conflict for the same interval and component. Keep both claims and request the signed duty record; receiving R2 later does not establish its truth.",
        "connect_to": [
          {
            "ref": "${first.generated_refs.0}",
            "rel": "chosen_because",
            "class": "motivational",
            "confidence": "high",
            "why": "P1 keeps the review unresolved because this unsupported assignment conflicts with the other report; it requests the missing signed record.",
            "evidence": "P1 explicitly cites R1 and R2, says their assignments conflict, and refuses to choose by receipt order."
          },
          {
            "ref": "${second.generated_refs.0}",
            "rel": "chosen_because",
            "class": "motivational",
            "confidence": "high",
            "why": "P1 keeps the review unresolved because this unsupported assignment conflicts with the other report; it requests the missing signed record.",
            "evidence": "P1 explicitly cites R1 and R2, says their assignments conflict, and refuses to choose by receipt order."
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
{"tool":"kmp_write_memory","save_as":"pending","arguments":"${pending_review.next_actions.0.arguments}"}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "pending_read",
  "arguments": {
    "about": "example:guide:late-conflict",
    "ref": "${pending.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_wake",
  "save_as": "unresolved",
  "arguments": {
    "about": "example:guide:late-conflict",
    "budget": {
      "max_bytes": 120000,
      "detail": "full"
    },
    "axis": "observed",
    "as_of": {
      "time": "2026-09-04T00:00:00Z"
    }
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "conflict_frame",
  "arguments": {
    "expected_revision": "${initial_state.view_revision}",
    "idempotency_key": "guide-late-conflict:view-conflict:v1",
    "explanation": "Inspect conflict evidence for SHIFT-6 on the observed clock",
    "focus": {
      "time_range": {
        "axis": "observed",
        "from": "2026-09-01T00:00:00Z",
        "to": "2026-09-04T00:00:00Z"
      }
    },
    "projection": {
      "semantic_zoom": "moment",
      "dimensions": [
        "shift"
      ],
      "labels": [
        {
          "key": "shift",
          "op": "in",
          "values": [
            "SHIFT-6"
          ]
        }
      ],
      "relation_classes": [
        "evidential",
        "motivational"
      ]
    },
    "selection": "${pending.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "conflict_state",
  "arguments": {}
}
```

Read `proof.conflicts`: R1 and R2 are still live. `proof.superseded` is empty.
The evidence-backed conclusion is: **no se puede determinar** quién tuvo la
responsabilidad con R1 y R2. P1 records exactly that uncertainty. A retrieval
response can return these relevant texts successfully; that does not mean
KMP has resolved the disagreement or must emit the token `UNKNOWN`.

In ChronoLoom select R2 to see its `contradicts` edge to R1, then P1 to see
the two motivational links. Relation class is not source certainty.

## Receive the late proof, then record the explicit resolution

L1 becomes available on September 5. Its signed assignment concerns September 1.
The `answers` edge is deliberately narrow: L1 supplies the record P1 requested;
it does not claim that the review caused the old shift or created the record.
Do not infer authenticity from the word “signed” in arbitrary real data:
the signatures and their matching shift scope are explicit premises here.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "record",
  "arguments": {
    "about": "example:guide:late-conflict",
    "actor": "guide-writer",
    "idempotency_key": "guide-late-conflict:record:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "environment": [
        "field"
      ],
      "shift": [
        "SHIFT-6"
      ],
      "agentic_process": [
        "duty-review"
      ],
      "task": [
        "responsibility-review"
      ]
    },
    "occurred_at": "2026-09-01T08:00:00Z",
    "observed_at": "2026-09-05T09:00:00Z",
    "source_kind": "human",
    "read_context": {
      "inspected_refs": [
        "${pending.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "L1, signed by Maya and Zoe: Maya alone held operational responsibility for SHIFT-6, journal in field, on September 1 from 08:00 to 12:00 UTC. Zoe was standby only. Both signatures attest the full interval. This record answers the request in P1; it was received on September 5 at 09:00 UTC.",
        "summary_en": "What proves SHIFT-6 responsibility? Signed record L1 identifies Maya as the only journal operator in field on September 1, 08:00 to 12:00 UTC, and Zoe as standby. Both attest the whole period. Received September 5 at 09:00 UTC, it answers P1.",
        "evidence": "L1, signed duty record received late; received 2026-09-05T09:00:00Z: L1, signed by Maya and Zoe: Maya alone held operational responsibility for SHIFT-6, journal in field, on September 1 from 08:00 to 12:00 UTC. Zoe was standby only. Both signatures attest the full interval. This record answers the request in P1; it was received on September 5 at 09:00 UTC.",
        "connect_to": [
          {
            "ref": "${pending.generated_refs.0}",
            "rel": "answers",
            "class": "evidential",
            "confidence": "high",
            "why": "L1 supplies the signed duty record explicitly requested by the unresolved P1 review.",
            "evidence": "L1 states that both Maya and Zoe signed the full SHIFT-6 interval and that this record answers the request in P1."
          }
        ]
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "record_read",
  "arguments": {
    "about": "example:guide:late-conflict",
    "ref": "${record.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

Inspect the replacement targets again before D1. An inspection shows current
objects and links, not a snapshot at the scene clock. The dated recall below
is what checks prior knowledge. D1 replaces only the wrong assignment R2 and
the unresolved review P1. R1 remains a historical report consistent with L1.
`verified_by` points to the signed source; each `supersedes` explains the
specific replacement. Do not delete either old object or change its clocks.

```json
{
  "tool": "kmp_inspect",
  "save_as": "second_before_resolution",
  "arguments": {
    "about": "example:guide:late-conflict",
    "ref": "${second.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "pending_before_resolution",
  "arguments": {
    "about": "example:guide:late-conflict",
    "ref": "${pending.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "resolved_review",
  "arguments": {
    "about": "example:guide:late-conflict",
    "actor": "guide-writer",
    "idempotency_key": "guide-late-conflict:resolved:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "environment": [
        "field"
      ],
      "shift": [
        "SHIFT-6"
      ],
      "agentic_process": [
        "duty-review"
      ],
      "task": [
        "responsibility-review"
      ]
    },
    "occurred_at": "2026-09-05T09:05:00Z",
    "observed_at": "2026-09-05T09:05:00Z",
    "source_kind": "human",
    "read_context": {
      "inspected_refs": [
        "${record.generated_refs.0}",
        "${second.generated_refs.0}",
        "${pending.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "D1: Confirm Maya as the sole responsible operator for SHIFT-6 on September 1 from 08:00 to 12:00 UTC, based on signed record L1. Replace the R2 assignment to Zoe and the unresolved P1 decision. Preserve R1, R2 and P1 as historical evidence; this conclusion became known on September 5 at 09:05 UTC.",
        "summary_en": "Who is confirmed for SHIFT-6? D1 uses signed L1 to confirm Maya for September 1, 08:00 to 12:00 UTC. The September 5, 09:05 UTC review replaces the Zoe assignment in R2 and the uncertainty in P1, retaining R1, R2 and P1 as historical evidence.",
        "evidence": "D1, final review decision; received 2026-09-05T09:05:00Z: D1: Confirm Maya as the sole responsible operator for SHIFT-6 on September 1 from 08:00 to 12:00 UTC, based on signed record L1. Replace the R2 assignment to Zoe and the unresolved P1 decision. Preserve R1, R2 and P1 as historical evidence; this conclusion became known on September 5 at 09:05 UTC.",
        "connect_to": [
          {
            "ref": "${record.generated_refs.0}",
            "rel": "verified_by",
            "class": "evidential",
            "confidence": "high",
            "why": "D1 confirms Maya for the exact disputed SHIFT-6 interval because L1 bears both signatures and identifies Zoe as standby only.",
            "evidence": "L1: Maya alone held operational responsibility for SHIFT-6, journal in field, on September 1 from 08:00 to 12:00 UTC. Zoe was standby only. Both signatures attest the full interval."
          },
          {
            "ref": "${second.generated_refs.0}",
            "rel": "supersedes",
            "class": "evidential",
            "confidence": "high",
            "why": "D1 explicitly replaces the R2 assignment to Zoe with the confirmed Maya assignment after receiving L1; R2 remains evidence of the earlier report.",
            "evidence": "D1: Replace the R2 assignment to Zoe and the unresolved P1 decision. Preserve R1, R2 and P1 as historical evidence."
          },
          {
            "ref": "${pending.generated_refs.0}",
            "rel": "supersedes",
            "class": "evidential",
            "confidence": "high",
            "why": "The signed record supplies the proof that P1 lacked, and D1 explicitly closes that unresolved review from September 5 at 09:05 UTC.",
            "evidence": "D1 confirms Maya based on signed record L1, replaces the unresolved P1 decision, and dates the known conclusion to September 5 at 09:05 UTC."
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
{"tool":"kmp_write_memory","save_as":"resolved","arguments":"${resolved_review.next_actions.0.arguments}"}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "resolved_read",
  "arguments": {
    "about": "example:guide:late-conflict",
    "ref": "${resolved.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

## Compare what was known with what happened

Repeat the earlier observed selection after the resolution has been written.
It must still return the contradiction without applying the later replacement.
Then read the later selection. These are two intentional historical selections,
not retries to force an answer. Preserve the proof and its declared clock.

```json
{
  "tool": "kmp_wake",
  "save_as": "unresolved_again",
  "arguments": {
    "about": "example:guide:late-conflict",
    "budget": {
      "max_bytes": 120000,
      "detail": "full"
    },
    "axis": "observed",
    "as_of": {
      "time": "2026-09-04T00:00:00Z"
    }
  }
}
```

```json
{
  "tool": "kmp_wake",
  "save_as": "resolved_recall",
  "arguments": {
    "about": "example:guide:late-conflict",
    "budget": {
      "max_bytes": 120000,
      "detail": "full"
    },
    "axis": "observed",
    "as_of": {
      "time": "2026-09-06T00:00:00Z"
    }
  }
}
```

```json
{
  "tool": "kmp_ask",
  "save_as": "ask_before",
  "arguments": {
    "about": "example:guide:late-conflict",
    "question": "Who was the sole responsible operator for SHIFT-6?",
    "asked_as": "¿Quién tuvo la responsabilidad única de SHIFT-6?",
    "axis": "observed",
    "as_of": {
      "time": "2026-09-04T00:00:00Z"
    },
    "budget": {
      "max_bytes": 120000,
      "detail": "full"
    }
  }
}
```

```json
{
  "tool": "kmp_ask",
  "save_as": "ask_after",
  "arguments": {
    "about": "example:guide:late-conflict",
    "question": "Who was the sole responsible operator for SHIFT-6?",
    "asked_as": "¿Quién tuvo la responsabilidad única de SHIFT-6?",
    "axis": "observed",
    "as_of": {
      "time": "2026-09-06T00:00:00Z"
    },
    "budget": {
      "max_bytes": 120000,
      "detail": "full"
    }
  }
}
```

Ask is an optional semantic entrance to this same proof. Inspect the returned
sources and the lifecycle markers; do not interpret its answer wrapper as a
new judgment. Before L1 and D1 the supported answer is uncertainty; afterward
it is Maya, with L1 and D1 as proof. Neither result proves anything outside
SHIFT-6. A genuinely semantic UNKNOWN follows the guide's selection limit;
do not use navigation to bypass it. The next calls have the separate, explicit
purpose of comparing the event timeline and known history.

```json
{
  "tool": "kmp_goto",
  "save_as": "shift_events",
  "arguments": {
    "about": "example:guide:late-conflict",
    "at": {
      "time": "2026-09-01T08:00:00Z"
    },
    "axis": "occurred",
    "dimensions": {
      "mode": "only",
      "include": [
        "shift"
      ],
      "scope_ids": [
        "SHIFT-6"
      ],
      "selectors": [
        {
          "key": "component",
          "op": "in",
          "values": [
            "journal"
          ]
        },
        {
          "key": "environment",
          "op": "in",
          "values": [
            "field"
          ]
        }
      ]
    },
    "limit": {
      "entries": 20
    },
    "budget": {
      "max_bytes": 50000
    }
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "receipt_history",
  "arguments": {
    "about": "example:guide:late-conflict",
    "from": {
      "time": "2026-09-02T00:00:00Z"
    },
    "axis": "observed",
    "dimensions": {
      "mode": "only",
      "include": [
        "shift"
      ],
      "scope_ids": [
        "SHIFT-6"
      ],
      "selectors": [
        {
          "key": "component",
          "op": "in",
          "values": [
            "journal"
          ]
        },
        {
          "key": "environment",
          "op": "in",
          "values": [
            "field"
          ]
        }
      ]
    },
    "limit": {
      "entries": 20
    },
    "budget": {
      "max_bytes": 50000
    }
  }
}
```

The event-time selection can contain the late record L1 beside R1 and R2 at
the asserted shift start. That is not evidence that anyone knew L1 on September 1.
The observed traversal places L1 on September 5 and D1 five minutes later.
Its cursor starts before the earliest receipt, so no inclusive boundary entry
is lost. Keep only the intended interval when using this pattern on a larger
history; follow all returned pages and deduplicate exact refs.

## Audit the links and old objects

Follow the directed edges. The original R2 text and every original coordinate
must survive unchanged. Its current inspection can now show the replacement
link; that extra link does not rewrite its receipt time.

```json
{
  "tool": "kmp_trace",
  "save_as": "disagreement",
  "arguments": {
    "about": "example:guide:late-conflict",
    "from": "${second.generated_refs.0}",
    "to": "${first.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "signed_proof",
  "arguments": {
    "about": "example:guide:late-conflict",
    "from": "${resolved.generated_refs.0}",
    "to": "${record.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "replacement",
  "arguments": {
    "about": "example:guide:late-conflict",
    "from": "${resolved.generated_refs.0}",
    "to": "${second.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "closed_review",
  "arguments": {
    "about": "example:guide:late-conflict",
    "from": "${resolved.generated_refs.0}",
    "to": "${pending.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "second_after",
  "arguments": {
    "about": "example:guide:late-conflict",
    "ref": "${second.generated_refs.0}",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

## Review the completed story in ChronoLoom

Frame the September 1 event slice, then return to the complete observed history.
The scene's date range selects visible memories. Its catalogue and selected
object inspector remain current; use the dated MCP proof for lifecycle at a
past instant. Moving the view never changes a subsequent retrieval's scope.
Use the returned revision; if a human moves first, read state and rebase.

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "event_frame",
  "arguments": {
    "expected_revision": "${conflict_state.view_revision}",
    "idempotency_key": "guide-late-conflict:view-event:v1",
    "explanation": "Inspect event evidence for SHIFT-6 on the occurred clock",
    "focus": {
      "time_range": {
        "axis": "occurred",
        "from": "2026-09-01T00:00:00Z",
        "to": "2026-09-02T00:00:00Z"
      }
    },
    "projection": {
      "semantic_zoom": "moment",
      "dimensions": [
        "shift"
      ],
      "labels": [
        {
          "key": "shift",
          "op": "in",
          "values": [
            "SHIFT-6"
          ]
        }
      ],
      "relation_classes": [
        "evidential",
        "motivational"
      ]
    },
    "selection": "${record.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "event_state",
  "arguments": {}
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "final_frame",
  "arguments": {
    "expected_revision": "${event_state.view_revision}",
    "idempotency_key": "guide-late-conflict:view-final:v1",
    "explanation": "Inspect final evidence for SHIFT-6 on the observed clock",
    "focus": {
      "time_range": {
        "axis": "observed",
        "from": "2026-09-01T00:00:00Z",
        "to": "2026-09-06T00:00:00Z"
      }
    },
    "projection": {
      "semantic_zoom": "moment",
      "dimensions": [
        "shift"
      ],
      "labels": [
        {
          "key": "shift",
          "op": "in",
          "values": [
            "SHIFT-6"
          ]
        }
      ],
      "relation_classes": [
        "evidential",
        "motivational"
      ]
    },
    "selection": "${resolved.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "final_state",
  "arguments": {}
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "view_state",
  "arguments": {}
}
```

The initial scene has one report; the conflict scene has two reports and P1;
the final observed scene has all five memories, with observations and decisions
and both evidential and motivational relations. Select D1 and follow
`verified_by` to L1 and `supersedes` to R2 or P1. Select R2 to inspect the
original disagreement. A counter can include more relations than the selection
mode draws; do not report its count as all arcs being visible simultaneously.

## Negative cases and limits

- R2 being later does not justify supersession. With only R1 and R2, retain
  both and report uncertainty. A high-confidence contradiction cannot choose
  a high-confidence winner.
- If the reports concern different shifts, components, intervals or meanings
  of responsibility, investigate that distinction before asserting conflict.
  The same names or opposite labels alone prove nothing.
- If L1 lacks an attestation for the whole disputed interval, or its provenance
  cannot be checked, keep the uncertainty. The presence of an evidence string
  is not automatic authentication, truth checking or authority resolution.
- Do not backdate D1 to the old shift or stamp L1's receipt as September 1.
  That would leak later knowledge into historical observed retrieval.
- `supersedes` is a declared lifecycle. A path to L1 alone does not replace R2;
  D1 explicitly authorizes that replacement in this scenario. Old assertions
  remain inspectable and the earlier conflict remains historically visible.
- There is no basis here for other shifts, person aliases, performance or
  monetary totals. Do not extrapolate from the labels or these proof paths.

No editorial CI gate is needed for this explanation. The standalone replay
checks the native writes, unchanged history, historical conflict, replacement
proof and view state. Writer learning still needs unseen sources and a blind
reader evaluation after the guide work is complete.
