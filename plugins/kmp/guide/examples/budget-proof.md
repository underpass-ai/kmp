# Worked example: budgets, continuation and insufficient evidence

The agent-context section also exercises an agent context's short continuation: one
returned identifier retains the exact inspection and negotiated allowance.

PACK-8 is a fictional review packet containing an export history and an
unrelated warehouse report. An LLM writer interprets the five sources below;
the kernel preserves its typed records and justified links. This authored
native replay checks the teaching calls, not a new LLM's learning.

Use `example:guide:budget-proof` in an isolated store. Read the installed
agent entry once, then this exact example, the budget, scope and relations
topics, and each extended verb at first use. Source interpretation happens
before the later reading questions; do not add missing facts to satisfy a
question. A checksum test does not authorize an unrelated deletion.

## Sources in receipt order

The fictional events were each reported at their event time on September 1,
2026 (UTC). Ingestion is the actual replay time; no validity interval is
invented. PACK-8 identifies the review packet, not a semantic identity claim.

| Source | Occurred and observed | Literal source |
| --- | --- | --- |
| C1 | 2026-09-01 08:00:00 UTC | C1: Export EXP-8 must keep ledger records on this device. Network access is not permitted during the export. |
| D1 | 2026-09-01 09:00:00 UTC | D1: For export EXP-8 we choose SQLite because it can keep the ledger on this device without network access, as required by C1. |
| T1 | 2026-09-01 10:00:00 UTC | T1: The EXP-8 offline test used the SQLite plan D1. Input and restored ledger checksums both equal 9a7c. The test finished without network access. |
| R1 | 2026-09-01 11:00:00 UTC | R1: Export EXP-8 completed its offline round trip with matching checksum 9a7c. Test report T1 verifies this result. |
| X1 | 2026-09-01 12:00:00 UTC | X1: Warehouse WH-9 lights were blue at 12:00 UTC on September 1. This independent inspection reports only the warehouse lighting. |

C1 is a constraint, D1 a decision, T1 and X1 observations, and R1 a
success_path. D1 is chosen_because C1; T1 uses_background D1 as the plan
under test; R1 is verified_by T1. These are distinct claims, with different
classes and reasons. X1 has no justified semantic connection to that chain.
Sharing the packet label does not supply one. X1 intentionally uses
options.strict=false for this isolated unlinked write: strict mode otherwise
requires a connection after the about exists. State this choice explicitly;
do not invent a relation to satisfy that precondition. The four linked-history
writes keep the default strict mode and every source retains its evidence.

The JSON envelopes are teaching notation. Send only `arguments` to `tool`;
`save_as` binds the returned value. `${name.generated_refs.0}` copies an exact
returned ref. `expect_partial` marks an intentionally incomplete response in
the replay, never an MCP argument or permission to call that response complete.

## Recover, write and inspect the sources

An empty about returns not_found. With an existing about, use its actual
catalogue and refs instead of repeating this fresh-store assumption.

```json
{
  "tool": "kmp_wake",
  "save_as": "initial",
  "expect_error": "not_found",
  "arguments": {
    "about": "example:guide:budget-proof",
    "budget": {"max_bytes": 15000}
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "constraint",
  "arguments": {
    "about": "example:guide:budget-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-budget-proof:constraint:v1",
    "labels": {
      "packet": [
        "PACK-8"
      ],
      "component": [
        "export"
      ],
      "agentic_process": [
        "budget-review"
      ],
      "task": [
        "export-check"
      ]
    },
    "occurred_at": "2026-09-01T08:00:00Z",
    "observed_at": "2026-09-01T08:00:00Z",
    "memories": [
      {
        "id": "current",
        "kind": "constraint",
        "summary": "C1: Export EXP-8 must keep ledger records on this device. Network access is not permitted during the export.",
        "summary_en": "C1 requires offline, device-local ledger storage for export EXP-8: no network access is allowed.",
        "evidence": "C1, received 2026-09-01T08:00:00Z: C1: Export EXP-8 must keep ledger records on this device. Network access is not permitted during the export."
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
    "about": "example:guide:budget-proof",
    "ref": "${constraint.generated_refs.0}",
    "budget": {"max_bytes": 20000}
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "decision",
  "arguments": {
    "about": "example:guide:budget-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-budget-proof:decision:v1",
    "labels": {
      "packet": [
        "PACK-8"
      ],
      "component": [
        "export"
      ],
      "agentic_process": [
        "budget-review"
      ],
      "task": [
        "export-check"
      ]
    },
    "occurred_at": "2026-09-01T09:00:00Z",
    "observed_at": "2026-09-01T09:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${constraint.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "D1: For export EXP-8 we choose SQLite because it can keep the ledger on this device without network access, as required by C1.",
        "summary_en": "D1 selects SQLite for EXP-8 because its local storage meets the C1 offline ledger requirement.",
        "evidence": "D1, received 2026-09-01T09:00:00Z: D1: For export EXP-8 we choose SQLite because it can keep the ledger on this device without network access, as required by C1.",
        "connect_to": [
          {
            "ref": "${constraint.generated_refs.0}",
            "rel": "chosen_because",
            "class": "motivational",
            "confidence": "high",
            "why": "D1 explicitly chooses SQLite to meet C1's device-local, network-free export requirement; this is the stated reason for that choice.",
            "evidence": "D1: For export EXP-8 we choose SQLite because it can keep the ledger on this device without network access, as required by C1."
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
  "save_as": "decision_read",
  "arguments": {
    "about": "example:guide:budget-proof",
    "ref": "${decision.generated_refs.0}",
    "budget": {"max_bytes": 20000}
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "test",
  "arguments": {
    "about": "example:guide:budget-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-budget-proof:test:v1",
    "labels": {
      "packet": [
        "PACK-8"
      ],
      "component": [
        "export"
      ],
      "agentic_process": [
        "budget-review"
      ],
      "task": [
        "export-check"
      ]
    },
    "occurred_at": "2026-09-01T10:00:00Z",
    "observed_at": "2026-09-01T10:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${decision.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "T1: The EXP-8 offline test used the SQLite plan D1. Input and restored ledger checksums both equal 9a7c. The test finished without network access.",
        "summary_en": "T1 reports a successful offline EXP-8 test of the D1 SQLite plan: original and restored checksums match at 9a7c.",
        "evidence": "T1, received 2026-09-01T10:00:00Z: T1: The EXP-8 offline test used the SQLite plan D1. Input and restored ledger checksums both equal 9a7c. The test finished without network access.",
        "connect_to": [
          {
            "ref": "${decision.generated_refs.0}",
            "rel": "uses_background",
            "class": "evidential",
            "confidence": "high",
            "why": "T1 identifies D1 as the plan used in the test. The plan is context for interpreting the test; it is not itself proof that the test passed.",
            "evidence": "T1: The EXP-8 offline test used the SQLite plan D1. Input and restored ledger checksums both equal 9a7c. The test finished without network access."
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
  "save_as": "test_read",
  "arguments": {
    "about": "example:guide:budget-proof",
    "ref": "${test.generated_refs.0}",
    "budget": {"max_bytes": 20000}
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "result",
  "arguments": {
    "about": "example:guide:budget-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-budget-proof:result:v1",
    "labels": {
      "packet": [
        "PACK-8"
      ],
      "component": [
        "export"
      ],
      "agentic_process": [
        "budget-review"
      ],
      "task": [
        "export-check"
      ]
    },
    "occurred_at": "2026-09-01T11:00:00Z",
    "observed_at": "2026-09-01T11:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${test.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "success_path",
        "summary": "R1: Export EXP-8 completed its offline round trip with matching checksum 9a7c. Test report T1 verifies this result.",
        "summary_en": "R1 records successful offline completion of EXP-8, verified by the matching 9a7c checksum in T1.",
        "evidence": "R1, received 2026-09-01T11:00:00Z: R1: Export EXP-8 completed its offline round trip with matching checksum 9a7c. Test report T1 verifies this result.",
        "connect_to": [
          {
            "ref": "${test.generated_refs.0}",
            "rel": "verified_by",
            "class": "evidential",
            "confidence": "high",
            "why": "R1's matching-checksum and offline-completion claims are verified by the measured result recorded in T1.",
            "evidence": "R1: Export EXP-8 completed its offline round trip with matching checksum 9a7c. Test report T1 verifies this result."
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
  "save_as": "result_read",
  "arguments": {
    "about": "example:guide:budget-proof",
    "ref": "${result.generated_refs.0}",
    "budget": {"max_bytes": 20000}
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "unrelated",
  "arguments": {
    "about": "example:guide:budget-proof",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-budget-proof:unrelated:v1",
    "labels": {
      "packet": [
        "PACK-8"
      ],
      "component": [
        "warehouse"
      ],
      "agentic_process": [
        "budget-review"
      ],
      "task": [
        "warehouse-inspection"
      ]
    },
    "occurred_at": "2026-09-01T12:00:00Z",
    "observed_at": "2026-09-01T12:00:00Z",
    "options": {
      "strict": false
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "X1: Warehouse WH-9 lights were blue at 12:00 UTC on September 1. This independent inspection reports only the warehouse lighting.",
        "summary_en": "X1 observes blue lights at Warehouse WH-9 at 12:00 UTC on September 1; this independent lighting inspection reports nothing about an export.",
        "evidence": "X1, received 2026-09-01T12:00:00Z: X1: Warehouse WH-9 lights were blue at 12:00 UTC on September 1. This independent inspection reports only the warehouse lighting."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "unrelated_read",
  "arguments": {
    "about": "example:guide:budget-proof",
    "ref": "${unrelated.generated_refs.0}",
    "budget": {"max_bytes": 20000}
  }
}
```

```json
{
  "tool": "kmp_wake",
  "save_as": "catalogue",
  "arguments": {
    "about": "example:guide:budget-proof",
    "budget": {"max_bytes": 20000, "detail": "compact"}
  }
}
```

```json
{
  "tool": "kmp_wake",
  "save_as": "historical_context",
  "arguments": {
    "about": "example:guide:budget-proof",
    "axis": "observed",
    "interval": {"start": "2026-09-01T08:00:00Z", "end": "2026-09-01T09:00:00Z"},
    "budget": {"max_bytes": 100000, "detail": "full"}
  }
}
```

The historical proof contains C1. `scope.context` identifies the state lines,
summary, semantic next actions and labels drawn from other times as well;
D1 in those state lines does not place D1 inside the selected hour.

```json
{
  "tool": "kmp_wake",
  "save_as": "empty_history",
  "arguments": {
    "about": "example:guide:budget-proof",
    "axis": "observed",
    "interval": {"start": "2026-09-01T08:00:00Z", "end": "2026-09-01T09:00:00Z"},
    "dimensions": {"selectors": [{"key": "component", "op": "in", "values": ["warehouse"]}]},
    "budget": {"max_bytes": 100000, "detail": "full"}
  }
}
```

The warehouse source arrived later. This hour has empty proof and a null
resume cursor, while its about context and labels still describe the warehouse.
The response's scope distinguishes that context from evidence for the hour.

```json
{
  "tool": "kmp_view_open",
  "save_as": "view",
  "arguments": {
    "about": "example:guide:budget-proof"
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "packet_frame",
  "arguments": {
    "expected_revision": "${view.view_revision}",
    "idempotency_key": "guide-budget-proof:view-packet:v1",
    "explanation": "Review packet in PACK-8",
    "focus": {"time_range": {"axis": "observed", "from": "2026-09-01T07:00:00Z", "to": "2026-09-01T13:00:00Z"}},
    "projection": {"semantic_zoom": "moment", "dimensions": ["packet"], "labels": [{"key": "packet", "op": "in", "values": ["PACK-8"]}], "relation_classes": ["motivational", "evidential"]},
    "selection": "${result.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "packet_state",
  "arguments": {
  }
}
```

## Enumerate an interval with two temporal pages

The reading request is: list the records received in [08:00, 12:00) UTC.
Use the observed clock and the actual PACK-8 catalogue value. First capture
the inclusive 08:00 boundary with Goto. Retain only records exactly on that
boundary; the point lookup can also return older state in larger histories.
Then Forward starts strictly after 08:00. A page is a slice, not the interval.

The first Forward page contains D1 and T1. Preserve it and execute
the returned next_actions call. If the reading allowance ends here, report these refs,
the covered boundary, the unchanged clock/labels/limit/budget and the exact
continuation call shown next. Do not answer as though R1 or later records
had already been read. A continuation goes in from.ref; do not replace it
with the last record's timestamp or restart at the original time.

```json
{
  "tool": "kmp_goto",
  "save_as": "boundary",
  "arguments": {
    "about": "example:guide:budget-proof",
    "at": {"time": "2026-09-01T08:00:00Z"},
    "axis": "observed",
    "dimensions": {"mode": "only", "include": ["packet"], "selectors": [{"key": "packet", "op": "in", "values": ["PACK-8"]}]},
    "limit": {"entries": 10},
    "budget": {"max_bytes": 30000}
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "temporal_first",
  "expect_partial": true,
  "arguments": {
    "about": "example:guide:budget-proof",
    "from": {"time": "2026-09-01T08:00:00Z"},
    "axis": "observed",
    "dimensions": {"mode": "only", "include": ["packet"], "selectors": [{"key": "packet", "op": "in", "values": ["PACK-8"]}]},
    "limit": {"entries": 2},
    "budget": {"max_bytes": 30000}
  }
}
```

At this checkpoint, the interval is **partial**. The next call resumes
that selection with all bound arguments unchanged. In this small packet it
returns R1 and X1; X1 is exactly at 12:00 and must be excluded from the
half-open interval. Merge C1, D1, T1 and R1 by exact ref. Do not count X1,
and do not count repeated coordinates of one entry as separate memories.
Complete page.has_more first by executing next_actions and collecting each section.
Then selection.has_more reports remaining history; execute the returned navigation call.
A repeated cursor without progress is a reason to stop and report the problem.

```json
{
  "tool": "kmp_forward",
  "save_as": "temporal_second",
  "arguments": "${temporal_first.next_actions.0.arguments}"
}
```

## Audit a path over three pages

The separate audit request is to explain the path from R1 to C1. It is not
an authorization or identity inference. Follow the exact directed relations:
R1 verified_by T1, T1 uses_background D1, D1 chosen_because C1. Each link's
reason and evidence still need to support its own claim.

Limit the response to one relation per page. Trace continuation uses
page.cursor; the temporal next_actions above carries its own complete arguments. Keep both endpoints,
about, depth, byte budget and page size unchanged. Save each returned edge
and verify that new edges arrive and the opaque cursor advances; a page
marker is not a new path selection. Trace has no offset field to inspect.

```json
{
  "tool": "kmp_trace",
  "save_as": "trace_first",
  "expect_partial": true,
  "arguments": {
    "about": "example:guide:budget-proof",
    "from": "${result.generated_refs.0}",
    "to": "${constraint.generated_refs.0}",
    "budget": {"depth": 6, "max_bytes": 12000},
    "page": {"entries": 1}
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "trace_second",
  "expect_partial": true,
  "arguments": "${trace_first.next_actions.0.arguments}"
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "trace_third",
  "arguments": "${trace_second.next_actions.0.arguments}"
}
```

## Inspect when only the stable object fits

A 512-byte allowance cannot hold the source inspection. Inspect returns its
stable object with warnings; it can exceed that requested ceiling rather
than erase the object. Expandable evidence and links remain pending. An
accepted response, or a readable title in ChronoLoom, does not mean the proof
has been consumed.

Retain the full first object and its next cursor. Setting page.repeat_object
to false makes the continuation return only object.ref and object_reused=true;
combine its evidence and links with the original object. The cursor still
validates the full object and selection, and rejects a changed source.

Inspect returns complete `next_actions`. When 512 bytes cannot fit the next
whole item, the retry offers at least `page.minimum_progress_bytes`. When the
complete inspection fits the usual 10,000-byte budget, it offers that complete
read, avoiding a separate call for every item.
Execute that call and keep following returned actions until `page.has_more`
is false. Retain the original object and append evidence, incoming/outgoing
links and raw records from each page. A fixed budget leaves the proof partial;
never treat it as absent. The native replay checks this action walk against
a complete inspection of the same stored source.

```python
while packet["page"]["has_more"]:
    action = packet["next_actions"][0]
    packet = call(action["tool"], action["arguments"])
    append_expansion_items(packet)
```

The calls below also show the optional object-reuse path. Here 512 bytes is too
small even for the expansion envelope, so this alternative continuation raises
the allowance to required_bytes, the exact size of the complete inspection
including its object. Reusing the object can otherwise
let proof fit the original ceiling. If an item still cannot fit, allow more
context or hand off the explicitly partial result. Keep about, ref and include
unchanged; count both actual responses. Do not overwrite the retained object
with the ref-only continuation.

```json
{
  "tool": "kmp_inspect",
  "save_as": "inspect_partial",
  "expect_partial": true,
  "arguments": {
    "about": "example:guide:budget-proof",
    "ref": "${result.generated_refs.0}",
    "budget": {"max_bytes": 512}
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "inspect_complete",
  "arguments": {
    "about": "example:guide:budget-proof",
    "ref": "${result.generated_refs.0}",
    "budget": {"max_bytes": "${inspect_partial.page.required_bytes}"},
    "page": {"cursor": "${inspect_partial.page.next_cursor}", "repeat_object": false}
  }
}
```

## A selection cap is not unfinished pagination

Here max_entries=1 deliberately caps the recall selection. Even after its
eligible expansion is fully paged, withheld evidence cannot be
recovered by pretending that another page exists. Compare selection_omitted
with page.has_more and excluded_by_detail. This compact recall is an
illustration of a capped selection, not a replacement for the completed
interval traversal. Report its limit explicitly; changing max_entries means
starting a new selection, not continuing this one. budget.tokens is advisory,
while max_bytes controls the native structured response subject to its floor.

```json
{
  "tool": "kmp_wake",
  "save_as": "capped_recall",
  "arguments": {
    "about": "example:guide:budget-proof",
    "budget": {"max_bytes": 20000, "max_entries": 1, "detail": "compact"}
  }
}
```

## Missing path: do not manufacture a connection

A second, independent audit asks whether R1 has a stored proof path to X1.
Inspecting both records has shown only export evidence and warehouse lighting.
Trace should report no path. Shared packet membership is not causal proof;
do not add follows or another fallback relation merely to make the audit pass.
View the packet and the original disconnected source before the final
semantic question. The viewer has its own reads; it does not consume an
outstanding MCP page or validate a missing proof path.

```json
{
  "tool": "kmp_trace",
  "save_as": "no_path",
  "arguments": {
    "about": "example:guide:budget-proof",
    "from": "${result.generated_refs.0}",
    "to": "${unrelated.generated_refs.0}",
    "budget": {"depth": 6, "max_bytes": 12000}
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "export_proof_frame",
  "arguments": {
    "expected_revision": "${packet_state.view_revision}",
    "idempotency_key": "guide-budget-proof:view-export_proof:v1",
    "explanation": "Review export proof in PACK-8",
    "focus": {"time_range": {"axis": "observed", "from": "2026-09-01T07:00:00Z", "to": "2026-09-01T13:00:00Z"}},
    "projection": {"semantic_zoom": "moment", "dimensions": ["packet"], "labels": [{"key": "packet", "op": "in", "values": ["PACK-8"]}, {"key": "component", "op": "in", "values": ["export"]}], "relation_classes": ["motivational", "evidential"]},
    "selection": "${result.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "export_proof_state",
  "arguments": {
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "unrelated_source_frame",
  "arguments": {
    "expected_revision": "${export_proof_state.view_revision}",
    "idempotency_key": "guide-budget-proof:view-unrelated_source:v1",
    "explanation": "Review unrelated source in PACK-8",
    "focus": {"time_range": {"axis": "observed", "from": "2026-09-01T07:00:00Z", "to": "2026-09-01T13:00:00Z"}},
    "projection": {"semantic_zoom": "moment", "dimensions": ["packet"], "labels": [{"key": "packet", "op": "in", "values": ["PACK-8"]}], "relation_classes": ["motivational", "evidential"]},
    "selection": "${unrelated.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "unrelated_source_state",
  "arguments": {
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "view_state",
  "arguments": {
  }
}
```

## Continue through the agent context

The earlier reads intentionally exercised complete calls without a context.
Register once for this inspection of D1, already written above:

```json
{
  "tool": "kmp_guide",
  "save_as": "short_context",
  "arguments": {"registration_key":"guide-budget-proof-short-reader"}
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "short_first",
  "expect_partial": true,
  "arguments": {
    "context_id":"${short_context.context_id}",
    "purpose":"audit",
    "about":"example:guide:budget-proof",
    "ref":"${decision.generated_refs.0}",
    "include":{"raw":true},
    "budget":{"max_bytes":512}
  }
}
```

The returned `next_actions[0].arguments` contains only `continuation` when the
short form fits. It already retains the context, D1 reference, raw inclusion and
larger allowance. Execute it directly:

```json
{
  "tool": "kmp_inspect",
  "save_as": "short_next",
  "expect_partial": true,
  "arguments":"${short_first.next_actions.0.arguments}"
}
```

If still partial, execute `short_next.next_actions[0]` and append the next page.
Reusing `short_first`'s identifier repeats that same page, not the next one.
Adding `budget`, `context_id` or another argument to the identifier is rejected:
use the returned action unchanged, or start a new explicit selection. An expired
identifier reports `CONTINUATION_UNAVAILABLE`; restart the original read and keep
its earlier pages marked partial. A changed source still triggers the native
cursor conflict. No identifier permits skipping HTTP grants or missing proof.

## Terminal semantic UNKNOWN

The final independent question is “¿Quién autorizó PURGE-9?”. Render it in
English without dropping PURGE-9, and preserve the user's wording in asked_as.
None of the sources names that operation or its approver. Read the returned
missing proof and UNKNOWN. At most one selection in the user's original
words is permitted after the English attempt; changing budgets or other
arguments is not another authorized semantic retry.

After the second UNKNOWN, stop this semantic investigation. Do not inspect
the about root, sweep other abouts, navigate time, or fabricate an approver
from the export history. Earlier traversal and audit calls above answer
different requests; they are not a fallback after this terminal result.

Ask and Wake also return executable `projection.next_action` calls. These
requests deliberately begin at 512 bytes, with the same clock, interval and
component filter. Send the returned `arguments` unchanged to its `tool`.
If `core_text_shortened` is true, the action omits `page.cursor`: discard that
partial reconstruction and restart at the proposed allowance to restore the
full core. Otherwise append only the expansions beyond each section's `core`
count. Continue until `next_action` is null; `has_more=false` alone only says
that expansion has finished. A fixed budget may leave the result partial.
A cursor conflict returns `feedback[].action` for a fresh selection; never
combine the old pages with the restarted selection.

Read `projection.sections` after each call. Its `remaining` values count only
eligible expansion after that page. Suppose `proof.evidence` reports core=5,
eligible=24, returned_on_page=0 on the first page: remaining=19. If `proof.path`
reports core=0, eligible=151, returned_on_page=4, remaining=147. These are
illustrative counts, not fixed totals for PACK-8. On later pages the counts
also subtract all earlier expansion, so do not subtract just the current page
yourself. A shortened core still requires a restart even when some expansion
has arrived. Zero remaining does not establish export completion or remove a
detail/selection cap. In temporal or inspect results the corresponding counts
are under `page.sections`; one remaining raw reference does not mean one
remaining source passage.

```json
{
  "tool": "kmp_wake",
  "save_as": "wake_action_start",
  "expect_partial": true,
  "arguments": {
    "about": "example:guide:budget-proof",
    "role": "reviewer", "intent": "Review offline export",
    "axis": "observed",
    "interval": {"start": "2026-09-01T08:00:00Z", "end": "2026-09-01T12:00:00Z"},
    "dimensions": {"selectors": [{"key": "component", "op": "in", "values": ["export"]}]},
    "budget": {"max_bytes": 512, "detail": "full"}
  }
}
```

```json
{
  "tool": "kmp_ask",
  "save_as": "ask_action_start",
  "expect_partial": true,
  "arguments": {
    "about": "example:guide:budget-proof",
    "question": "Why does export EXP-8 use SQLite without network access?",
    "asked_as": "¿Por qué la exportación EXP-8 usa SQLite sin acceso a la red?",
    "axis": "observed",
    "interval": {"start": "2026-09-01T08:00:00Z", "end": "2026-09-01T12:00:00Z"},
    "dimensions": {"selectors": [{"key": "component", "op": "in", "values": ["export"]}]},
    "budget": {"max_bytes": 512, "detail": "full"}
  }
}
```

The native replay executes these returned actions to completion and compares
all reconstructed sections against a full read of the same store. It does not
invent a cursor or recover a missing field from the lesson. The next two
questions concern an unrelated, unsupported authorization:

```json
{
  "tool": "kmp_ask",
  "save_as": "unknown_english",
  "arguments": {
    "about": "example:guide:budget-proof",
    "question": "Who authorized PURGE-9?",
    "asked_as": "¿Quién autorizó PURGE-9?",
    "budget": {"max_bytes": 20000, "detail": "full"}
  }
}
```

```json
{
  "tool": "kmp_ask",
  "save_as": "unknown_original",
  "arguments": {
    "about": "example:guide:budget-proof",
    "question": "¿Quién autorizó PURGE-9?",
    "asked_as": "¿Quién autorizó PURGE-9?",
    "budget": {"max_bytes": 20000, "detail": "full"}
  }
}
```

The result is UNKNOWN, not a guess about an authorizer. No tool call in
this authored lesson follows that terminal result.

Run `python3 scripts/guide_examples/replay.py --lesson budget-proof
--guide-mode markdown --binary <development-binary> --trace <new-jsonl>
--result <new-json>`. The checker assembles actual pages, checks source text,
interval bounds and directed proof, and retains the intermediate pending
continuations in its result. It invokes no model. For a separate visual
review, use --hold-view; viewer operations review the already defined packet,
not another attempt to answer PURGE-9.

Before using these examples with an independent LLM writer, give it only the
source history and guide. Reader questions, expected refs and assertions
belong to this teaching review, never to that writer's source input.
