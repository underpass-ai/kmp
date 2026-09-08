# Worked example: labels, synonyms and negation

The fictional LEX-7 catalogue teaches three separate operations: recover a
Spanish source through its English search summary, catalogue synonyms only
when a glossary supports them, and compare opposite states at matching
conditions. Labels help select candidates; typed relations preserve the
writer's semantic claim and its proof.

Use `example:guide:labels-negation` in an isolated store. Read the installed
guide/AGENT.md once and follow its exact reference to this lesson, the live
tool schemas, scope and relation guidance, and the language/summary topic.
The LLM writer interprets the sources. The authored replay checks native
behavior and does not measure a new LLM's learning.

## Sources in receipt order

Only use each source after its observed time in this teaching sequence.
No source resolves the RUN-7 production disagreement. The later maintenance
decision concerns RUN-8, and is a decision rather than an observed outcome.
These are reports of particular checks; do not invent validity intervals
for their states. Ingestion remains the real run time.

| Source | Occurred (UTC) | Observed (UTC) | Literal source |
| --- | --- | --- | --- |

| S1 | 2026-09-01T08:00:00Z | 2026-09-01T08:10:00Z | S1: En RUN-7, el componente bitácora estaba habilitado en prod a las 08:00 UTC del 1 de septiembre. Responsable: Ana. |

| G1 | 2026-09-01T09:00:00Z | 2026-09-01T09:00:00Z | G1: Norma del catálogo LEX-7: bitácora y journal nombran el mismo componente de registro; catalogarlo como journal. Conservar entorno y responsable. Habilitado y deshabilitado son estados incompatibles sólo para la misma comprobación, instante, entorno y responsable. |

| S2 | 2026-09-01T08:00:00Z | 2026-09-01T09:10:00Z | S2: Journal estaba habilitado en prod durante RUN-7 a las 08:00 UTC del 1 de septiembre, bajo la responsabilidad de Ana. Es otra redacción del informe S1. |

| S3 | 2026-09-01T08:00:00Z | 2026-09-01T09:20:00Z | S3: Durante RUN-7 a las 08:00 UTC del 1 de septiembre, journal no estaba habilitado en prod: estaba deshabilitado. Responsable: Ana. Este informe no aporta una verificación que resuelva la discrepancia con S1. |

| S4 | 2026-09-01T08:00:00Z | 2026-09-01T09:30:00Z | S4: Journal estaba deshabilitado en staging durante RUN-7 a las 08:00 UTC del 1 de septiembre. Responsable: Ana. Este informe sólo describe staging, no prod. |

| D1 | 2026-09-02T08:00:00Z | 2026-09-02T08:00:00Z | D1: Tras revisar los informes de RUN-7, decidimos deshabilitar journal en prod para RUN-8 desde las 08:00 UTC del 2 de septiembre. Responsable: Ana. Esta decisión posterior no determina el estado que tuvo RUN-7 ni sustituye sus informes. |

The four reports are `observation`; G1 is a `constraint` because it
prescribes a catalogue convention; D1 is a `decision`. Do not label all
sources as observations to avoid choosing their meaning. `intent` describes
the writing act and differs from the memory kind and relation class.

JSON envelopes name a tool, its arguments and a local `save_as` binding.
Send only the arguments. `${name.generated_refs.0}` copies a returned ref.
An unexpected error or an incomplete page stops the replay; in larger
histories follow the returned cursor before claiming complete coverage.

## Recover and record the original wording

A fresh about returns `not_found`. On an existing about, recover its
catalogue and use the actual refs instead of pretending the history is empty.

```json
{
  "tool": "kmp_wake",
  "save_as": "initial",
  "expect_error": "not_found",
  "arguments": {
    "about": "example:guide:labels-negation",
    "budget": {"max_bytes": 20000}
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "original",
  "arguments": {
    "about": "example:guide:labels-negation",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-labels-negation:original:v1",
    "scope": {
      "process": "lexicon-review",
      "task": "operation-check"
    },
    "labels": {
      "component": "bitácora",
      "environment": "prod",
      "owner": "ana",
      "run": "RUN-7",
      "state": "enabled"
    },
    "occurred_at": "2026-09-01T08:00:00Z",
    "observed_at": "2026-09-01T08:10:00Z",
    "current": {
      "kind": "observation",
      "summary": "S1: En RUN-7, el componente bitácora estaba habilitado en prod a las 08:00 UTC del 1 de septiembre. Responsable: Ana.",
      "summary_en": "S1 reports enabled logging: component bitácora was enabled in prod under Ana during RUN-7 at 08:00 UTC on September 1.",
      "evidence": "S1, received 2026-09-01T08:10:00Z: S1: En RUN-7, el componente bitácora estaba habilitado en prod a las 08:00 UTC del 1 de septiembre. Responsable: Ana."
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "original_before",
  "arguments": {
    "about": "example:guide:labels-negation",
    "ref": "${original.generated_refs.0}",
    "budget": {"max_bytes": 30000},
    "include": {"raw": true}
  }
}
```

At this point the catalogue contains `component=bitácora`. The writer
does not yet have G1's claim that it names the same component as journal.
The English summary preserves S1, RUN-7, Ana, prod and its clock while
making the Spanish statement searchable. It is search text, never a new
source or permission to drop the original negation.

This semantic read deliberately uses English words supplied by summary_en.
Inspect its cited source and `matched_via`/`summary_terms`; a result must
still carry S1's Spanish text. It does not require a lexical bridge.

```json
{
  "tool": "kmp_ask",
  "save_as": "english_summary",
  "arguments": {
    "about": "example:guide:labels-negation",
    "question": "Which logging component was enabled?",
    "asked_as": "¿Qué componente de registro estaba habilitado?",
    "budget": {"max_bytes": 25000, "detail": "full"}
  }
}
```

## Read the glossary and relabel with evidence

G1 is a scoped catalogue rule, not a global synonym dictionary. Its context
link says why the original record is relevant; it does not turn the glossary
into another enabled-state report. Read the catalogue after writing G1.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "glossary",
  "arguments": {
    "about": "example:guide:labels-negation",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-labels-negation:glossary:v1",
    "scope": {
      "process": "lexicon-review",
      "task": "catalogue-rule"
    },
    "labels": {
      "component": "journal"
    },
    "occurred_at": "2026-09-01T09:00:00Z",
    "observed_at": "2026-09-01T09:00:00Z",
    "current": {
      "kind": "constraint",
      "summary": "G1: Norma del catálogo LEX-7: bitácora y journal nombran el mismo componente de registro; catalogarlo como journal. Conservar entorno y responsable. Habilitado y deshabilitado son estados incompatibles sólo para la misma comprobación, instante, entorno y responsable.",
      "summary_en": "G1 is the LEX-7 catalogue rule: bitácora and journal name one logging component, catalogued as journal. Retain environment and owner. Enabled and disabled conflict only for the same check, instant, environment and owner.",
      "evidence": "G1, received 2026-09-01T09:00:00Z: G1: Norma del catálogo LEX-7: bitácora y journal nombran el mismo componente de registro; catalogarlo como journal. Conservar entorno y responsable. Habilitado y deshabilitado son estados incompatibles sólo para la misma comprobación, instante, entorno y responsable."
    },
    "read_context": {
      "inspected_refs": [
        "${original.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${original.generated_refs.0}",
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "La norma G1 explica cómo catalogar el término bitácora usado por S1 dentro de LEX-7; no verifica el estado habilitado que S1 afirma.",
        "evidence": "G1: Norma del catálogo LEX-7: bitácora y journal nombran el mismo componente de registro; catalogarlo como journal. Conservar entorno y responsable. Habilitado y deshabilitado son estados incompatibles sólo para la misma comprobación, instante, entorno y responsable."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "glossary_read",
  "arguments": {
    "about": "example:guide:labels-negation",
    "ref": "${glossary.generated_refs.0}",
    "budget": {"max_bytes": 30000},
    "include": {"raw": false}
  }
}
```

```json
{
  "tool": "kmp_wake",
  "save_as": "catalogue_before_relabel",
  "arguments": {
    "about": "example:guide:labels-negation",
    "budget": {"max_bytes": 25000, "detail": "compact"}
  }
}
```

```json
{
  "tool": "kmp_relabel",
  "save_as": "canonical_label",
  "arguments": {
    "about": "example:guide:labels-negation",
    "ref": "${original.generated_refs.0}",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-labels-negation:canonical-component:v1",
    "observed_at": "2026-09-01T09:01:00Z",
    "remove": {"component": "bitácora"},
    "add": {"component": "journal"},
    "why": "G1 identifica bitácora y journal como el mismo componente de LEX-7 y prescribe journal para el catálogo; se conserva el texto, la prueba y los relojes de S1."
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "original_read",
  "arguments": {
    "about": "example:guide:labels-negation",
    "ref": "${original.generated_refs.0}",
    "budget": {"max_bytes": 30000},
    "include": {"raw": true}
  }
}
```

Verify that S1's object and source did not change. The new component
coordinate inherits S1's occurrence and receipt clocks; the relabel edge
records September 1 at 09:01 as the catalogue change. This current catalogue
can find older evidence; it does not prove that G1 was already known at
08:10. Keep the glossary's own observed time when reasoning historically.

Do not reuse `bitácora` under another label key such as `alias`: that value
already belongs to the component key in this about and strict writes refuse
such a collision. The original word remains searchable in S1 and G1.

## Restate a claim and retain its negation

S2 explicitly restates S1 at the same RUN-7 instant, environment and owner.
G1 supplies the scoped component equivalence. `restates` relates these
claims; sharing journal alone would not justify it.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "restated",
  "arguments": {
    "about": "example:guide:labels-negation",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-labels-negation:restated:v1",
    "scope": {
      "process": "lexicon-review",
      "task": "operation-check"
    },
    "labels": {
      "component": "journal",
      "environment": "prod",
      "owner": "ana",
      "run": "RUN-7",
      "state": "enabled"
    },
    "occurred_at": "2026-09-01T08:00:00Z",
    "observed_at": "2026-09-01T09:10:00Z",
    "current": {
      "kind": "observation",
      "summary": "S2: Journal estaba habilitado en prod durante RUN-7 a las 08:00 UTC del 1 de septiembre, bajo la responsabilidad de Ana. Es otra redacción del informe S1.",
      "summary_en": "S2 restates S1: journal logging was enabled in prod for RUN-7 at 08:00 UTC on September 1, with Ana responsible.",
      "evidence": "S2, received 2026-09-01T09:10:00Z: S2: Journal estaba habilitado en prod durante RUN-7 a las 08:00 UTC del 1 de septiembre, bajo la responsabilidad de Ana. Es otra redacción del informe S1."
    },
    "read_context": {
      "inspected_refs": [
        "${original.generated_refs.0}",
        "${glossary.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${original.generated_refs.0}",
        "rel": "restates",
        "class": "evidential",
        "confidence": "high",
        "why": "S2 declara que reformula S1: ambos afirman habilitado para RUN-7, prod, Ana y el mismo instante; G1 identifica los dos nombres del componente.",
        "evidence": "S2: Journal estaba habilitado en prod durante RUN-7 a las 08:00 UTC del 1 de septiembre, bajo la responsabilidad de Ana. Es otra redacción del informe S1. G1: Norma del catálogo LEX-7: bitácora y journal nombran el mismo componente de registro; catalogarlo como journal. Conservar entorno y responsable. Habilitado y deshabilitado son estados incompatibles sólo para la misma comprobación, instante, entorno y responsable."
      },
      {
        "ref": "${glossary.generated_refs.0}",
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "La equivalencia de términos de G1 permite catalogar y comparar S2 con el componente nombrado en S1, sin sustituir la prueba del estado.",
        "evidence": "G1: Norma del catálogo LEX-7: bitácora y journal nombran el mismo componente de registro; catalogarlo como journal. Conservar entorno y responsable. Habilitado y deshabilitado son estados incompatibles sólo para la misma comprobación, instante, entorno y responsable."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "restated_read",
  "arguments": {
    "about": "example:guide:labels-negation",
    "ref": "${restated.generated_refs.0}",
    "budget": {"max_bytes": 30000},
    "include": {"raw": false}
  }
}
```

S3 says **not enabled**, then explicitly says disabled. Retain both
that negation and the absence of resolving verification. Its disagreement
with S1 is supported by matching check, instant, component, environment and
owner. Later receipt alone does not supersede S1 or S2.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "negative",
  "arguments": {
    "about": "example:guide:labels-negation",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-labels-negation:negative:v1",
    "scope": {
      "process": "lexicon-review",
      "task": "operation-check"
    },
    "labels": {
      "component": "journal",
      "environment": "prod",
      "owner": "ana",
      "run": "RUN-7",
      "state": "disabled"
    },
    "occurred_at": "2026-09-01T08:00:00Z",
    "observed_at": "2026-09-01T09:20:00Z",
    "current": {
      "kind": "observation",
      "summary": "S3: Durante RUN-7 a las 08:00 UTC del 1 de septiembre, journal no estaba habilitado en prod: estaba deshabilitado. Responsable: Ana. Este informe no aporta una verificación que resuelva la discrepancia con S1.",
      "summary_en": "S3 says journal was not enabled in prod; it was disabled for RUN-7 at 08:00 UTC on September 1 under Ana. This report supplies no verification that resolves its disagreement with S1.",
      "evidence": "S3, received 2026-09-01T09:20:00Z: S3: Durante RUN-7 a las 08:00 UTC del 1 de septiembre, journal no estaba habilitado en prod: estaba deshabilitado. Responsable: Ana. Este informe no aporta una verificación que resuelva la discrepancia con S1."
    },
    "read_context": {
      "inspected_refs": [
        "${original.generated_refs.0}",
        "${glossary.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${original.generated_refs.0}",
        "rel": "contradicts",
        "class": "evidential",
        "confidence": "high",
        "why": "S1 afirma habilitado y S3 lo niega para el mismo RUN-7, prod, Ana e instante; G1 prueba que bitácora y journal designan el mismo componente en este catálogo.",
        "evidence": "S1: En RUN-7, el componente bitácora estaba habilitado en prod a las 08:00 UTC del 1 de septiembre. Responsable: Ana. S3: Durante RUN-7 a las 08:00 UTC del 1 de septiembre, journal no estaba habilitado en prod: estaba deshabilitado. Responsable: Ana. Este informe no aporta una verificación que resuelva la discrepancia con S1. G1: Norma del catálogo LEX-7: bitácora y journal nombran el mismo componente de registro; catalogarlo como journal. Conservar entorno y responsable. Habilitado y deshabilitado son estados incompatibles sólo para la misma comprobación, instante, entorno y responsable."
      },
      {
        "ref": "${glossary.generated_refs.0}",
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "G1 fija las condiciones y el vocabulario usados al comparar la negación de S3 con S1; el glosario no decide cuál de los informes es correcto.",
        "evidence": "G1: Norma del catálogo LEX-7: bitácora y journal nombran el mismo componente de registro; catalogarlo como journal. Conservar entorno y responsable. Habilitado y deshabilitado son estados incompatibles sólo para la misma comprobación, instante, entorno y responsable."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "negative_read",
  "arguments": {
    "about": "example:guide:labels-negation",
    "ref": "${negative.generated_refs.0}",
    "budget": {"max_bytes": 30000},
    "include": {"raw": false}
  }
}
```

```json
{
  "tool": "kmp_view_open",
  "save_as": "opened",
  "arguments": {
    "about": "example:guide:labels-negation"
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "conflict_frame",
  "arguments": {
    "expected_revision": "${opened.view_revision}",
    "idempotency_key": "guide-labels-negation:view-conflict:v1",
    "explanation": "Review conflict for LEX-7",
    "focus": {"time_range": {"axis": "observed", "from": "2026-09-01T00:00:00Z", "to": "2026-09-02T00:00:00Z"}},
    "projection": {"semantic_zoom": "moment", "dimensions": ["component"], "labels": [{"key": "component", "op": "in", "values": ["journal"]}, {"key": "environment", "op": "in", "values": ["prod"]}, {"key": "owner", "op": "in", "values": ["ana"]}, {"key": "run", "op": "in", "values": ["RUN-7"]}], "relation_classes": ["evidential", "procedural"]},
    "selection": "${negative.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "conflict_state",
  "arguments": {
  }
}
```

## Opposite words under different conditions

S4 is about staging. It may share the component, owner, check and state label
with another report, but it does not contradict an enabled report in prod.
The background relation preserves the comparison and names its boundary.
D1 is a later decision for RUN-8; it does not certify an execution or settle
the historical RUN-7 disagreement. `follows` records only order.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "other_environment",
  "arguments": {
    "about": "example:guide:labels-negation",
    "intent": "record_observation",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-labels-negation:other_environment:v1",
    "scope": {
      "process": "lexicon-review",
      "task": "operation-check"
    },
    "labels": {
      "component": "journal",
      "environment": "staging",
      "owner": "ana",
      "run": "RUN-7",
      "state": "disabled"
    },
    "occurred_at": "2026-09-01T08:00:00Z",
    "observed_at": "2026-09-01T09:30:00Z",
    "current": {
      "kind": "observation",
      "summary": "S4: Journal estaba deshabilitado en staging durante RUN-7 a las 08:00 UTC del 1 de septiembre. Responsable: Ana. Este informe sólo describe staging, no prod.",
      "summary_en": "S4 records disabled journal logging in staging for RUN-7 at 08:00 UTC on September 1 under Ana. Its scope is staging, not prod.",
      "evidence": "S4, received 2026-09-01T09:30:00Z: S4: Journal estaba deshabilitado en staging durante RUN-7 a las 08:00 UTC del 1 de septiembre. Responsable: Ana. Este informe sólo describe staging, no prod."
    },
    "read_context": {
      "inspected_refs": [
        "${negative.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${negative.generated_refs.0}",
        "rel": "uses_background",
        "class": "evidential",
        "confidence": "high",
        "why": "S4 permite comparar la comprobación de staging con la de prod descrita por S3; los entornos diferentes impiden tratarlas como el mismo hecho.",
        "evidence": "S4: Journal estaba deshabilitado en staging durante RUN-7 a las 08:00 UTC del 1 de septiembre. Responsable: Ana. Este informe sólo describe staging, no prod."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "other_environment_read",
  "arguments": {
    "about": "example:guide:labels-negation",
    "ref": "${other_environment.generated_refs.0}",
    "budget": {"max_bytes": 30000},
    "include": {"raw": false}
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "later_decision",
  "arguments": {
    "about": "example:guide:labels-negation",
    "intent": "record_decision",
    "actor": "guide-writer",
    "source_kind": "human",
    "idempotency_key": "guide-labels-negation:later_decision:v1",
    "scope": {
      "process": "lexicon-review",
      "task": "maintenance-decision"
    },
    "labels": {
      "component": "journal",
      "environment": "prod",
      "owner": "ana",
      "run": "RUN-8",
      "state": "disabled"
    },
    "occurred_at": "2026-09-02T08:00:00Z",
    "observed_at": "2026-09-02T08:00:00Z",
    "current": {
      "kind": "decision",
      "summary": "D1: Tras revisar los informes de RUN-7, decidimos deshabilitar journal en prod para RUN-8 desde las 08:00 UTC del 2 de septiembre. Responsable: Ana. Esta decisión posterior no determina el estado que tuvo RUN-7 ni sustituye sus informes.",
      "summary_en": "D1 follows review of RUN-7 reports: disable journal in prod for RUN-8 from 08:00 UTC on September 2, with Ana responsible. This later decision does not determine the earlier RUN-7 state or replace its reports.",
      "evidence": "D1, received 2026-09-02T08:00:00Z: D1: Tras revisar los informes de RUN-7, decidimos deshabilitar journal en prod para RUN-8 desde las 08:00 UTC del 2 de septiembre. Responsable: Ana. Esta decisión posterior no determina el estado que tuvo RUN-7 ni sustituye sus informes."
    },
    "read_context": {
      "inspected_refs": [
        "${other_environment.generated_refs.0}"
      ]
    },
    "connect_to": [
      {
        "ref": "${other_environment.generated_refs.0}",
        "rel": "follows",
        "class": "procedural",
        "confidence": "high",
        "why": "La decisión D1 se registra después de revisar los informes de RUN-7; este enlace conserva el orden y no verifica ni sustituye sus estados anteriores.",
        "evidence": "D1: Tras revisar los informes de RUN-7, decidimos deshabilitar journal en prod para RUN-8 desde las 08:00 UTC del 2 de septiembre. Responsable: Ana. Esta decisión posterior no determina el estado que tuvo RUN-7 ni sustituye sus informes."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "later_decision_read",
  "arguments": {
    "about": "example:guide:labels-negation",
    "ref": "${later_decision.generated_refs.0}",
    "budget": {"max_bytes": 30000},
    "include": {"raw": false}
  }
}
```

```json
{
  "tool": "kmp_wake",
  "save_as": "catalogue",
  "arguments": {
    "about": "example:guide:labels-negation",
    "budget": {"max_bytes": 25000, "detail": "compact"}
  }
}
```

## Navigate the catalogue, then match all conditions

Read actual label values from wake before selecting them. A component-only
traversal returns six memories of three kinds, not six interchangeable
claims. Return only component coordinates while selectors still test whole
entries' environment, owner and run labels. This is one label's time order.
The start below precedes all source events, so there is no event on its
excluded boundary. To start exactly on an event, capture it with goto first.

The matched RUN-7 production slice contains S1, S2 and S3, with both enabled
and disabled labels. Filtering to enabled would conceal the disagreement.
The staging slice contains S4. The later RUN-8 slice contains D1.

```json
{
  "tool": "kmp_forward",
  "save_as": "component_history",
  "arguments": {
    "about": "example:guide:labels-negation",
    "from": {"time": "2026-09-01T00:00:00Z"},
    "axis": "occurred",
    "dimensions": {"mode": "only", "include": ["component"], "selectors": [{"key": "component", "op": "in", "values": ["journal"]}]},
    "budget": {"max_bytes": 50000},
    "limit": {"entries": 30}
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "same_conditions",
  "arguments": {
    "about": "example:guide:labels-negation",
    "from": {"time": "2026-09-01T00:00:00Z"},
    "axis": "occurred",
    "dimensions": {"mode": "only", "include": ["component"], "selectors": [{"key": "component", "op": "in", "values": ["journal"]}, {"key": "environment", "op": "in", "values": ["prod"]}, {"key": "owner", "op": "in", "values": ["ana"]}, {"key": "run", "op": "in", "values": ["RUN-7"]}]},
    "budget": {"max_bytes": 50000},
    "limit": {"entries": 30}
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "staging_only",
  "arguments": {
    "about": "example:guide:labels-negation",
    "from": {"time": "2026-09-01T00:00:00Z"},
    "axis": "occurred",
    "dimensions": {"mode": "only", "include": ["component"], "selectors": [{"key": "component", "op": "in", "values": ["journal"]}, {"key": "environment", "op": "in", "values": ["staging"]}, {"key": "owner", "op": "in", "values": ["ana"]}, {"key": "run", "op": "in", "values": ["RUN-7"]}]},
    "budget": {"max_bytes": 50000},
    "limit": {"entries": 30}
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "later_only",
  "arguments": {
    "about": "example:guide:labels-negation",
    "from": {"time": "2026-09-01T00:00:00Z"},
    "axis": "occurred",
    "dimensions": {"mode": "only", "include": ["component"], "selectors": [{"key": "component", "op": "in", "values": ["journal"]}, {"key": "environment", "op": "in", "values": ["prod"]}, {"key": "owner", "op": "in", "values": ["ana"]}, {"key": "run", "op": "in", "values": ["RUN-8"]}]},
    "budget": {"max_bytes": 50000},
    "limit": {"entries": 30}
  }
}
```

`notin` also admits memories without that key. “Not staging” therefore
includes G1, whose catalogue rule has no environment. Combine `exists` with
`notin` if the intended selection requires a recorded environment. Neither
filter establishes a new fact or turns unknown scope into prod.

```json
{
  "tool": "kmp_forward",
  "save_as": "not_staging",
  "arguments": {
    "about": "example:guide:labels-negation",
    "from": {"time": "2026-09-01T00:00:00Z"},
    "axis": "occurred",
    "dimensions": {"mode": "only", "include": ["component"], "selectors": [{"key": "component", "op": "in", "values": ["journal"]}, {"key": "environment", "op": "notin", "values": ["staging"]}]},
    "budget": {"max_bytes": 50000},
    "limit": {"entries": 30}
  }
}
```

```json
{
  "tool": "kmp_forward",
  "save_as": "known_not_staging",
  "arguments": {
    "about": "example:guide:labels-negation",
    "from": {"time": "2026-09-01T00:00:00Z"},
    "axis": "occurred",
    "dimensions": {"mode": "only", "include": ["component"], "selectors": [{"key": "component", "op": "in", "values": ["journal"]}, {"key": "environment", "op": "exists"}, {"key": "environment", "op": "notin", "values": ["staging"]}]},
    "budget": {"max_bytes": 50000},
    "limit": {"entries": 30}
  }
}
```

```json
{
  "tool": "kmp_wake",
  "save_as": "conflict_after_later_decision",
  "arguments": {
    "about": "example:guide:labels-negation",
    "axis": "observed",
    "as_of": {"time": "2026-09-03T00:00:00Z"},
    "dimensions": {"selectors": [{"key": "component", "op": "in", "values": ["journal"]}, {"key": "environment", "op": "in", "values": ["prod"]}, {"key": "owner", "op": "in", "values": ["ana"]}, {"key": "run", "op": "in", "values": ["RUN-7"]}]},
    "budget": {"max_bytes": 60000, "detail": "full"}
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "restatement_path",
  "arguments": {
    "about": "example:guide:labels-negation",
    "from": "${restated.generated_refs.0}",
    "to": "${original.generated_refs.0}",
    "budget": {"max_bytes": 40000}
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "contradiction_path",
  "arguments": {
    "about": "example:guide:labels-negation",
    "from": "${negative.generated_refs.0}",
    "to": "${original.generated_refs.0}",
    "budget": {"max_bytes": 40000}
  }
}
```

Audit the two directed paths and compare their why/evidence with the
sources. The later decision has no `supersedes` edge; the RUN-7 contradiction
remains live. KMP preserves the writer's claims; accepting a write does not
independently verify the truth of either report.

## Review the contexts in ChronoLoom

First retain the production disagreement, then inspect staging separately,
and finally remove the environment filter to see all six memories. Select
G1 and S1 to audit the catalogue rule and preserved source; follow S3's
contradiction and S2's restatement. The panel is Current record: its current
links can extend outside the scene window. Follow a relation to move to its
target's recorded time; inspect the new shared state afterward.

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "staging_frame",
  "arguments": {
    "expected_revision": "${conflict_state.view_revision}",
    "idempotency_key": "guide-labels-negation:view-staging:v1",
    "explanation": "Review staging for LEX-7",
    "focus": {"time_range": {"axis": "observed", "from": "2026-09-01T00:00:00Z", "to": "2026-09-02T00:00:00Z"}},
    "projection": {"semantic_zoom": "moment", "dimensions": ["component"], "labels": [{"key": "component", "op": "in", "values": ["journal"]}, {"key": "environment", "op": "in", "values": ["staging"]}], "relation_classes": ["evidential", "procedural"]},
    "selection": "${other_environment.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "staging_state",
  "arguments": {
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "catalogue_frame",
  "arguments": {
    "expected_revision": "${staging_state.view_revision}",
    "idempotency_key": "guide-labels-negation:view-catalogue:v1",
    "explanation": "Review catalogue for LEX-7",
    "focus": {"time_range": {"axis": "observed", "from": "2026-09-01T00:00:00Z", "to": "2026-09-03T00:00:00Z"}},
    "projection": {"semantic_zoom": "moment", "dimensions": ["component"], "labels": [{"key": "component", "op": "in", "values": ["journal"]}], "relation_classes": ["evidential", "procedural"]},
    "selection": "${glossary.generated_refs.0}"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "catalogue_state",
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

## Negative cases and limits

- Do not equate journal with every object called bitácora outside LEX-7.
  G1 is explicit evidence for one component and one catalogue.
- Do not drop “no” from S3's source or summary_en, or label it enabled simply
  because that word occurs in its sentence.
- Do not create a contradiction from enabled/disabled without matching all
  conditions; S4's staging context and D1's later run are the counterexamples.
- Do not treat a shared owner label as identity proof about a person, or a
  component label as equivalence of entire statements.
- Do not filter away a negative report to manufacture agreement, or infer
  prod from a missing environment label.
- Do not claim LLM learning from this authored replay. The independent
  writer must later operate unseen sources without future questions or gold
  answers, and the reader must justify its route and remaining uncertainty.

Run `python3 scripts/guide_examples/replay.py --lesson labels-negation
--guide-mode markdown --binary <development-binary> --trace <new-jsonl>
--result <new-json>`, optionally adding `--hold-view` for human review.
It invokes no model and stops on incomplete pages or failed evidence checks.
