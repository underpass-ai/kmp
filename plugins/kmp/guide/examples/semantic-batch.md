# One packet, local proof links and independent clocks

Fictional sources:

- R1, observed September 1 at 09:35 UTC: request logs for component **neb**
  show refresh success followed immediately by an unauthorized request. Its
  registry lists **Nebula cache** and **NC** as aliases. The event time is unknown.
- R2, observed at 09:50 UTC: at 09:45 the team chose a retry because of R1's
  refresh race. This is a decision; it does not prove the retry worked.

Use a fresh teaching store. The writer observed the collected sources at 10:00 UTC; actual ingestion uses the kernel clock. A minimal
write is a one-element `memories` array with id, kind, summary, evidence and
labels, plus about, actor and observed_at. The packet below adds a second
record and a forward local relation. Send only `arguments`; `${...}` copies
exact returned values. This replay checks the native contract, not LLM learning.

## Rejection leaves the about empty

This deliberately invalid packet has a valid first record and a second one
without evidence. Strict is the default. Repair the source field; do not
weaken validation merely to accept the packet.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "rejected",
  "arguments": {
    "about": "example:guide:semantic-batch",
    "actor": "guide-writer",
    "observed_at": "2026-09-01T10:00:00Z",
    "labels": {
      "component": [
        "neb"
      ]
    },
    "idempotency_key": "guide-semantic-batch:1",
    "memories": [
      {
        "id": "choice",
        "kind": "decision",
        "summary": "R2: The team chooses a retry after token refresh.",
        "evidence": "R2 selects a retry because R1 logs show the refresh race.",
        "observed_at": "2026-09-01T09:50:00Z",
        "occurred_at": "2026-09-01T09:45:00Z",
        "connect_to": [
          {
            "ref": "@logs",
            "rel": "chosen_because",
            "class": "causal",
            "why": "A retry addresses the refresh race recorded in R1.",
            "evidence": "R2 cites R1 as the reason for the retry."
          }
        ]
      },
      {
        "id": "logs",
        "kind": "observation",
        "summary": "R1: Request logs show a token refresh race.",
        "observed_at": "2026-09-01T09:35:00Z",
        "labels": {
          "alias": [
            "Nebula cache",
            "NC"
          ]
        }
      }
    ]
  },
  "expect_error": "invalid_argument"
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "still_empty",
  "arguments": {
    "about": "example:guide:semantic-batch",
    "ref": "example:guide:semantic-batch"
  },
  "expect_error": "not_found"
}
```

## Write both records and their proof in one call

`@logs` resolves inside the same packet, despite appearing later. It is not
an existing memory ref and needs no fabricated read_context. Shared component
membership applies to both records; only R1 has aliases. No process dimension
is invented. Sources and relation rationale remain separate.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "written",
  "arguments": {
    "about": "example:guide:semantic-batch",
    "actor": "guide-writer",
    "observed_at": "2026-09-01T10:00:00Z",
    "labels": {
      "component": [
        "neb"
      ]
    },
    "idempotency_key": "guide-semantic-batch:1",
    "memories": [
      {
        "id": "choice",
        "kind": "decision",
        "summary": "R2: The team chooses a retry after token refresh.",
        "evidence": "R2 selects a retry because R1 logs show the refresh race.",
        "observed_at": "2026-09-01T09:50:00Z",
        "occurred_at": "2026-09-01T09:45:00Z",
        "connect_to": [
          {
            "ref": "@logs",
            "rel": "chosen_because",
            "class": "causal",
            "why": "A retry addresses the refresh race recorded in R1.",
            "evidence": "R2 cites R1 as the reason for the retry."
          }
        ]
      },
      {
        "id": "logs",
        "kind": "observation",
        "summary": "R1: Request logs show a token refresh race.",
        "evidence": "R1 shows refresh success followed immediately by an unauthorized request; its registry lists Nebula cache and NC for neb.",
        "observed_at": "2026-09-01T09:35:00Z",
        "labels": {
          "alias": [
            "Nebula cache",
            "NC"
          ]
        }
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "choice",
  "arguments": {
    "about": "example:guide:semantic-batch",
    "ref": "${written.local_refs.choice}",
    "include": {
      "raw": true
    },
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "logs",
  "arguments": {
    "about": "example:guide:semantic-batch",
    "ref": "${written.local_refs.logs}",
    "include": {
      "raw": true
    },
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "proof",
  "arguments": {
    "about": "example:guide:semantic-batch",
    "from": "${written.local_refs.choice}",
    "to": "${written.local_refs.logs}",
    "budget": {
      "max_bytes": 30000
    }
  }
}
```

## Read the timeline and retry safely

At 09:40 on the observed clock only R1 is available. At 09:55 R2 joins it.
The later receipt does not replace either observation time. R1 still has no
known occurred_at. Retrying the exact packet keeps refs and sequences.

```json
{
  "tool": "kmp_goto",
  "save_as": "before_choice",
  "arguments": {
    "about": "example:guide:semantic-batch",
    "at": {
      "time": "2026-09-01T09:40:00Z"
    },
    "budget": {
      "max_bytes": 30000
    }
  }
}
```

```json
{
  "tool": "kmp_goto",
  "save_as": "after_choice",
  "arguments": {
    "about": "example:guide:semantic-batch",
    "at": {
      "time": "2026-09-01T09:55:00Z"
    },
    "budget": {
      "max_bytes": 30000
    }
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "retry",
  "arguments": {
    "about": "example:guide:semantic-batch",
    "actor": "guide-writer",
    "observed_at": "2026-09-01T10:00:00Z",
    "labels": {
      "component": [
        "neb"
      ]
    },
    "idempotency_key": "guide-semantic-batch:1",
    "memories": [
      {
        "id": "choice",
        "kind": "decision",
        "summary": "R2: The team chooses a retry after token refresh.",
        "evidence": "R2 selects a retry because R1 logs show the refresh race.",
        "observed_at": "2026-09-01T09:50:00Z",
        "occurred_at": "2026-09-01T09:45:00Z",
        "connect_to": [
          {
            "ref": "@logs",
            "rel": "chosen_because",
            "class": "causal",
            "why": "A retry addresses the refresh race recorded in R1.",
            "evidence": "R2 cites R1 as the reason for the retry."
          }
        ]
      },
      {
        "id": "logs",
        "kind": "observation",
        "summary": "R1: Request logs show a token refresh race.",
        "evidence": "R1 shows refresh success followed immediately by an unauthorized request; its registry lists Nebula cache and NC for neb.",
        "observed_at": "2026-09-01T09:35:00Z",
        "labels": {
          "alias": [
            "Nebula cache",
            "NC"
          ]
        }
      }
    ]
  }
}
```

## Check the shared graph

Review two kinds, one causal chosen_because relation, the literal sources,
component membership on both records and two aliases on R1 alone. Temporal
zoom and following the relation should take you from the decision to its proof.

```json
{
  "tool": "kmp_view_open",
  "save_as": "view",
  "arguments": {
    "about": "example:guide:semantic-batch"
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "frame",
  "arguments": {
    "view_id": "${view.view_id}",
    "expected_revision": "${view.view_revision}",
    "idempotency_key": "guide-semantic-batch:view",
    "selection": "${written.local_refs.choice}",
    "focus": {
      "time_range": {
        "from": "2026-09-01T09:30:00Z",
        "to": "2026-09-01T10:05:00Z",
        "axis": "observed"
      }
    },
    "projection": {
      "semantic_zoom": "moment"
    },
    "explanation": "Review the packet, separate clocks, labels and the decision proof"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "view_state",
  "arguments": {
    "view_id": "${view.view_id}"
  }
}
```
