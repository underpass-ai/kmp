# Worked example: resume memory and share a changing view

HND-9 is a fictional handoff. A writer records a constraint, a decision and
one review request worth remembering; it does not archive a conversation.
A fresh reader recovers that work from the same store. A person then changes
the clock and selection in ChronoLoom. The reader must observe that change,
audit the proof and preserve the person's frame when it resumes.

This is an authored teaching replay. Two sequential MCP processes verify
persistence and client separation; neither is an independent LLM learning
test. The LLM author interprets sources and chooses relations. KMP validates
and preserves those choices. In a later writer evaluation, provide only the
guide and sources available at that stage, with reader questions kept separate.

## Sources in receipt order

These are separate event and receipt clocks on September 1, 2026 UTC.
Ingestion uses the real replay clock. No source states a validity interval,
so none is invented. The first three sources are available to the writer;
F1 arrives only after the shared review. `component=journal` and `review=HND-9`
organize this declared teaching context, not an inferred identity.

| Source | Occurred | Observed | Literal source |
| --- | --- | --- | --- |
| C1 | 09:00 | 09:05 | C1: The HND-9 journal must work offline. Ledger records must remain on this device. |
| D1 | 09:10 | 09:15 | D1: We choose SQLite for the HND-9 journal because its device-local storage can satisfy the offline requirement C1. |
| H1 | 09:20 | 09:25 | H1: Please confirm D1 is the selection to review for HND-9. The offline restore test is still pending; no test report is available. |
| F1 | 10:00 | 10:05 | F1: In answer to H1, I confirm D1 is the selection to review for HND-9. This confirms the selection only; the offline restore test is still pending. |

C1 is a constraint and D1 a decision. Their chosen_because edge has
motivational class because D1 states its reason. H1 is a turn: a specific
review request with a pending action needed for continuation, not a transcript.
Its uses_background edge names the choice under review without claiming that
SQLite has passed a test. F1 is feedback with answers toward H1 and
confirms_selection toward D1. A screen selection alone does not supply F1:
only the later explicit source justifies writing that feedback.

Each JSON envelope gives a tool, arguments and a local save_as binding.
Send only arguments to the MCP tool. Copy `${name...}` from the corresponding
returned value; refs are opaque. `new_session` and `human_checkpoint` are
replay annotations, not MCP arguments. A new session reads its guide entry,
relevant topics and tool guidance again; it has no prior guide context.
Within each session those bodies are read once and reused.

## Writer: recover, record and retry the same logical write

Use a fresh isolated store and this exact about. An initial not_found is
expected here. With an existing about, inspect its memory and catalogue
instead of pretending it is empty. A write retry reuses its original payload
and idempotency_key; it does not rephrase the source or create a new key.

```json
{
  "tool": "kmp_wake",
  "save_as": "initial",
  "expect_error": "not_found",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "budget": {
      "max_bytes": 20000
    }
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "constraint",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-shared:constraint:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "review": [
        "HND-9"
      ],
      "agentic_process": [
        "handoff-review"
      ],
      "task": [
        "journal-choice"
      ]
    },
    "occurred_at": "2026-09-01T09:00:00Z",
    "observed_at": "2026-09-01T09:05:00Z",
    "memories": [
      {
        "id": "current",
        "kind": "constraint",
        "summary": "C1: The HND-9 journal must work offline. Ledger records must remain on this device.",
        "summary_en": "C1 requires device-local ledger storage and offline journal operation for HND-9.",
        "evidence": "C1, received 2026-09-01T09:05:00Z: C1: The HND-9 journal must work offline. Ledger records must remain on this device."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "constraint_read",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "ref": "${constraint.generated_refs.0}",
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "decision_review",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-shared:decision:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "review": [
        "HND-9"
      ],
      "agentic_process": [
        "handoff-review"
      ],
      "task": [
        "journal-choice"
      ]
    },
    "occurred_at": "2026-09-01T09:10:00Z",
    "observed_at": "2026-09-01T09:15:00Z",
    "read_context": {
      "inspected_refs": [
        "${constraint.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "D1: We choose SQLite for the HND-9 journal because its device-local storage can satisfy the offline requirement C1.",
        "summary_en": "D1 selects SQLite for HND-9 because device-local storage can meet C1, the offline journal requirement.",
        "evidence": "D1, received 2026-09-01T09:15:00Z: D1: We choose SQLite for the HND-9 journal because its device-local storage can satisfy the offline requirement C1.",
        "connect_to": [
          {
            "ref": "${constraint.generated_refs.0}",
            "rel": "chosen_because",
            "class": "motivational",
            "confidence": "high",
            "why": "D1 explicitly selects SQLite because its local storage can satisfy C1. The choice is motivated by that constraint, not evidence of a completed test.",
            "evidence": "D1: We choose SQLite for the HND-9 journal because its device-local storage can satisfy the offline requirement C1."
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
{"tool":"kmp_write_memory","save_as":"decision","arguments":"${decision_review.next_actions.0.arguments}"}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "decision_read",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "ref": "${decision.generated_refs.0}",
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "decision_retry",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-shared:decision:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "review": [
        "HND-9"
      ],
      "agentic_process": [
        "handoff-review"
      ],
      "task": [
        "journal-choice"
      ]
    },
    "occurred_at": "2026-09-01T09:10:00Z",
    "observed_at": "2026-09-01T09:15:00Z",
    "read_context": {
      "inspected_refs": [
        "${constraint.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "D1: We choose SQLite for the HND-9 journal because its device-local storage can satisfy the offline requirement C1.",
        "summary_en": "D1 selects SQLite for HND-9 because device-local storage can meet C1, the offline journal requirement.",
        "evidence": "D1, received 2026-09-01T09:15:00Z: D1: We choose SQLite for the HND-9 journal because its device-local storage can satisfy the offline requirement C1.",
        "connect_to": [
          {
            "ref": "${constraint.generated_refs.0}",
            "rel": "chosen_because",
            "class": "motivational",
            "confidence": "high",
            "why": "D1 explicitly selects SQLite because its local storage can satisfy C1. The choice is motivated by that constraint, not evidence of a completed test.",
            "evidence": "D1: We choose SQLite for the HND-9 journal because its device-local storage can satisfy the offline requirement C1."
          }
        ]
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "handoff",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-shared:handoff:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "review": [
        "HND-9"
      ],
      "agentic_process": [
        "handoff-review"
      ],
      "task": [
        "journal-choice"
      ]
    },
    "occurred_at": "2026-09-01T09:20:00Z",
    "observed_at": "2026-09-01T09:25:00Z",
    "read_context": {
      "inspected_refs": [
        "${decision.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "turn",
        "summary": "H1: Please confirm D1 is the selection to review for HND-9. The offline restore test is still pending; no test report is available.",
        "summary_en": "H1 requests confirmation of the D1 review selection for HND-9 and leaves the offline restore test pending without a report.",
        "evidence": "H1, received 2026-09-01T09:25:00Z: H1: Please confirm D1 is the selection to review for HND-9. The offline restore test is still pending; no test report is available.",
        "connect_to": [
          {
            "ref": "${decision.generated_refs.0}",
            "rel": "uses_background",
            "class": "evidential",
            "confidence": "high",
            "why": "H1 names D1 as the decision to review. D1 is the context for the request; it does not establish an offline test result.",
            "evidence": "H1: Please confirm D1 is the selection to review for HND-9. The offline restore test is still pending; no test report is available."
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
  "save_as": "handoff_written",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "ref": "${handoff.generated_refs.0}",
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

## Fresh reader: recover through returned refs

Close the writer process and start a new MCP client against the same store.
The caller gives it the about and the goal of continuing HND-9; do not inject
the writer's generated refs as its retrieval results. For this review, take
the observed snapshot at 09:30 UTC, after the handoff and before feedback.
Wake needs as_of or interval when axis is explicit; its compact snapshot
returns H1 and the label catalogue. That detail tier omits expanded proof;
the following inspections and trace read the specific proof this review needs. Inspect that cursor, follow its stored link to D1, then
follow D1's reason to C1. The particular output indices below are verified
for these sources; in other histories choose by relation and endpoints.

The view is ephemeral and process-scoped; it is not stored memory. This
reader has no old shared view to resume. It opens its own view once after
recovering the durable work, then navigates that view with intents.

```json
{
  "tool": "kmp_wake",
  "save_as": "resume",
  "new_session": "reader",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "axis": "observed",
    "as_of": {
      "time": "2026-09-01T09:30:00Z"
    },
    "budget": {
      "detail": "compact",
      "max_bytes": 20000
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "recovered_handoff",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "ref": "${resume.resume_cursor.ref}",
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "recovered_decision",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "ref": "${recovered_handoff.links.outgoing.0.to}",
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "recovered_constraint",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "ref": "${recovered_decision.links.outgoing.0.to}",
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "handoff_proof",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "from": "${recovered_handoff.object.ref}",
    "to": "${recovered_constraint.object.ref}",
    "budget": {
      "max_bytes": 16000,
      "depth": 6
    }
  }
}
```

```json
{
  "tool": "kmp_view_open",
  "save_as": "opened",
  "arguments": {
    "about": "example:guide:shared-resumption"
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "initial_frame",
  "arguments": {
    "expected_revision": "${opened.view_revision}",
    "idempotency_key": "guide-shared:initial-frame:v1",
    "explanation": "Review the journal decision and its pending offline test",
    "focus": {
      "time_range": {
        "axis": "occurred",
        "from": "2026-09-01T08:30:00Z",
        "to": "2026-09-01T11:00:00Z"
      }
    },
    "projection": {
      "semantic_zoom": "moment",
      "dimensions": [
        "review"
      ],
      "labels": [
        {
          "key": "review",
          "op": "in",
          "values": [
            "HND-9"
          ]
        }
      ],
      "relation_classes": [
        "motivational",
        "evidential"
      ]
    },
    "selection": "${recovered_decision.object.ref}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "before_human",
  "arguments": {}
}
```

## Person: change clock and selection

In the shared ChronoLoom view choose Observed and select C1, the original
constraint. Keep the review label. The observed clock shows when the team
received each source, not when the source's event occurred. The UI reports
its current frame and advances its revision. The control badge records who
moved it; it is not an exclusive lock or proof that an agent is connected.

The native replay simulates this gesture through the same loopback report
endpoint used by ChronoLoom; it labels that step as a fixture. With
`--interactive`, it waits here for actual browser actions and then reads their
state. That visual run is recorded separately. Neither mode fabricates a
human approval or writes memory from the gesture.

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "human_state",
  "human_checkpoint": true,
  "arguments": {}
}
```

## Retries and stale intent do not undo the person's move

First replay the already accepted initial intent exactly. Its key means it
already happened: applied=false with the CURRENT state is successful replay,
not a request to restore the earlier clock or selection. Then demonstrate a
new intent deliberately carrying the old revision. It must conflict. Read
the live state again; do not remove expected_revision to force it through.

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "old_intent_replay",
  "arguments": {
    "expected_revision": "${opened.view_revision}",
    "idempotency_key": "guide-shared:initial-frame:v1",
    "explanation": "Review the journal decision and its pending offline test",
    "focus": {
      "time_range": {
        "axis": "occurred",
        "from": "2026-09-01T08:30:00Z",
        "to": "2026-09-01T11:00:00Z"
      }
    },
    "projection": {
      "semantic_zoom": "moment",
      "dimensions": [
        "review"
      ],
      "labels": [
        {
          "key": "review",
          "op": "in",
          "values": [
            "HND-9"
          ]
        }
      ],
      "relation_classes": [
        "motivational",
        "evidential"
      ]
    },
    "selection": "${recovered_decision.object.ref}"
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "stale_intent",
  "expect_error": "conflict",
  "arguments": {
    "expected_revision": "${before_human.view_revision}",
    "idempotency_key": "guide-shared:stale-plan:v1",
    "explanation": "Deliberately stale teaching move; it must not replace the human frame",
    "selection": "${recovered_handoff.object.ref}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "after_conflict",
  "arguments": {}
}
```

## Audit, then rebase on the actual shared frame

Read the selected constraint on the person's chosen clock, and trace D1's
stated reason. Neither tool is made temporal by the viewer alone: pass the
clock explicitly to Goto. Inspect remains the current stored object, not a
historical projection. The reviewed source says a test is pending; a valid
choice or a successful view move cannot be reported as test success.

The next agreed focus is D1 with its C1 proof. Prepare it against the latest
revision and change only selection/trace. Omit focus and projection so the
person's clock, window, labels and layers remain. This is a newly considered
intent, with its own key; an intervening move still requires another state
read and reconsideration. Never reopen the view merely to navigate.

```json
{
  "tool": "kmp_goto",
  "save_as": "human_cursor",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "at": {
      "ref": "${after_conflict.state.selection}"
    },
    "axis": "${after_conflict.state.clock}",
    "dimensions": {
      "mode": "only",
      "include": [
        "review"
      ]
    },
    "budget": {
      "max_bytes": 20000
    }
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "decision_proof",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "from": "${recovered_decision.object.ref}",
    "to": "${recovered_constraint.object.ref}",
    "budget": {
      "max_bytes": 16000,
      "depth": 4
    }
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "ready_to_rebase",
  "arguments": {}
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "rebased",
  "arguments": {
    "expected_revision": "${ready_to_rebase.view_revision}",
    "idempotency_key": "guide-shared:review-proof:v1",
    "explanation": "Review D1 and its C1 proof within the observed frame you chose",
    "selection": "${recovered_decision.object.ref}",
    "trace": {
      "from": "${recovered_decision.object.ref}",
      "to": "${recovered_constraint.object.ref}"
    }
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "rebased_state",
  "arguments": {}
}
```

## Explicit feedback arrives; the test is still pending

F1 now arrives as a separate literal source. Record its limited confirmation,
not an inferred success_path or authorization to run a test. Both destinations
were inspected in this reader session. The source answers the stored review
request and confirms the decision under review; those are distinct links.
After the write, Forward from the recovered H1 ref on observed returns the
later feedback. This proves a temporal delta independently of the view.
A separate complete temporal enumeration checks the four stored records
without expanding the entire wake proof. The final compact observed snapshot
is at 10:10 UTC, after F1 was received.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "feedback_review",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "actor": "guide-reviewer",
    "source_kind": "human",
    "idempotency_key": "guide-shared:feedback:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "review": [
        "HND-9"
      ],
      "agentic_process": [
        "handoff-review"
      ],
      "task": [
        "journal-choice"
      ]
    },
    "occurred_at": "2026-09-01T10:00:00Z",
    "observed_at": "2026-09-01T10:05:00Z",
    "read_context": {
      "inspected_refs": [
        "${recovered_handoff.object.ref}",
        "${recovered_decision.object.ref}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "feedback",
        "summary": "F1: In answer to H1, I confirm D1 is the selection to review for HND-9. This confirms the selection only; the offline restore test is still pending.",
        "summary_en": "F1 answers H1 by confirming D1 as the HND-9 review selection; it does not confirm test success, and the offline restore test remains pending.",
        "evidence": "F1, received 2026-09-01T10:05:00Z: F1: In answer to H1, I confirm D1 is the selection to review for HND-9. This confirms the selection only; the offline restore test is still pending.",
        "connect_to": [
          {
            "ref": "${recovered_handoff.object.ref}",
            "rel": "answers",
            "class": "evidential",
            "confidence": "high",
            "why": "F1 explicitly answers H1, the stored request for confirmation of the review selection.",
            "evidence": "F1: In answer to H1, I confirm D1 is the selection to review for HND-9. This confirms the selection only; the offline restore test is still pending."
          },
          {
            "ref": "${recovered_decision.object.ref}",
            "rel": "confirms_selection",
            "class": "evidential",
            "confidence": "high",
            "why": "F1 explicitly confirms D1 as the selection to review. It limits that confirmation to selection and leaves the offline test pending.",
            "evidence": "F1: In answer to H1, I confirm D1 is the selection to review for HND-9. This confirms the selection only; the offline restore test is still pending."
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
{"tool":"kmp_write_memory","save_as":"feedback","arguments":"${feedback_review.next_actions.0.arguments}"}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "feedback_read",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "ref": "${feedback.generated_refs.0}",
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "since_handoff",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "from": {
      "ref": "${recovered_handoff.object.ref}"
    },
    "axis": "observed",
    "dimensions": {
      "mode": "only",
      "include": [
        "review"
      ]
    },
    "limit": {
      "entries": 10
    },
    "budget": {
      "max_bytes": 20000
    }
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "final_records",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "from": {
      "time": "2026-09-01T08:00:00Z"
    },
    "axis": "observed",
    "dimensions": {
      "mode": "only",
      "include": [
        "review"
      ]
    },
    "limit": {
      "entries": 10
    },
    "budget": {
      "max_bytes": 20000
    }
  }
}
```

```json
{
  "tool": "kmp_wake",
  "save_as": "final_memory",
  "arguments": {
    "about": "example:guide:shared-resumption",
    "axis": "observed",
    "as_of": {
      "time": "2026-09-01T10:10:00Z"
    },
    "budget": {
      "detail": "compact",
      "max_bytes": 20000
    }
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "view_state",
  "arguments": {}
}
```

The outcome is a recovered and reviewed selection, with the offline test
still pending. Preserve H1/F1 and their proof as the continuation point; do
not report the testing action complete. The view revision and memory frontier
are different coordinates and should never be substituted for each other.

Run `python3 scripts/guide_examples/replay_shared.py --binary <development-binary>
--trace <new-jsonl> --result <new-json>`. Add `--interactive` for the real UI
checkpoint and a final visual review before closing the reader. The same
lesson is executed in both modes; the automated gesture is identified in the
trace. Counts include each session's guide entry and native tool discovery.
No model is invoked and this is not independent writer learning.
