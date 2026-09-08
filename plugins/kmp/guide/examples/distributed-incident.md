# Worked example: one incident across support and project memory

INC-17 contains an outage and a later recovery. Support and project report the
same outage at different observation times. Use shared labels to find both
reports, inspect the sources, declare only the supported equivalence, then
follow the story across abouts and along their timelines.

These are fictional sources in two isolated abouts. Read the installed
`guide/AGENT.md`, the live tool schemas and this lesson's indexed node. Consult
the relations and scope topics and the extended write, relate, time, inspect,
trace and view guidance on first use. Reuse the guide while it remains in
context. The LLM decides what a source means; KMP validates and preserves that
choice. The repository replay uses `--lesson distributed-incident` and no model.
It checks this authored path, not another LLM's ability to learn it.

## Sources in their availability order

All dates below are UTC in 2026. `occurred_at` locates the reported event;
`observed_at` records when this source became known. Ingestion is the real run
time. No validity period is claimed. S4 is a new report about the old outage:
its event time stays September 1, while its identity evidence is only known
on September 2. Never use the event clock to claim that evidence was known earlier.

| Source | Owning about | Event time | First observed | Literal source |
| --- | --- | --- | --- | --- |
| S1, support incident log | `example:guide:incident-support` | Sep 1, 09:00 | Sep 1, 09:05 | At 09:00 UTC on September 1, INC-17 event EVT-17-F stopped checkout in Atlas. |
| S2, project recovery log | `example:guide:incident-project` | Sep 1, 09:30 | Sep 1, 09:35 | Rollback of rel-17 restored Atlas checkout at 09:30 UTC on September 1. This recovery is event EVT-17-R within INC-17. |
| S3, deployment audit | `example:guide:incident-project` | Sep 1, 09:00 | Sep 2, 08:00 | The deployment of rel-17 exhausted Atlas database connections at 09:00 UTC on September 1: outage EVT-17-F in INC-17. The audit compared it with recovery EVT-17-R. |
| S4, support/project reconciliation | `example:guide:incident-support` | Sep 1, 09:00 | Sep 2, 08:30 | The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event. |

`incident=INC-17` groups the whole incident. `event=EVT-17-F` names the outage
and `event=EVT-17-R` names its recovery. Matching incident labels do not make
these events identical. `atlas-incident-review` names this authored process.

S1 and S4 are observations; S2 is a successful rollback path; S3 is a failed
deployment path. All use `record_observation` as the writing intent. Intent,
memory kind, relation and relation class are separate choices. S3 uses
`uses_background` toward S2 because its source explicitly compares them;
it does not say the recovery is the failure or a requirement. In particular,
`checked_against` belongs to the constraint class in the live vocabulary and
is not a generic evidential comparison.

## Recover and write the first reports

Envelopes below bind returned results with `save_as`. Send only `arguments`.
`${name.generated_refs.0}` copies the exact returned ref. `expect_error`
names an intentional failure. A fresh missing about returns `not_found`;
if it already exists, recover its state and catalogue instead of replaying
the missing-about assumption. Every logical write has its own idempotency key.

```json
{
  "tool": "kmp_wake",
  "save_as": "support_empty",
  "arguments": {
    "about": "example:guide:incident-support"
  },
  "expect_error": "not_found"
}
```

```json
{
  "tool": "kmp_wake",
  "save_as": "project_empty",
  "arguments": {
    "about": "example:guide:incident-project"
  },
  "expect_error": "not_found"
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "outage",
  "arguments": {
    "about": "example:guide:incident-support",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-distributed:outage:v1",
    "scope": {
      "process": "atlas-incident-review"
    },
    "labels": {
      "incident": "INC-17",
      "event": "EVT-17-F"
    },
    "occurred_at": "2026-09-01T09:00:00Z",
    "observed_at": "2026-09-01T09:05:00Z",
    "current": {
      "kind": "observation",
      "summary": "At 09:00 UTC on September 1, INC-17 event EVT-17-F stopped checkout in Atlas.",
      "evidence": "At 09:00 UTC on September 1, INC-17 event EVT-17-F stopped checkout in Atlas."
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "outage_read",
  "arguments": {
    "about": "example:guide:incident-support",
    "ref": "${outage.generated_refs.0}",
    "budget": {
      "max_bytes": 20000
    }
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "recovery",
  "arguments": {
    "about": "example:guide:incident-project",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-distributed:recovery:v1",
    "scope": {
      "process": "atlas-incident-review"
    },
    "labels": {
      "incident": "INC-17",
      "event": "EVT-17-R"
    },
    "occurred_at": "2026-09-01T09:30:00Z",
    "observed_at": "2026-09-01T09:35:00Z",
    "current": {
      "kind": "success_path",
      "summary": "Rollback of rel-17 restored Atlas checkout at 09:30 UTC on September 1. This recovery is event EVT-17-R within INC-17.",
      "evidence": "Rollback of rel-17 restored Atlas checkout at 09:30 UTC on September 1. This recovery is event EVT-17-R within INC-17."
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "recovery_read",
  "arguments": {
    "about": "example:guide:incident-project",
    "ref": "${recovery.generated_refs.0}",
    "budget": {
      "max_bytes": 20000
    }
  }
}
```

## Initial view: what was known on September 1

Open the support about once. Add the project about to the projection, keeping
its ownership separate, and frame the incident on the observed clock. At
Memories detail (`moment`) inspect the original outage and recovery: they
have different kinds and event IDs. The source facts are already inspected
through MCP; the viewer makes their placement and distinction visible.

```json
{
  "tool": "kmp_view_open",
  "save_as": "view",
  "arguments": {
    "about": "example:guide:incident-support"
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "initial_frame",
  "arguments": {
    "expected_revision": "${view.view_revision}",
    "idempotency_key": "guide-distributed:view-initial:v1",
    "explanation": "Inspect INC-17 with separate about ownership and source observation times",
    "focus": {
      "time_range": {
        "axis": "observed",
        "from": "2026-09-01T00:00:00Z",
        "to": "2026-09-02T00:00:00Z"
      }
    },
    "projection": {
      "abouts": [
        "example:guide:incident-project"
      ],
      "semantic_zoom": "moment",
      "dimensions": [
        "incident"
      ],
      "labels": [
        {
          "key": "incident",
          "op": "in",
          "values": [
            "INC-17"
          ]
        }
      ]
    },
    "selection": "${recovery.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "initial_view_state",
  "arguments": {}
}
```

## Add the late audit and compare the abouts

S3 arrives a day later but reports the original event time. Its contextual
link targets the already inspected project recovery. Recover the catalogue
across the two explicit abouts before reusing `incident=INC-17` as a filter.
A compact wake does not include the full proof; the targeted inspections do.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "audit",
  "arguments": {
    "about": "example:guide:incident-project",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-distributed:audit:v1",
    "scope": {
      "process": "atlas-incident-review"
    },
    "labels": {
      "incident": "INC-17",
      "event": "EVT-17-F"
    },
    "occurred_at": "2026-09-01T09:00:00Z",
    "observed_at": "2026-09-02T08:00:00Z",
    "current": {
      "kind": "error_path",
      "summary": "The deployment of rel-17 exhausted Atlas database connections at 09:00 UTC on September 1: outage EVT-17-F in INC-17. The audit compared it with recovery EVT-17-R.",
      "evidence": "The deployment of rel-17 exhausted Atlas database connections at 09:00 UTC on September 1: outage EVT-17-F in INC-17. The audit compared it with recovery EVT-17-R."
    },
    "read_context": {
      "inspected_refs": [
        "${recovery.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${recovery.generated_refs.0}",
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "The audit explicitly compares the failed deployment with the separately recorded recovery; comparison does not identify the two events.",
        "evidence": "The deployment of rel-17 exhausted Atlas database connections at 09:00 UTC on September 1: outage EVT-17-F in INC-17. The audit compared it with recovery EVT-17-R."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "audit_read",
  "arguments": {
    "about": "example:guide:incident-project",
    "ref": "${audit.generated_refs.0}",
    "budget": {
      "max_bytes": 20000
    }
  }
}
```

```json
{
  "tool": "kmp_wake",
  "save_as": "catalogue",
  "arguments": {
    "about": "example:guide:incident-support",
    "dimensions": {
      "scope": "abouts",
      "abouts": [
        "example:guide:incident-support",
        "example:guide:incident-project"
      ]
    },
    "budget": {
      "max_bytes": 30000,
      "detail": "compact"
    }
  }
}
```

```json
{
  "tool": "kmp_relate",
  "save_as": "related",
  "arguments": {
    "about": "example:guide:incident-support",
    "dimensions": {
      "scope": "abouts",
      "abouts": [
        "example:guide:incident-support",
        "example:guide:incident-project"
      ],
      "mode": "only",
      "include": [
        "incident"
      ],
      "scope_ids": [
        "INC-17"
      ]
    },
    "interval": {
      "start": "2026-09-01T00:00:00Z",
      "end": "2026-09-03T00:00:00Z"
    },
    "axis": "occurred",
    "budget": {
      "max_bytes": 30000
    }
  }
}
```

In this fixture, the first proposal joins S3 and S1 by `evt-17-f`, `09:00`
and shared names. A second proposal joins S2 and S1 by shared names such as
Atlas and September. Inspect `from`, `to`, `proposed_by`, `shared` and `why`:
the second pair is a recovery and a failure, so reject that equivalence.
The first position is verified for this fixture, not a general acceptance
rule. In a different history choose the source-supported pair, copy its
returned fields, and inspect both endpoint objects in their owning abouts.
A coordinate comparison, sequence coincidence or proposal is not a stored
semantic edge and is not identity proof.

Extend the shared view to include the audit. If a person has moved the view
since the saved state, call `kmp_view_get_state` again and rebase the intent
on that returned revision instead of overwriting their change.

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "audit_frame",
  "arguments": {
    "expected_revision": "${initial_view_state.view_revision}",
    "idempotency_key": "guide-distributed:view-audit:v1",
    "explanation": "Inspect INC-17 with separate about ownership and source observation times",
    "focus": {
      "time_range": {
        "axis": "observed",
        "from": "2026-09-01T00:00:00Z",
        "to": "2026-09-03T00:00:00Z"
      }
    },
    "projection": {
      "abouts": [
        "example:guide:incident-project"
      ],
      "semantic_zoom": "moment",
      "dimensions": [
        "incident"
      ],
      "labels": [
        {
          "key": "incident",
          "op": "in",
          "values": [
            "INC-17"
          ]
        }
      ]
    },
    "selection": "${audit.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "audit_view_state",
  "arguments": {}
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "unlinked",
  "arguments": {
    "about": "example:guide:incident-support",
    "from": "${outage.generated_refs.0}",
    "to": "${audit.generated_refs.0}"
  },
  "expect_error": "invalid_argument"
}
```

Both endpoint objects exist, but `trace` rejects this cross-about request
with `invalid_argument` before an equivalence is declared. Naming the target
or obtaining a proposal does not make a path.

## Reject structurally invalid declarations

When S4 arrives it supplies explicit identity evidence. Even then, a
cross-about `same_event_as` without its returned proposal is refused. The
next request deliberately omits `read_context.relate_proposals`. Validation
fails before ingest; it does not add a fifth memory or change either report.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "missing_proposal",
  "arguments": {
    "about": "example:guide:incident-support",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-distributed:missing-proposal:v1",
    "scope": {
      "process": "atlas-incident-review"
    },
    "labels": {
      "incident": "INC-17",
      "event": "EVT-17-F"
    },
    "occurred_at": "2026-09-01T09:00:00Z",
    "observed_at": "2026-09-02T08:30:00Z",
    "current": {
      "kind": "observation",
      "summary": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event.",
      "evidence": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event."
    },
    "read_context": {
      "inspected_refs": [
        "${outage.generated_refs.0}",
        "${audit.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${audit.generated_refs.0}",
        "rel": "same_event_as",
        "class": "evidential",
        "confidence": "high",
        "why": "The reconciliation explicitly identifies support and audit as reports of outage EVT-17-F; it distinguishes recovery EVT-17-R.",
        "evidence": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event."
      },
      {
        "ref": "${outage.generated_refs.0}",
        "rel": "restates",
        "class": "evidential",
        "confidence": "high",
        "why": "This new support entry restates the original outage report with the now-confirmed cross-team identity; it does not replace that source.",
        "evidence": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event."
      }
    ]
  },
  "expect_error": "invalid_argument"
}
```

A proposal also does not authorize arbitrary cross-about relations. The
next request deliberately substitutes `follows/procedural`; it is refused.
Only the permitted equivalences cross this boundary through the writer.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "unsupported_cross_link",
  "arguments": {
    "about": "example:guide:incident-support",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-distributed:unsupported-cross-link:v1",
    "scope": {
      "process": "atlas-incident-review"
    },
    "labels": {
      "incident": "INC-17",
      "event": "EVT-17-F"
    },
    "occurred_at": "2026-09-01T09:00:00Z",
    "observed_at": "2026-09-02T08:30:00Z",
    "current": {
      "kind": "observation",
      "summary": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event.",
      "evidence": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event."
    },
    "read_context": {
      "inspected_refs": [
        "${outage.generated_refs.0}",
        "${audit.generated_refs.0}"
      ],
      "relate_proposals": [
        {
          "from": "${related.proposed.0.from}",
          "to": "${related.proposed.0.to}",
          "proposed_by": "${related.proposed.0.proposed_by}"
        }
      ]
    },
    "connect_to": [
      {
        "ref": "${audit.generated_refs.0}",
        "rel": "follows",
        "class": "procedural",
        "confidence": "high",
        "why": "The reconciliation explicitly identifies support and audit as reports of outage EVT-17-F; it distinguishes recovery EVT-17-R.",
        "evidence": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event."
      },
      {
        "ref": "${outage.generated_refs.0}",
        "rel": "restates",
        "class": "evidential",
        "confidence": "high",
        "why": "This new support entry restates the original outage report with the now-confirmed cross-team identity; it does not replace that source.",
        "evidence": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event."
      }
    ]
  },
  "expect_error": "invalid_argument"
}
```

## Declare the supported identity and audit its path

The accepted request below creates a new support observation of the outage.
It restates S1 and declares `same_event_as` toward S3. It preserves both
original texts and their clocks and does not merge the abouts. Copy only
`from`, `to` and `proposed_by` from the chosen proposal into read context;
the full proposal has extra fields which this input does not accept.
The proposal may refer to the old local report because the new entry
explicitly restates that inspected report. `why` explains identity; the
literal S4 evidence proves the source actually asserts it. Neither field
is filled with a matching score as a substitute for the source.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "identity",
  "arguments": {
    "about": "example:guide:incident-support",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-distributed:identity:v1",
    "scope": {
      "process": "atlas-incident-review"
    },
    "labels": {
      "incident": "INC-17",
      "event": "EVT-17-F"
    },
    "occurred_at": "2026-09-01T09:00:00Z",
    "observed_at": "2026-09-02T08:30:00Z",
    "current": {
      "kind": "observation",
      "summary": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event.",
      "evidence": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event."
    },
    "read_context": {
      "inspected_refs": [
        "${outage.generated_refs.0}",
        "${audit.generated_refs.0}"
      ],
      "relate_proposals": [
        {
          "from": "${related.proposed.0.from}",
          "to": "${related.proposed.0.to}",
          "proposed_by": "${related.proposed.0.proposed_by}"
        }
      ]
    },
    "connect_to": [
      {
        "ref": "${audit.generated_refs.0}",
        "rel": "same_event_as",
        "class": "evidential",
        "confidence": "high",
        "why": "The reconciliation explicitly identifies support and audit as reports of outage EVT-17-F; it distinguishes recovery EVT-17-R.",
        "evidence": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event."
      },
      {
        "ref": "${outage.generated_refs.0}",
        "rel": "restates",
        "class": "evidential",
        "confidence": "high",
        "why": "This new support entry restates the original outage report with the now-confirmed cross-team identity; it does not replace that source.",
        "evidence": "The support report and deployment audit identify the same outage EVT-17-F in INC-17. Recovery EVT-17-R is a separate, later event."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "identity_read",
  "arguments": {
    "about": "example:guide:incident-support",
    "ref": "${identity.generated_refs.0}",
    "budget": {
      "max_bytes": 20000
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "audit_after",
  "arguments": {
    "about": "example:guide:incident-project",
    "ref": "${audit.generated_refs.0}",
    "budget": {
      "max_bytes": 20000
    }
  }
}
```

```json
{
  "tool": "kmp_relate",
  "save_as": "declared",
  "arguments": {
    "about": "example:guide:incident-support",
    "dimensions": {
      "scope": "abouts",
      "abouts": [
        "example:guide:incident-support",
        "example:guide:incident-project"
      ],
      "mode": "only",
      "include": [
        "incident"
      ],
      "scope_ids": [
        "INC-17"
      ]
    },
    "interval": {
      "start": "2026-09-01T00:00:00Z",
      "end": "2026-09-03T00:00:00Z"
    },
    "axis": "occurred",
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "identity_path",
  "arguments": {
    "about": "example:guide:incident-support",
    "from": "${identity.generated_refs.0}",
    "to": "${audit.generated_refs.0}",
    "budget": {
      "max_bytes": 20000
    }
  }
}
```

Expect four memories across two owners: two observations, one error path
and one success path. `declared` now includes `same_event_as` between support
and project, plus local `restates` and `uses_background`. `identity_path`
contains the cross-about edge, its `why`, evidence and `kmp_relate` method.
The audit's object, original evidence and original coordinates remain
unchanged; a later incoming relation can legitimately add evidence beside them.

## Follow the local and combined timelines

Follow the project audit on the occurred clock to reach its 09:30 recovery.
Follow the original support report on the observed clock to reach the later
reconciliation. Each local request explicitly uses its own about. The
cross-about trace above only crosses the declared equivalence; it is not
permission to trace arbitrary edges across owners.

```json
{
  "tool": "kmp_forward",
  "save_as": "project_recovery",
  "arguments": {
    "about": "example:guide:incident-project",
    "axis": "occurred",
    "from": {
      "ref": "${audit.generated_refs.0}"
    },
    "dimensions": {
      "mode": "only",
      "include": [
        "incident"
      ],
      "scope_ids": [
        "INC-17"
      ]
    },
    "window": {
      "after_entries": 10
    },
    "include": {
      "relations": false,
      "evidence": false
    },
    "budget": {
      "detail": "compact",
      "depth": 1,
      "max_bytes": 30000
    }
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "support_reconciliation",
  "arguments": {
    "about": "example:guide:incident-support",
    "axis": "observed",
    "from": {
      "ref": "${outage.generated_refs.0}"
    },
    "dimensions": {
      "mode": "only",
      "include": [
        "incident"
      ],
      "scope_ids": [
        "INC-17"
      ]
    },
    "window": {
      "after_entries": 10
    },
    "include": {
      "relations": false,
      "evidence": false
    },
    "budget": {
      "detail": "compact",
      "depth": 1,
      "max_bytes": 30000
    }
  }
}
```

A two-about read on the observed clock for September 1 contains S1 and S2.
It excludes the late audit, reconciliation and equivalence even though
those later reports refer to an event on September 1.

```json
{
  "tool": "kmp_relate",
  "save_as": "known_day1",
  "arguments": {
    "about": "example:guide:incident-support",
    "dimensions": {
      "scope": "abouts",
      "abouts": [
        "example:guide:incident-support",
        "example:guide:incident-project"
      ],
      "mode": "only",
      "include": [
        "incident"
      ],
      "scope_ids": [
        "INC-17"
      ]
    },
    "interval": {
      "start": "2026-09-01T00:00:00Z",
      "end": "2026-09-02T00:00:00Z"
    },
    "axis": "observed",
    "budget": {
      "max_bytes": 30000
    }
  }
}
```

To enumerate from 09:00 inclusively, use `goto` at that instant and
`forward` strictly after it. Both requests share one incident coordinate
lane and an explicit list of abouts. Merge by ref and retain only entries
in the intended half-open interval, here ending September 3. For larger
histories consume each returned cursor before claiming coverage; entry
limits and view zoom do not prove the rest was read.

On occurred, the boundary contains S1, S3 and S4 (three reports of the old
outage); forward returns S2. On observed, the boundary is empty and forward
returns S1, S2, S3, S4 in availability order. These compact navigation calls
omit expanded relations and evidence because those objects and the identity
path were inspected above. This reduces repeated context at the cost of
leaner replies; it is not a new compression algorithm or proof that omitted
material is irrelevant in another task.

```json
{
  "tool": "kmp_goto",
  "save_as": "occurred_start",
  "arguments": {
    "about": "example:guide:incident-support",
    "axis": "occurred",
    "dimensions": {
      "scope": "abouts",
      "abouts": [
        "example:guide:incident-support",
        "example:guide:incident-project"
      ],
      "mode": "only",
      "include": [
        "incident"
      ],
      "scope_ids": [
        "INC-17"
      ]
    },
    "include": {
      "relations": false,
      "evidence": false
    },
    "budget": {
      "max_bytes": 30000,
      "detail": "compact",
      "depth": 1
    },
    "at": {
      "time": "2026-09-01T09:00:00Z"
    },
    "window": {
      "before_entries": 0,
      "after_entries": 0
    }
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "occurred_later",
  "arguments": {
    "about": "example:guide:incident-support",
    "axis": "occurred",
    "dimensions": {
      "scope": "abouts",
      "abouts": [
        "example:guide:incident-support",
        "example:guide:incident-project"
      ],
      "mode": "only",
      "include": [
        "incident"
      ],
      "scope_ids": [
        "INC-17"
      ]
    },
    "include": {
      "relations": false,
      "evidence": false
    },
    "budget": {
      "max_bytes": 30000,
      "detail": "compact",
      "depth": 1
    },
    "from": {
      "time": "2026-09-01T09:00:00Z"
    },
    "window": {
      "after_entries": 10
    }
  }
}
```

```json
{
  "tool": "kmp_goto",
  "save_as": "observed_start",
  "arguments": {
    "about": "example:guide:incident-support",
    "axis": "observed",
    "dimensions": {
      "scope": "abouts",
      "abouts": [
        "example:guide:incident-support",
        "example:guide:incident-project"
      ],
      "mode": "only",
      "include": [
        "incident"
      ],
      "scope_ids": [
        "INC-17"
      ]
    },
    "include": {
      "relations": false,
      "evidence": false
    },
    "budget": {
      "max_bytes": 30000,
      "detail": "compact",
      "depth": 1
    },
    "at": {
      "time": "2026-09-01T09:00:00Z"
    },
    "window": {
      "before_entries": 0,
      "after_entries": 0
    }
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "observed_later",
  "arguments": {
    "about": "example:guide:incident-support",
    "axis": "observed",
    "dimensions": {
      "scope": "abouts",
      "abouts": [
        "example:guide:incident-support",
        "example:guide:incident-project"
      ],
      "mode": "only",
      "include": [
        "incident"
      ],
      "scope_ids": [
        "INC-17"
      ]
    },
    "include": {
      "relations": false,
      "evidence": false
    },
    "budget": {
      "max_bytes": 30000,
      "detail": "compact",
      "depth": 1
    },
    "from": {
      "time": "2026-09-01T09:00:00Z"
    },
    "window": {
      "after_entries": 10
    }
  }
}
```

## Final view and limits

Frame the full observed interval and select the reconciliation. Check both
owners, three kinds, incident/event labels, the cross-about equivalence,
its direction and source. Change to occurred to see the three outage reports
at 09:00 and the separate recovery at 09:30. Read shared state after a human
move. A viewer change does not change the axis or filters of subsequent MCP
memory calls; pass those explicitly as above.

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "framed",
  "arguments": {
    "expected_revision": "${audit_view_state.view_revision}",
    "idempotency_key": "guide-distributed:view-observed:v1",
    "explanation": "Compare when support and project first observed the outage, recovery and later identity proof",
    "focus": {
      "time_range": {
        "axis": "observed",
        "from": "2026-09-01T00:00:00Z",
        "to": "2026-09-03T00:00:00Z"
      }
    },
    "projection": {
      "abouts": [
        "example:guide:incident-project"
      ],
      "semantic_zoom": "moment",
      "dimensions": [
        "incident"
      ],
      "labels": [
        {
          "key": "incident",
          "op": "in",
          "values": [
            "INC-17"
          ]
        }
      ]
    },
    "selection": "${identity.generated_refs.0}"
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

The sources justify evidential relations in this story. Do not invent
other classes just to diversify a picture. KMP's structural validation does
not decide whether S4 is truthful or whether a fabricated rationale is
semantically sound. The LLM must keep an unsupported proposal unconfirmed.
Without S4 or comparable identity evidence, preserve the separate reports
and the uncertainty; neither a shared incident label nor a high proposal
weight warrants merging them. This authored replay proves the native path
and its refusal cases, not a model's judgment on unseen material.

The evidence inspector reads the current stored object and its links. A
historical scene window does not time-bound that inspection: a later incoming
link may appear in the panel even when its source is outside the scene.
Use the explicit observed interval in `known_day1` for a claim about what
was known then.

Each Moment projection carries the equivalence declared by its visible source
without importing the target memory. ChronoLoom draws it when both endpoints
are visible in the selected about planes. Removing the project plane, excluding
the audit by a label, or moving the window before the reconciliation hides the
cross-about arc. The source-owned declaration remains auditable through
inspect/trace; a missing arc alone does not prove that the stored link is absent.
