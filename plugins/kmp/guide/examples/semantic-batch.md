# One packet, local proof links and independent clocks

Fictional sources:

- R1, observed September 1 at 09:35 UTC: request logs for component **neb**
  show refresh success followed immediately by an unauthorized request. Its
  registry lists **Nebula cache** and **NC** as aliases. The event time is unknown.
- R2, observed at 09:50 UTC: at 09:45 the team chose a retry because of R1's
  refresh race. This is a decision; it does not prove the retry worked.
- R3, observed at 10:30 UTC: an operator asks to remove the retry selected in
  R2. The team has not approved any change; R2 remains the selected behavior.

Use a fresh teaching store. The writer collected R1/R2 at 10:00 UTC and records R3 later, at 10:30. Actual ingestion uses the kernel clock. A minimal
write is a one-element `memories` array with id, kind, summary, evidence and
labels, plus about, actor and observed_at. The packet below adds a second
record and a forward local relation. Send only `arguments`; `${...}` copies
exact returned values. This replay checks the native contract, not LLM learning.

## Rejection leaves the about empty

This deliberately invalid packet has a valid first record and a second one
without evidence. The refusal returns code `MEMORY_EVIDENCE_REQUIRED`,
field `memories[1].evidence` and no fabricated repair action. Strict is the
default. Repair the source field; do not
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
        ],
        "labels": {
          "source": [
            "R2"
          ]
        }
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
          ],
          "source": [
            "R1"
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
membership applies to both records; only R1 has aliases. Each record gets its
own source label: R1 on the logs and R2 on the decision. Putting both source
values in top-level labels would attach both to every record; that would not
represent these sources faithfully. No process dimension
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
        ],
        "labels": {
          "source": [
            "R2"
          ]
        }
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
          ],
          "source": [
            "R1"
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
        ],
        "labels": {
          "source": [
            "R2"
          ]
        }
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
          ],
          "source": [
            "R1"
          ]
        }
      }
    ]
  }
}
```

## A request is not an approved state change

R3 reports a request about R2; it does not replace or update R2. Use an
observation for the request fact, with its source and explicit lack of approval.
`request` and `outcome` are not kinds: INVALID_KIND returns `allowed_values`,
but KMP does not decide which meaning fits the source. Do not automatically
replace every rejected kind with observation.

The earlier `choice` inspection supplies actual prior context. `uses_background`
records that R3 refers to R2; its why/evidence do not claim a change. A future
approval would need its own source and justified lifecycle relation. Do not
infer validity dates from this request's observation time.

```json
{"tool":"kmp_write_memory","save_as":"request","arguments":{"about":"example:guide:semantic-batch","actor":"guide-writer","observed_at":"2026-09-01T10:30:00Z","idempotency_key":"guide-semantic-batch:request","labels":{"component":["neb"]},"read_context":{"inspected_refs":["${choice.object.ref}"]},"memories":[{"id":"request","kind":"observation","summary":"R3: An operator requests removal of the R2 retry; the team has not approved a change.","evidence":"R3 at 10:30 UTC: remove the retry selected in R2? The team has not approved any change; R2 remains the selected behavior.","labels":{"source":["R3"]},"connect_to":[{"ref":"${choice.object.ref}","rel":"uses_background","class":"evidential","why":"R3 refers to the R2 retry decision as the context for the request, without changing it.","evidence":"R3 explicitly asks about the retry selected in R2 and says no change has been approved."}]}]}}
```

```json
{"tool":"kmp_inspect","save_as":"request_read","arguments":{"about":"example:guide:semantic-batch","ref":"${request.local_refs.request}","budget":{"max_bytes":30000}}}
```

```json
{"tool":"kmp_inspect","save_as":"choice_after_request","arguments":{"about":"example:guide:semantic-batch","ref":"${choice.object.ref}","budget":{"max_bytes":30000}}}
```

```json
{"tool":"kmp_goto","save_as":"request_time","arguments":{"about":"example:guide:semantic-batch","at":{"ref":"${request.local_refs.request}"},"axis":"observed","limit":{"entries":10},"budget":{"max_bytes":30000}}}
```

## Check the shared graph

Review three memories with two kinds: the decision has a causal chosen_because
link to the logs; the later request has an evidential uses_background link to
the decision. Component membership is shared, source labels are individual,
and only R1 carries the two aliases. Temporal
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
        "to": "2026-09-01T10:35:00Z",
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

The accepted packet reports `coverage.label_memberships=6`: the shared component
on both records, one source label per record and two aliases on the logs.
R3 is a separate later write. A label naming a source is not its evidence: the
literal source text remains in each evidence field. `source_coverage=not_assessed` does
not certify that R1/R2 contained no other facts. For this teaching audit, execute
`written.receipt.action` verbatim. Inspect's `object.text` contains JSON with
`receipt.writer.local_refs` and the accepted `receipt.canonical_memory`; its
chosen_because relation keeps R2's reason and proof. This detail is available on
demand and need not be loaded after every successful write.

## Self-check fidelity and navigation

An accepted packet is what you submitted, not proof that you interpreted the
sources correctly. Before a handoff or after a difficult extraction, compare
the source with the receipt or the relevant memory's Inspect result. Reuse the
results you already read; this is not a second full receipt read after every write.

| Source fact | Check in this lesson | Why it matters |
| --- | --- | --- |
| R1 was observed at 09:35; event time is unknown | `logs` has observed_at 09:35 and no occurred_at | A midnight default or packet time would move the observation |
| R2 was chosen at 09:45 and observed at 09:50 | `choice` carries those two distinct clocks | The packet's 10:00 provenance is neither event |
| R1 names Nebula cache and NC for component neb | Both alias memberships and component=neb exist | A spelling only in prose cannot be selected as a label |
| R3 requests removing the retry but changes no decision | R2 stays selected; the request does not supersede it | A proposal is not an approval or an executed violation |

For facts with different lifecycles, split the memories before writing. For
example, a source may state a person's role, their use of `@north`, and a data
residency requirement. If the account later changes hands, only its assignment
expires; the role and requirement remain. Use `account: ["@north"]` for both
assignments, with separate person labels and evidence-backed validity. Keep a
literal ambiguous signature searchable without asserting the person's identity.
The complete account history is `guide:kmp-agent:example:alias-ownership`.

Check at least the route the memory is meant to support: reuse a catalogue
label, move through its timeline with Forward/Goto and inspect a relevant link's
proof. Use Trace when the claimed explanation spans several links. In ChronoLoom,
select that label, choose the source's clock, zoom to the event and follow the
relation. Seeing one known answer with Ask does not verify these other routes.

When a source says `UE`, an English search rendering can say `UE (EU)`; `EU`
alone fails the literal identifier lint. If the source text says `2026`, `09:00`
and `UTC`, keep those tokens in the rendering even when also adding an ISO date.
Never alter the canonical text to satisfy the lint. Review the complete feedback
array so independent record errors are repaired together. The lint checks literal
identifiers, not semantic equivalence or fidelity of clocks to prose evidence.
