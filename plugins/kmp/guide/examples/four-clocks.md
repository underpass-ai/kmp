# Worked example: four clocks and zoom to the proof

Use this fictional Atlas permit to separate when a decision was signed, when
the reviewer received it, when KMP recorded it and when it applied. The LLM
interprets the sources; KMP preserves the chosen coordinates. A fact that
applied before it was reported is not evidence that the reviewer knew it then.

Read the installed guide/AGENT.md and its exact reference to this lesson in
guide:kmp-agent. Consult time, writer, lifecycle and audit guidance when used.
Run only in the isolated about `example:guide:four-clocks`. This authored replay
checks the protocol; it does not measure an LLM's judgment on unseen sources.

## Resolve a real calendar before writing

Read the real UTC clock once. D4 is that UTC calendar day; D1, D2 and D3 are
three, two and one days earlier. D5 is the following midnight. In this synthetic
case, the archived reports become available together on D4. Preserve the source
reviewer's historical observation times. Ingestion is the real kernel commit
on D4, not an input the LLM fills in. If the run crosses UTC midnight, record
that fact and start a new isolated run with a new calendar.

For example, a run on September 8 uses September 5 as D1, September 6 as D2,
September 7 as D3 and September 9 as D5. Never send those sample dates blindly.
The replay reads the real clock and records the following `clock` bindings:

| Binding | Resolved UTC time |
| --- | --- |
| day1 | D1 00:00 |
| occurred | D1 09:00 |
| checked | D1 09:15 |
| day2 | D2 00:00, also the interval's exclusive end |
| day3 | D3 00:00 |
| observed | D3 14:00 |
| check_observed | D3 14:15 |
| receipt_observed | D3 14:30 |
| day4 | D4 00:00 |
| day5 | D5 00:00, exclusive validity end |

`${clock.occurred}` is a local calendar binding, not a tool result. Other
`${name...}` values copy returned results. Envelopes name `tool`, `arguments`
and `save_as`; send only arguments. The sources below use the same resolved
calendar. `expect_error` identifies an intentional refusal. Complete every
returned continuation before claiming a complete selection; this tiny story
fits its explicit limits and the replay stops if a page is incomplete.

## Sources available to the writer

All source text is fictional and literal within this scenario. The writer
gets these documents and the calendar, not the later reader's questions.

| Source | Occurred | First observed by reviewer | Literal source |
| --- | --- | --- | --- |
| S1, signed permit | D1 09:00 | D3 14:00 | Permit TEMP-4 allows Atlas to retry failed uploads from D2 00:00 UTC until D5 00:00 UTC, exclusive. |
| S2, signature check | D1 09:15 | D3 14:15 | CHECK-4 verifies the signature of permit TEMP-4 and the validity dates printed on it. |
| S3, delivery receipt | D2 00:00 | D3 14:30 | RECEIPT-4 confirms delivery of permit TEMP-4 to the Atlas operations mailbox. |

S1 is a decision with a stated validity interval. S2 is an observation about
signature verification, not an extension of that validity. S3 is feedback
confirming delivery, not approval of the permission or proof that the reviewer
read it at delivery time. Only S1 gets `valid_from` and `valid_until`.

## Recover, write and inspect the original sources

```json
{"tool":"kmp_wake","save_as":"initial","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000}},"expect_error":"not_found"}
```

A fresh missing about returns not_found. An existing about requires recovery
and catalogue reuse; do not treat it as empty.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "permit",
  "arguments": {
    "about": "example:guide:four-clocks",
    "intent": "record_decision",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-clocks:permit:v1",
    "scope": {
      "process": "atlas-permit-review"
    },
    "labels": {"document": ["TEMP-4"], "record": ["permit"]},
    "occurred_at": "${clock.occurred}",
    "observed_at": "${clock.observed}",
    "valid_from": "${clock.day2}",
    "valid_until": "${clock.day5}",
    "current": {
      "kind": "decision",
      "summary": "Permit TEMP-4 allows Atlas to retry failed uploads during its stated validity interval.",
      "evidence": "S1, signed permit: Permit TEMP-4 allows Atlas to retry failed uploads from D2 00:00 UTC until D5 00:00 UTC, exclusive."
    }
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"permit_read","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"ref":"${permit.generated_refs.0}"}}
```

```json
{"tool":"kmp_wake","save_as":"catalogue","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000}}}
```

Reuse document=TEMP-4 and the process returned in the catalogue. New record
values distinguish the permit from its verification and receipt. The signature
check supports the permit document; its why explains that limited support and
its evidence is the observed check. It does not prove permission before D2.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "signature",
  "arguments": {
    "about": "example:guide:four-clocks",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-clocks:signature:v1",
    "scope": {
      "process": "atlas-permit-review"
    },
    "labels": {"document": ["TEMP-4"], "record": ["signature-check"]},
    "occurred_at": "${clock.checked}",
    "observed_at": "${clock.check_observed}",
    "current": {
      "kind": "observation",
      "summary": "CHECK-4 verified the signature and printed validity dates of permit TEMP-4.",
      "evidence": "S2, signature check: CHECK-4 verifies the signature of permit TEMP-4 and the validity dates printed on it."
    },
    "read_context": {
      "inspected_refs": [
        "${permit.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${permit.generated_refs.0}",
        "rel": "supports",
        "class": "evidential",
        "confidence": "high",
        "why": "The recorded signature check supports the authenticity of this permit and its printed dates, not permission outside those dates.",
        "evidence": "S2: CHECK-4 verifies the signature of permit TEMP-4 and the validity dates printed on it."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"signature_read","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"ref":"${signature.generated_refs.0}"}}
```

The receipt refers to the inspected permit as background. It supplies no
source for a stronger authorization or causal claim.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "receipt",
  "arguments": {
    "about": "example:guide:four-clocks",
    "intent": "record_feedback",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-clocks:receipt:v1",
    "scope": {
      "process": "atlas-permit-review"
    },
    "labels": {"document": ["TEMP-4"], "record": ["delivery-receipt"]},
    "occurred_at": "${clock.day2}",
    "observed_at": "${clock.receipt_observed}",
    "current": {
      "kind": "feedback",
      "summary": "RECEIPT-4 confirms that permit TEMP-4 reached the Atlas operations mailbox.",
      "evidence": "S3, delivery receipt: RECEIPT-4 confirms delivery of permit TEMP-4 to the Atlas operations mailbox."
    },
    "read_context": {
      "inspected_refs": [
        "${permit.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${permit.generated_refs.0}",
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "The receipt names this permit as its subject; delivery alone does not establish a new approval or the time the reviewer read it.",
        "evidence": "S3: RECEIPT-4 confirms delivery of permit TEMP-4 to the Atlas operations mailbox."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"receipt_read","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"ref":"${receipt.generated_refs.0}"}}
```

## Enumerate a half-open interval without losing either boundary

For occurred [D1 09:00, D2 00:00), first keep only entries exactly at the
start from goto. Then read strictly later entries with forward, paginate if
needed, merge by ref and exclude times at or after D2 00:00. Expected: S1 and
S2; the receipt S3 lies exactly at the excluded end. Forward alone omits S1.
Rewind from the end independently checks the two earlier entries in this case.
Use the document lane consistently so extra label coordinates do not duplicate
entry counts. This is temporal enumeration and does not require Ask.

```json
{"tool":"kmp_goto","save_as":"occurred_start","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"at":{"time":"${clock.occurred}"},"axis":"occurred","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_forward","save_as":"occurred_later","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"from":{"time":"${clock.occurred}"},"axis":"occurred","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_rewind","save_as":"before_end","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"from":{"time":"${clock.day2}"},"axis":"occurred","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"]},"limit":{"entries":10}}}
```

## Stand at D2 with each of the four clocks

Filter record=permit to ask about the same S1 memory in each read. A clock
changes temporal admission, not the stored text. At D2 00:00:

| Axis | Expected S1 selection | Meaning |
| --- | --- | --- |
| occurred | Present | It was signed on D1. |
| observed | Absent | The reviewer first received it on D3. |
| ingested | Absent | KMP first recorded it on the real execution day D4. |
| validity | Present | The stated permission starts exactly at D2. |

Validity does not mean knowledge. A validity result at D2 must not be reported
as evidence that the reviewer or kernel knew the permit on D2.

```json
{"tool":"kmp_goto","save_as":"day2_occurred","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"at":{"time":"${clock.day2}"},"axis":"occurred","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"],"selectors":[{"key":"record","op":"in","values":["permit"]}]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_goto","save_as":"day2_observed","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"at":{"time":"${clock.day2}"},"axis":"observed","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"],"selectors":[{"key":"record","op":"in","values":["permit"]}]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_goto","save_as":"day2_ingested","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"at":{"time":"${clock.day2}"},"axis":"ingested","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"],"selectors":[{"key":"record","op":"in","values":["permit"]}]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_goto","save_as":"day2_validity","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"at":{"time":"${clock.day2}"},"axis":"validity","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"],"selectors":[{"key":"record","op":"in","values":["permit"]}]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_goto","save_as":"before_valid","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"at":{"time":"${clock.occurred}"},"axis":"validity","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"],"selectors":[{"key":"record","op":"in","values":["permit"]}]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_goto","save_as":"expired","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"at":{"time":"${clock.day5}"},"axis":"validity","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"],"selectors":[{"key":"record","op":"in","values":["permit"]}]},"limit":{"entries":10}}}
```

Before D2 and at the exclusive D5 end, validity returns no permit. Its
source and history remain stored; expiry does not delete the decision.
Now traverse the actual observation and ingestion timelines. Inspect gave
all label coordinates the same ingestion time for S1; copy one returned
`ingested_at` exactly when locating its real commit, rather than inventing it.

```json
{"tool":"kmp_goto","save_as":"observed_start","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"at":{"time":"${clock.observed}"},"axis":"observed","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_forward","save_as":"observed_later","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"from":{"time":"${clock.observed}"},"axis":"observed","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_forward","save_as":"ingested_day4","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"from":{"time":"${clock.day4}"},"axis":"ingested","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_goto","save_as":"ingested_exact","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"at":{"time":"${permit_read.links.incoming.0.coordinate.ingested_at}"},"axis":"ingested","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"],"selectors":[{"key":"record","op":"in","values":["permit"]}]},"limit":{"entries":10}}}
```

## Refuse a forged ingestion clock and audit the support

The following intentional negative call copies the valid signature-write
arguments and adds an unsupported writer field. It must fail before writing.
The normal writer has no ingested_at input. Canonical migration/replay is a
different operation; it is not a way to backdate a new writer observation.

```json
{"tool":"kmp_write_memory","save_as":"forged_ingestion","arguments":{"about":"example:guide:four-clocks","intent":"record_observation","actor":"guide-writer","source_kind":"human","idempotency_key":"guide-clocks:forged-ingestion:v1","scope":{"process":"atlas-permit-review"},"labels":{"document": ["TEMP-4"], "record": ["signature-check"]},"occurred_at":"${clock.checked}","observed_at":"${clock.check_observed}","current":{"kind":"observation","summary":"CHECK-4 verified the signature and printed validity dates of permit TEMP-4.","evidence":"S2, signature check: CHECK-4 verifies the signature of permit TEMP-4 and the validity dates printed on it."},"read_context":{"inspected_refs":["${permit.generated_refs.0}"]},"connect_to":[{"ref":"${permit.generated_refs.0}","rel":"supports","class":"evidential","confidence":"high","why":"The recorded signature check supports the authenticity of this permit and its printed dates, not permission outside those dates.","evidence":"S2: CHECK-4 verifies the signature of permit TEMP-4 and the validity dates printed on it."}],"ingested_at":"${clock.day1}"},"expect_error":"invalid_argument"}
```

```json
{"tool":"kmp_forward","save_as":"after_refusal","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"from":{"time":"${clock.day4}"},"axis":"ingested","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_trace","save_as":"signature_path","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"from":"${signature.generated_refs.0}","to":"${permit.generated_refs.0}"}}
```

The final selection still has three memories. Trace must preserve the
signature check’s direction, why and evidence. It does not invent a proof
that receipt means approval.

## Zoom, switch clocks and return to the evidence

Open once. Every subsequent intent uses the returned revision; on a conflict,
read current state and rebase deliberately. Start at Atlas over all four days,
narrow to [D1 09:00,D2 00:00) at Episode, then choose Moment to read the
permit and its signature proof. Zoom changes the representation. Selecting a
ref does not make a hidden memory part of a projection. The inspector reads
the stored object; it is not an independent historical query.

```json
{"tool":"kmp_view_open","save_as":"opened","arguments":{"about":"example:guide:four-clocks"}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"overview_frame","arguments":{"idempotency_key":"guide-clocks:view-overview:v1","expected_revision":"${opened.view_revision}","explanation":"Inspect the permit using occurred time at atlas detail","focus":{"time_range":{"axis":"occurred","from":"${clock.day1}","to":"${clock.day5}"}},"projection":{"semantic_zoom":"atlas","dimensions":["document"],"labels":[{"key":"document","op":"in","values":["TEMP-4"]}]},"selection":null}}
```

```json
{"tool":"kmp_view_get_state","save_as":"overview_state","arguments":{}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"zoom_frame","arguments":{"idempotency_key":"guide-clocks:view-zoom:v1","expected_revision":"${overview_state.view_revision}","explanation":"Inspect the permit using occurred time at episode detail","focus":{"time_range":{"axis":"occurred","from":"${clock.occurred}","to":"${clock.day2}"}},"projection":{"semantic_zoom":"episode","dimensions":["document"],"labels":[{"key":"document","op":"in","values":["TEMP-4"]}]},"selection":null}}
```

```json
{"tool":"kmp_view_get_state","save_as":"zoom_state","arguments":{}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"detail_frame","arguments":{"idempotency_key":"guide-clocks:view-detail:v1","expected_revision":"${zoom_state.view_revision}","explanation":"Inspect the permit using occurred time at moment detail","focus":{"time_range":{"axis":"occurred","from":"${clock.occurred}","to":"${clock.day2}"}},"projection":{"semantic_zoom":"moment","dimensions":["document"],"labels":[{"key":"document","op":"in","values":["TEMP-4"]}]},"selection":"${permit.generated_refs.0}"}}
```

```json
{"tool":"kmp_view_get_state","save_as":"detail_state","arguments":{}}
```

Keep the narrow window but switch to observed: it should be empty. Then
move the observed window to D3, where all three reports were first known.
An empty window is not proof that these memories were never written.

```json
{"tool":"kmp_view_apply_intent","save_as":"observed_empty_frame","arguments":{"idempotency_key":"guide-clocks:view-observed_empty:v1","expected_revision":"${detail_state.view_revision}","explanation":"Inspect the permit using observed time at moment detail","focus":{"time_range":{"axis":"observed","from":"${clock.occurred}","to":"${clock.day2}"}},"projection":{"semantic_zoom":"moment","dimensions":["document"],"labels":[{"key":"document","op":"in","values":["TEMP-4"]}]},"selection":null}}
```

```json
{"tool":"kmp_view_get_state","save_as":"observed_empty_state","arguments":{}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"observed_frame","arguments":{"idempotency_key":"guide-clocks:view-observed:v1","expected_revision":"${observed_empty_state.view_revision}","explanation":"Inspect the permit using observed time at moment detail","focus":{"time_range":{"axis":"observed","from":"${clock.observed}","to":"${clock.day4}"}},"projection":{"semantic_zoom":"moment","dimensions":["document"],"labels":[{"key":"document","op":"in","values":["TEMP-4"]}]},"selection":"${permit.generated_refs.0}"}}
```

```json
{"tool":"kmp_view_get_state","save_as":"observed_state","arguments":{}}
```

The ingested window is the real execution day D4. Validity on D2 instead
filters to record=permit and shows the permission interval. Neither move
changes an earlier MCP read: subsequent temporal calls still need their own
explicit axis, cursor, dimensions and budget.

```json
{"tool":"kmp_view_apply_intent","save_as":"ingested_frame","arguments":{"idempotency_key":"guide-clocks:view-ingested:v1","expected_revision":"${observed_state.view_revision}","explanation":"Inspect the permit using ingested time at moment detail","focus":{"time_range":{"axis":"ingested","from":"${clock.day4}","to":"${clock.day5}"}},"projection":{"semantic_zoom":"moment","dimensions":["document"],"labels":[{"key":"document","op":"in","values":["TEMP-4"]}]},"selection":"${permit.generated_refs.0}"}}
```

```json
{"tool":"kmp_view_get_state","save_as":"ingested_state","arguments":{}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"validity_frame","arguments":{"idempotency_key":"guide-clocks:view-validity:v1","expected_revision":"${ingested_state.view_revision}","explanation":"Inspect the permit using validity time at moment detail","focus":{"time_range":{"axis":"validity","from":"${clock.day2}","to":"${clock.day3}"}},"projection":{"semantic_zoom":"moment","dimensions":["document"],"labels":[{"key":"document","op":"in","values":["TEMP-4"]},{"key":"record","op":"in","values":["permit"]}]},"selection":"${permit.generated_refs.0}"}}
```

```json
{"tool":"kmp_view_get_state","save_as":"validity_state","arguments":{}}
```

```json
{"tool":"kmp_view_get_state","save_as":"view_state","arguments":{}}
```

```json
{"tool":"kmp_goto","save_as":"occurred_after_view","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"at":{"time":"${clock.occurred}"},"axis":"occurred","dimensions":{"mode":"only","include":["document"],"scope_ids":["TEMP-4"]},"limit":{"entries":10}}}
```

```json
{"tool":"kmp_inspect","save_as":"permit_after_view","arguments":{"about":"example:guide:four-clocks","budget":{"max_bytes":30000},"ref":"${permit.generated_refs.0}"}}
```

Compare occurred_after_view with occurred_start: the explicit native read
returns the same S1 entry even while the viewer stands on validity. The stored
object is unchanged. View state is shared presentation, not an implicit query
context and not a memory write.

This lesson establishes four distinct clocks, both interval boundaries, expiry
and a proof-directed zoom using three justified memory kinds. It does not
claim that every aggregation level exposes every relationship, nor that an LLM
has learned to make these choices. Inspect the source when the picture alone
cannot answer. Continue any incomplete native page before drawing conclusions.
