# Worked example: quantities, corrections and a source-backed total

Recover a trip's documents, follow corrections and exclusions, then calculate
with the verified operands. KMP preserves the graph; an LLM interprets sources
and a calculator performs arithmetic. Successful writer validation does not
verify arithmetic, detect a duplicate or convert a currency.

Read guide/AGENT.md once, this lesson, and the extended writer, time, scope,
relations, audit and viewer guidance when used. This isolated authored replay
is teaching material, not a measurement of LLM learning. It has no future reader
question or expected answer in the source-writing phase.

## Literal fictional sources

All dates below are UTC in September 2026. An expense happened on September 1;
later documents can correct or cancel it without becoming new expenses. Keep
each document's occurrence and observation as listed. Run after September 4
09:00 UTC. Read the real clock before the new calculation: `clock.calculated`
is that instant and `clock.until` is the next UTC midnight. Ingestion belongs
to the kernel. The sources do not contain a precomputed total.

| Source | Document occurred / first observed | Literal source |
| --- | --- | --- |
| P7 | Sep 1 08:00 / 08:00 | TRIP-7 reimburses settled EUR charges including tax. Count each transaction once, use its explicitly corrected amount and exclude voided charges. Other currencies require a separate subtotal; no conversion rate is supplied. |
| R-TRAIN | Sep 1 09:00 / 09:00 | TRIP-7 transaction TX-TRAIN: settled transport charge 20.00 EUR, tax included. |
| H-0 | Sep 1 10:00 / 10:00 | TRIP-7 transaction TX-HOTEL: hotel invoice initially reports 30.00 EUR, tax included. |
| M-1 | Sep 1 11:00 / 11:00 | TRIP-7 transaction TX-MEAL: posted meal charge 10.00 EUR, tax included. |
| U-1 | Sep 1 12:00 / 12:00 | TRIP-7 transaction TX-USD: settled local transport charge 7.00 USD, tax included. |
| H-1 | Sep 2 10:00 / 10:00 | TRIP-7 correction H-1 replaces H-0 for transaction TX-HOTEL: the settled hotel amount is 35.00 EUR including tax, not 30.00 EUR. This is not a second transaction. |
| H-1-C | Sep 2 11:00 / 11:00 | TRIP-7 copy H-1-C reproduces H-1 for transaction TX-HOTEL, 35.00 EUR including tax. It is a duplicate document, not another charge. |
| V-MEAL | Sep 3 11:00 / 11:00 | TRIP-7 transaction TX-MEAL from M-1 was voided in full. Its posted 10.00 EUR leaves 0.00 EUR settled. |
| STMT-7 | Sep 4 09:00 / 09:00 | TRIP-7 closing statement: TX-TRAIN settled 20.00 EUR; TX-HOTEL settled 35.00 EUR; TX-MEAL voided, settled 0.00 EUR; TX-USD settled 7.00 USD. No currency conversion rate. |

The policy is a constraint, the receipts/correction/copy/statement are
observations, and the merchant's void confirmation is feedback. A correction
is a document about the same transaction, not an extra operand. The shared
transaction label finds documents; only H-1-C's literal statement justifies
`same_event_as`. An unrelated equal amount would not justify that link.

The JSON envelopes are teaching notation. Send only `arguments`; bindings
copy returned refs. Read every relevant continuation before calculating. The
small selections below fit the explicit limits; the replay refuses partial
pages instead of treating them as complete.

## Write the sources without anticipating the total

```json
{"tool": "kmp_wake", "save_as": "initial", "arguments": {"about": "example:guide:quantities", "budget": {"max_bytes": 40000}}, "expect_error": "not_found"}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "policy",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-quantities:policy:v1",
    "observed_at": "2026-09-01T08:00:00Z",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "P7",
      "role": "rule"
    },
    "current": {
      "kind": "constraint",
      "summary": "P7 limits TRIP-7 settlement to unique settled EUR transactions, corrected amounts and no voids.",
      "evidence": "P7: TRIP-7 reimburses settled EUR charges including tax. Count each transaction once, use its explicitly corrected amount and exclude voided charges. Other currencies require a separate subtotal; no conversion rate is supplied."
    },
    "occurred_at": "2026-09-01T08:00:00Z"
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "policy_read", "arguments": {"about": "example:guide:quantities", "ref": "${policy.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_wake", "save_as": "catalogue", "arguments": {"about": "example:guide:quantities", "budget": {"max_bytes": 40000}}}
```

Reuse trip=TRIP-7 and the process returned by wake. Source, rule, result and
audit are different roles. Currency is a unit filter, not a conversion rule.
New document IDs H-1 and H-1-C are intentionally distinct from H-0; after
reading the catalogue, `labels_new` declares that intent without renaming it.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "train",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-quantities:train:v1",
    "observed_at": "2026-09-01T09:00:00Z",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "R-TRAIN",
      "transaction": "TX-TRAIN",
      "currency": "EUR",
      "role": "source"
    },
    "current": {
      "kind": "observation",
      "summary": "TRIP-7 transaction TX-TRAIN: settled transport charge 20.00 EUR, tax included.",
      "evidence": "R-TRAIN: TRIP-7 transaction TX-TRAIN: settled transport charge 20.00 EUR, tax included."
    },
    "occurred_at": "2026-09-01T09:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${policy.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${policy.generated_refs.0}",
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "The writer uses the TRIP-7 settlement rule to catalogue this source; the rule does not prove that the charge is eligible.",
        "evidence": "P7 applies to TRIP-7; R-TRAIN identifies TRIP-7 as its trip."
      }
    ]
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "train_read", "arguments": {"about": "example:guide:quantities", "ref": "${train.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "hotel_old",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-quantities:hotel_old:v1",
    "observed_at": "2026-09-01T10:00:00Z",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "H-0",
      "transaction": "TX-HOTEL",
      "currency": "EUR",
      "role": "source"
    },
    "current": {
      "kind": "observation",
      "summary": "TRIP-7 transaction TX-HOTEL: hotel invoice initially reports 30.00 EUR, tax included.",
      "evidence": "H-0: TRIP-7 transaction TX-HOTEL: hotel invoice initially reports 30.00 EUR, tax included."
    },
    "occurred_at": "2026-09-01T10:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${policy.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${policy.generated_refs.0}",
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "The writer uses the TRIP-7 settlement rule to catalogue this source; the rule does not prove that the charge is eligible.",
        "evidence": "P7 applies to TRIP-7; H-0 identifies TRIP-7 as its trip."
      }
    ]
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "hotel_old_read", "arguments": {"about": "example:guide:quantities", "ref": "${hotel_old.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "meal",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-quantities:meal:v1",
    "observed_at": "2026-09-01T11:00:00Z",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "M-1",
      "transaction": "TX-MEAL",
      "currency": "EUR",
      "role": "source"
    },
    "current": {
      "kind": "observation",
      "summary": "TRIP-7 transaction TX-MEAL: posted meal charge 10.00 EUR, tax included.",
      "evidence": "M-1: TRIP-7 transaction TX-MEAL: posted meal charge 10.00 EUR, tax included."
    },
    "occurred_at": "2026-09-01T11:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${policy.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${policy.generated_refs.0}",
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "The writer uses the TRIP-7 settlement rule to catalogue this source; the rule does not prove that the charge is eligible.",
        "evidence": "P7 applies to TRIP-7; M-1 identifies TRIP-7 as its trip."
      }
    ]
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "meal_read", "arguments": {"about": "example:guide:quantities", "ref": "${meal.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "usd",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-quantities:usd:v1",
    "observed_at": "2026-09-01T12:00:00Z",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "U-1",
      "transaction": "TX-USD",
      "currency": "USD",
      "role": "source"
    },
    "current": {
      "kind": "observation",
      "summary": "TRIP-7 transaction TX-USD: settled local transport charge 7.00 USD, tax included.",
      "evidence": "U-1: TRIP-7 transaction TX-USD: settled local transport charge 7.00 USD, tax included."
    },
    "occurred_at": "2026-09-01T12:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${policy.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${policy.generated_refs.0}",
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "The writer uses the TRIP-7 settlement rule to catalogue this source; the rule does not prove that the charge is eligible.",
        "evidence": "P7 applies to TRIP-7; U-1 identifies TRIP-7 as its trip."
      }
    ]
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "usd_read", "arguments": {"about": "example:guide:quantities", "ref": "${usd.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_rewind", "save_as": "before_corrections", "arguments": {"about": "example:guide:quantities", "from": {"time": "2026-09-02T00:00:00Z"}, "axis": "observed", "dimensions": {"mode": "only", "include": ["trip"], "scope_ids": ["TRIP-7"], "selectors": [{"key": "currency", "op": "in", "values": ["EUR"]}]}, "limit": {"entries": 20}, "budget": {"max_bytes": 40000}}}
```

At this observed cutoff the EUR slice contains R-TRAIN, H-0 and M-1. It does
not contain a corrected invoice, a void confirmation or a final total. Adding
these figures gives a raw posted-document sum, not the policy's final settled
reimbursement. Do not silently apply later knowledge to this earlier window.

Now record the later documents with their actual later document clocks.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "hotel_final",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-quantities:hotel_final:v1",
    "observed_at": "2026-09-02T10:00:00Z",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "H-1",
      "transaction": "TX-HOTEL",
      "currency": "EUR",
      "role": "source"
    },
    "current": {
      "kind": "observation",
      "summary": "TRIP-7 correction H-1 replaces H-0 for transaction TX-HOTEL: the settled hotel amount is 35.00 EUR including tax, not 30.00 EUR. This is not a second transaction.",
      "evidence": "H-1: TRIP-7 correction H-1 replaces H-0 for transaction TX-HOTEL: the settled hotel amount is 35.00 EUR including tax, not 30.00 EUR. This is not a second transaction."
    },
    "occurred_at": "2026-09-02T10:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${hotel_old.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${hotel_old.generated_refs.0}",
        "rel": "corrects",
        "class": "evidential",
        "confidence": "high",
        "why": "The final invoice corrects the amount of this same hotel transaction; it must replace, not be added to, the preliminary figure.",
        "evidence": "H-1 explicitly replaces H-0 for TX-HOTEL with settled 35.00 EUR instead of 30.00 EUR."
      }
    ],
    "options": {
      "labels_new": [
        "document"
      ]
    }
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "hotel_final_read", "arguments": {"about": "example:guide:quantities", "ref": "${hotel_final.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "hotel_copy",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-quantities:hotel_copy:v1",
    "observed_at": "2026-09-02T11:00:00Z",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "H-1-C",
      "transaction": "TX-HOTEL",
      "currency": "EUR",
      "role": "source"
    },
    "current": {
      "kind": "observation",
      "summary": "TRIP-7 copy H-1-C reproduces H-1 for transaction TX-HOTEL, 35.00 EUR including tax. It is a duplicate document, not another charge.",
      "evidence": "H-1-C: TRIP-7 copy H-1-C reproduces H-1 for transaction TX-HOTEL, 35.00 EUR including tax. It is a duplicate document, not another charge."
    },
    "occurred_at": "2026-09-02T11:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${hotel_final.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${hotel_final.generated_refs.0}",
        "rel": "same_event_as",
        "class": "evidential",
        "confidence": "high",
        "why": "The copy and final invoice report one hotel charge; the explicit transaction and copy statement justify counting it once.",
        "evidence": "H-1-C says it reproduces H-1 for TX-HOTEL and is not another charge."
      }
    ],
    "options": {
      "labels_new": [
        "document"
      ]
    }
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "hotel_copy_read", "arguments": {"about": "example:guide:quantities", "ref": "${hotel_copy.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "void",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_feedback",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-quantities:void:v1",
    "observed_at": "2026-09-03T11:00:00Z",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "V-MEAL",
      "transaction": "TX-MEAL",
      "currency": "EUR",
      "role": "source"
    },
    "current": {
      "kind": "feedback",
      "summary": "TRIP-7 transaction TX-MEAL from M-1 was voided in full. Its posted 10.00 EUR leaves 0.00 EUR settled.",
      "evidence": "V-MEAL: TRIP-7 transaction TX-MEAL from M-1 was voided in full. Its posted 10.00 EUR leaves 0.00 EUR settled."
    },
    "occurred_at": "2026-09-03T11:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${meal.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${meal.generated_refs.0}",
        "rel": "updates_state",
        "class": "causal",
        "confidence": "high",
        "why": "The merchant confirmation changes this transaction from posted to fully voided, so its charge is not a settled expense.",
        "evidence": "V-MEAL names M-1 and TX-MEAL, voids the full 10.00 EUR and reports 0.00 EUR settled."
      }
    ]
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "void_read", "arguments": {"about": "example:guide:quantities", "ref": "${void.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "statement",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-quantities:statement:v1",
    "observed_at": "2026-09-04T09:00:00Z",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "STMT-7",
      "role": "source"
    },
    "current": {
      "kind": "observation",
      "summary": "TRIP-7 closing statement: TX-TRAIN settled 20.00 EUR; TX-HOTEL settled 35.00 EUR; TX-MEAL voided, settled 0.00 EUR; TX-USD settled 7.00 USD. No currency conversion rate.",
      "evidence": "STMT-7: TRIP-7 closing statement: TX-TRAIN settled 20.00 EUR; TX-HOTEL settled 35.00 EUR; TX-MEAL voided, settled 0.00 EUR; TX-USD settled 7.00 USD. No currency conversion rate."
    },
    "occurred_at": "2026-09-04T09:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${hotel_final.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${hotel_final.generated_refs.0}",
        "rel": "supports",
        "class": "evidential",
        "confidence": "high",
        "why": "The closing statement independently confirms the corrected settled hotel figure; it does not create another charge.",
        "evidence": "STMT-7 lists TX-HOTEL settled at 35.00 EUR, matching H-1."
      }
    ]
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "statement_read", "arguments": {"about": "example:guide:quantities", "ref": "${statement.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_wake", "save_as": "sources_catalogue", "arguments": {"about": "example:guide:quantities", "budget": {"max_bytes": 50000, "detail": "compact"}}}
```

## Recover candidates, then follow the proof

Use the trip lane for traversal and currency/role selectors for membership.
Selectors read all labels even when the returned coordinates only show trip.
The EUR source slice contains six documents, not six independent charges.
Its earliest source is after September 1 midnight, so forward cannot omit an
entry at that boundary in this fixture. A general interval also needs the
inclusive goto probe. Read H-0 -> H-1 as a correction by following the stored
H-1 -> H-0 edge; direction matters. The copy and void have their own proof.

```json
{"tool": "kmp_forward", "save_as": "eur_candidates", "arguments": {"about": "example:guide:quantities", "from": {"time": "2026-09-01T00:00:00Z"}, "axis": "observed", "dimensions": {"mode": "only", "include": ["trip"], "scope_ids": ["TRIP-7"], "selectors": [{"key": "currency", "op": "in", "values": ["EUR"]}, {"key": "role", "op": "in", "values": ["source"]}]}, "limit": {"entries": 20}, "budget": {"max_bytes": 50000}}}
```

```json
{"tool": "kmp_forward", "save_as": "usd_candidates", "arguments": {"about": "example:guide:quantities", "from": {"time": "2026-09-01T00:00:00Z"}, "axis": "observed", "dimensions": {"mode": "only", "include": ["trip"], "scope_ids": ["TRIP-7"], "selectors": [{"key": "currency", "op": "in", "values": ["USD"]}, {"key": "role", "op": "in", "values": ["source"]}]}, "limit": {"entries": 20}, "budget": {"max_bytes": 50000}}}
```

```json
{"tool": "kmp_trace", "save_as": "correction_path", "arguments": {"about": "example:guide:quantities", "from": "${hotel_final.generated_refs.0}", "to": "${hotel_old.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_trace", "save_as": "duplicate_path", "arguments": {"about": "example:guide:quantities", "from": "${hotel_copy.generated_refs.0}", "to": "${hotel_final.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_trace", "save_as": "void_path", "arguments": {"about": "example:guide:quantities", "from": "${void.generated_refs.0}", "to": "${meal.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_inspect", "save_as": "train_operand", "arguments": {"about": "example:guide:quantities", "ref": "${train.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_inspect", "save_as": "hotel_final_operand", "arguments": {"about": "example:guide:quantities", "ref": "${hotel_final.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_inspect", "save_as": "statement_operand", "arguments": {"about": "example:guide:quantities", "ref": "${statement.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

## Calculate only from the verified operands

| Document | Treatment | Source of the choice |
| --- | --- | --- |
| R-TRAIN | +20.00 EUR | Settled TX-TRAIN, also confirmed by STMT-7. |
| H-0 | Replaced, do not add 30.00 EUR | H-1 explicitly corrects this transaction. |
| H-1 | +35.00 EUR | Corrected settled TX-HOTEL. |
| H-1-C | Duplicate, do not add 35.00 EUR again | Explicit same-event proof to H-1. |
| M-1 | Exclude posted 10.00 EUR | V-MEAL confirms a full void. |
| V-MEAL | Evidence of the void, not a new negative expense | It reports zero settled, not a separate refund to subtract again. |
| U-1 | Separate 7.00 USD; outside this EUR total | No conversion rate. |

Use decimal arithmetic or integer cents: 2,000 + 3,500 = 5,500 EUR cents,
therefore **55.00 EUR**. Do not add the old invoice or copy, subtract a voided
charge a second time, add tax already included, or sum 55.00 EUR + 7.00 USD
as 62.00 EUR. Without a conversion source the all-currency converted total
is undetermined. The authored check recalculates from retrieved operand text;
it does not turn these scenario-specific choices into an automatic resolver.

Now create a new derived value at the real calculation time. Its `total_of`
links point only to included operands. `checked_against` uses class constraint
for the rule and closing statement; evidence does not force class evidential.
The direct evidence records units, formula and exclusions. This is a calculated
EUR settlement subtotal, not a new primary receipt or a universal trip total.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "total",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "derived",
    "idempotency_key": "guide-quantities:total:v1",
    "observed_at": "${clock.calculated}",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "CALC-7",
      "currency": "EUR",
      "role": "result"
    },
    "current": {
      "kind": "derived_value",
      "summary": "TRIP-7 settled EUR subtotal is 55.00 EUR from TX-TRAIN and corrected TX-HOTEL.",
      "evidence": "Calculation for TRIP-7 from inspected sources: R-TRAIN 20.00 EUR + corrected H-1 35.00 EUR = 55.00 EUR (2000 + 3500 = 5500 cents). H-1 replaces H-0, so omit 30.00 EUR; H-1-C explicitly copies H-1, so count TX-HOTEL once; V-MEAL voids M-1 fully, so omit 10.00 EUR without subtracting again. U-1 is 7.00 USD, kept separate without conversion. STMT-7 confirms the settled component amounts; P7 requires unique settled EUR charges with tax already included."
    },
    "read_context": {
      "inspected_refs": [
        "${train.generated_refs.0}",
        "${hotel_final.generated_refs.0}",
        "${policy.generated_refs.0}",
        "${statement.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${train.generated_refs.0}",
        "rel": "total_of",
        "class": "evidential",
        "confidence": "high",
        "why": "The subtotal includes this distinct settled transport charge exactly once.",
        "evidence": "R-TRAIN records TX-TRAIN settled 20.00 EUR including tax; STMT-7 confirms it."
      },
      {
        "ref": "${hotel_final.generated_refs.0}",
        "rel": "total_of",
        "class": "evidential",
        "confidence": "high",
        "why": "The subtotal includes the corrected hotel amount once for TX-HOTEL, excluding its old value and duplicate document.",
        "evidence": "H-1 replaces H-0 with 35.00 EUR for TX-HOTEL; H-1-C says it is the same charge."
      },
      {
        "ref": "${policy.generated_refs.0}",
        "rel": "checked_against",
        "class": "constraint",
        "confidence": "high",
        "why": "The calculation is checked against the rule requiring unique settled EUR charges and no voids or currency conversion.",
        "evidence": "P7 states count each transaction once, use corrected amounts, exclude voids and separate currencies."
      },
      {
        "ref": "${statement.generated_refs.0}",
        "rel": "checked_against",
        "class": "constraint",
        "confidence": "high",
        "why": "The selected component amounts and excluded void are checked against the closing settlement record.",
        "evidence": "STMT-7 lists 20.00 EUR and 35.00 EUR settled, the meal at 0.00 EUR, and the separate 7.00 USD charge."
      }
    ]
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "total_read", "arguments": {"about": "example:guide:quantities", "ref": "${total.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

Keep exclusion decisions auditable without rewriting receipt objects or
retroactively changing their clocks. The next memory is the agent's new audit
of omitted candidates, not another merchant charge. Its `excluded_from` edge
points **from the exclusion record to the total**; `derived_from` and
`chosen_because` point to the documents justifying the decision. Inspect this
record to reach the duplicate, correction, void and currency source.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "exclusions",
  "arguments": {
    "about": "example:guide:quantities",
    "intent": "record_decision",
    "actor": "guide-writer",
    "source_kind": "derived",
    "idempotency_key": "guide-quantities:exclusions:v1",
    "observed_at": "${clock.calculated}",
    "scope": {
      "process": "trip-settlement"
    },
    "labels": {
      "trip": "TRIP-7",
      "document": "AUDIT-7",
      "role": "audit"
    },
    "current": {
      "kind": "decision",
      "summary": "Exclude H-0, duplicate H-1-C, voided M-1 and USD receipt U-1 from the TRIP-7 EUR subtotal CALC-7.",
      "evidence": "Audit of CALC-7: H-1 corrects H-0 from 30.00 EUR to 35.00 EUR for the same TX-HOTEL; H-1-C explicitly duplicates H-1. V-MEAL voids M-1, leaving 0.00 EUR settled. U-1 is 7.00 USD and P7 supplies no conversion rate. These omissions prevent double counting and mixed units."
    },
    "read_context": {
      "inspected_refs": [
        "${total.generated_refs.0}",
        "${hotel_old.generated_refs.0}",
        "${hotel_copy.generated_refs.0}",
        "${meal.generated_refs.0}",
        "${usd.generated_refs.0}",
        "${hotel_final.generated_refs.0}",
        "${void.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${total.generated_refs.0}",
        "rel": "excluded_from",
        "class": "constraint",
        "confidence": "high",
        "why": "These documented candidates are deliberately omitted from this EUR subtotal for replacement, duplicate, void or unit mismatch reasons.",
        "evidence": "H-1 replaces H-0; H-1-C duplicates H-1; V-MEAL voids M-1; U-1 is USD and P7 supplies no conversion."
      },
      {
        "ref": "${hotel_old.generated_refs.0}",
        "rel": "derived_from",
        "class": "evidential",
        "confidence": "high",
        "why": "This is the preliminary operand omitted after its explicit correction.",
        "evidence": "H-0 reports 30.00 EUR for TX-HOTEL; H-1 replaces that amount."
      },
      {
        "ref": "${hotel_copy.generated_refs.0}",
        "rel": "derived_from",
        "class": "evidential",
        "confidence": "high",
        "why": "The duplicate document identifies the extra 35.00 EUR that must not become a second operand.",
        "evidence": "H-1-C explicitly reproduces H-1 for the same TX-HOTEL."
      },
      {
        "ref": "${meal.generated_refs.0}",
        "rel": "derived_from",
        "class": "evidential",
        "confidence": "high",
        "why": "This is the posted charge removed from the eligible set by its later full void.",
        "evidence": "M-1 reports TX-MEAL posted 10.00 EUR; V-MEAL names and voids it."
      },
      {
        "ref": "${usd.generated_refs.0}",
        "rel": "derived_from",
        "class": "evidential",
        "confidence": "high",
        "why": "The foreign-currency receipt proves a unit mismatch for the EUR subtotal, not a zero-valued expense.",
        "evidence": "U-1 records 7.00 USD; no conversion rate is supplied."
      },
      {
        "ref": "${hotel_final.generated_refs.0}",
        "rel": "chosen_because",
        "class": "motivational",
        "confidence": "high",
        "why": "The explicit replacement is the reason to omit H-0 while retaining the corrected hotel operand.",
        "evidence": "H-1 says it replaces H-0 for the same transaction with 35.00 EUR."
      },
      {
        "ref": "${void.generated_refs.0}",
        "rel": "chosen_because",
        "class": "motivational",
        "confidence": "high",
        "why": "The merchant full-void confirmation is the reason to exclude M-1 under P7.",
        "evidence": "V-MEAL confirms all 10.00 EUR were voided and 0.00 EUR remains settled."
      }
    ]
  }
}
```

```json
{"tool": "kmp_inspect", "save_as": "exclusions_read", "arguments": {"about": "example:guide:quantities", "ref": "${exclusions.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_trace", "save_as": "total_train_path", "arguments": {"about": "example:guide:quantities", "from": "${total.generated_refs.0}", "to": "${train.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_trace", "save_as": "total_hotel_path", "arguments": {"about": "example:guide:quantities", "from": "${total.generated_refs.0}", "to": "${hotel_final.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_trace", "save_as": "exclusion_path", "arguments": {"about": "example:guide:quantities", "from": "${exclusions.generated_refs.0}", "to": "${total.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_trace", "save_as": "exclusion_void_path", "arguments": {"about": "example:guide:quantities", "from": "${exclusions.generated_refs.0}", "to": "${void.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_inspect", "save_as": "hotel_old_after", "arguments": {"about": "example:guide:quantities", "ref": "${hotel_old.generated_refs.0}", "budget": {"max_bytes": 40000}}}
```

```json
{"tool": "kmp_rewind", "save_as": "before_corrections_again", "arguments": {"about": "example:guide:quantities", "from": {"time": "2026-09-02T00:00:00Z"}, "axis": "observed", "dimensions": {"mode": "only", "include": ["trip"], "scope_ids": ["TRIP-7"], "selectors": [{"key": "currency", "op": "in", "values": ["EUR"]}]}, "limit": {"entries": 20}, "budget": {"max_bytes": 40000}}}
```

The earlier observed selection must still contain only its original three
EUR documents. Later correction and calculation have not erased H-0 or made
the subtotal known before it was calculated. Inspect shows a current object
and links, so an old source can expose later incoming proof; that is not a
historical claim. Use explicit temporal admission before relying on that proof.

## Three views of the same evidence

Open once. Start at observed September 1, before correction and cancellation.
Next frame the EUR source documents at Moment and follow the corrected invoice,
copy and void edges. Finally show the full trip through the real calculation
and select CALC-7: included operands lead out, the exclusion audit leads in.
Follow AUDIT-7 to the omitted documents. Read each why and literal evidence;
the picture does not perform the calculation. The viewer has its own shared
revision and does not replace the explicit cursor and filters on memory calls.

```json
{"tool": "kmp_view_open", "save_as": "opened", "arguments": {"about": "example:guide:quantities"}}
```

```json
{"tool": "kmp_view_apply_intent", "save_as": "early_frame", "arguments": {"idempotency_key": "guide-quantities:view-early:v1", "expected_revision": "${opened.view_revision}", "explanation": "Inspect what was known before the correction and void", "focus": {"time_range": {"axis": "observed", "from": "2026-09-01T00:00:00Z", "to": "2026-09-02T00:00:00Z"}}, "projection": {"semantic_zoom": "moment", "dimensions": ["trip"], "labels": [{"key": "trip", "op": "in", "values": ["TRIP-7"]}]}, "selection": null}}
```

```json
{"tool": "kmp_view_get_state", "save_as": "early_state", "arguments": {}}
```

```json
{"tool": "kmp_view_apply_intent", "save_as": "sources_frame", "arguments": {"idempotency_key": "guide-quantities:view-sources:v1", "expected_revision": "${early_state.view_revision}", "explanation": "Follow the corrected invoice, duplicate and void evidence", "focus": {"time_range": {"axis": "observed", "from": "2026-09-01T00:00:00Z", "to": "${clock.until}"}}, "projection": {"semantic_zoom": "moment", "dimensions": ["trip"], "labels": [{"key": "trip", "op": "in", "values": ["TRIP-7"]}, {"key": "currency", "op": "in", "values": ["EUR"]}, {"key": "role", "op": "in", "values": ["source"]}]}, "selection": "${hotel_final.generated_refs.0}"}}
```

```json
{"tool": "kmp_view_get_state", "save_as": "sources_state", "arguments": {}}
```

```json
{"tool": "kmp_view_apply_intent", "save_as": "settlement_frame", "arguments": {"idempotency_key": "guide-quantities:view-settlement:v1", "expected_revision": "${sources_state.view_revision}", "explanation": "Inspect the EUR subtotal, its included operands and exclusion audit", "focus": {"time_range": {"axis": "observed", "from": "2026-09-01T00:00:00Z", "to": "${clock.until}"}}, "projection": {"semantic_zoom": "moment", "dimensions": ["trip"], "labels": [{"key": "trip", "op": "in", "values": ["TRIP-7"]}]}, "selection": "${total.generated_refs.0}"}}
```

```json
{"tool": "kmp_view_get_state", "save_as": "settlement_state", "arguments": {}}
```

```json
{"tool": "kmp_view_get_state", "save_as": "view_state", "arguments": {}}
```

This lesson validates an authored calculation and its graph. It does not
prove an LLM will find every operand, identify unseen duplicates or select
valid rates. Negative boundaries are explicit: early evidence is incomplete,
an equal amount is not identity, USD cannot enter an EUR sum without a rate,
and KMP's write acceptance is not an arithmetic verifier. Ask may later retrieve
the stored subtotal; it neither replaces this navigation nor creates the sum.
