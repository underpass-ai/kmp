# Worked example: a person's alias and a reassigned account

Use this lesson to distinguish identity from assignment. Two names may refer
to the same person; an account may instead be assigned to different people at
different times. The LLM reads the source and chooses that meaning. A shared
label or a matching name is not identity evidence.

This fictional registry belongs to `example:guide:alias-ownership`, never to
a real person's memory. Read the brief agent entry and live tool schemas first. Use
the installed guide/AGENT.md, then follow its exact node reference to this
lesson. The Markdown indexes the writer prerequisites; read the relevant
nodes before writing. The example body comes from MCP. The repository replay accepts `--lesson alias-ownership`
and reads Markdown before inspecting the chosen node. It invokes no model and does not measure a new writer's learning.

## Sources, in the order they become available

| Source | Occurred, observed and effective from (UTC) | Literal source |
| --- | --- | --- |
| S1, Atlas directory | 2026-09-01T08:00:00Z | Elena Vega coordina Atlas. |
| S2, Atlas access administrator | 2026-09-01T10:00:00Z | Asignamos la cuenta @oak a Jon para operar Atlas desde ahora. |
| S3, signed directory clarification | 2026-09-02T09:00:00Z | Elena Vega firma también como Nora. Ambos nombres identifican a la misma persona. |
| S4, visitor log | 2026-09-03T11:00:00Z | Visita a Atlas: Nora. No se registraron apellidos ni un identificador de persona. |
| S5, Atlas access administrator | 2026-09-05T10:00:00Z | Asignamos la cuenta @oak a Rui desde ahora. Esta asignación sustituye la asignación anterior a Jon. Jon y Rui son personas distintas. |

S1, S3 and S4 are observations from documents. S2 and S5 are decisions to
assign an account. The S3 memory can use `same_entity_as` toward the S1 memory
because their named subject is explicitly the same person. This does not
make their entire texts interchangeable. S5 uses `supersedes` toward S2:
the **assignment** changes, and neither person becomes the account or the
other person. Both relations are evidential. `uses_background` below is an
honest contextual connection where the sources establish no richer meaning.

Do not assign an end date to S2 before S5 arrives. These sources align their
first three clocks deliberately; ingestion is the real run time. This is not
a late-evidence exercise.

## Call notation and initial recovery

The JSON envelopes name a `tool`, its `arguments` and a local `save_as`
binding. Send only the arguments. `${name.generated_refs.0}` copies an exact
returned value, never a constructed ref. `expect_error` names one intentional
negative result. The replay stops on any other error or incomplete page;
larger histories require the returned pagination before claiming coverage.

```json
{"tool":"kmp_wake","save_as":"initial","expect_error":"not_found","arguments":{"about":"example:guide:alias-ownership","budget":{"max_bytes":20000}}}
```

A fresh missing about returns `not_found` in the embedded engine. If it
already exists, recover it and reuse its catalogue; do not bypass strict
validation by pretending it is empty.

## Record the person and the first assignment

`identity-review` and `access-review` are task identifiers chosen for this
registry. The person label names the directory subject; the account label
names the account, not its owner. First write S1 without a prior relation.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "elena",
  "arguments": {
    "about": "example:guide:alias-ownership",
    "actor": "guide-writer",
    "idempotency_key": "guide-alias:elena:v1",
    "source_kind": "human",
    "labels": {
      "person": [
        "elena-vega"
      ],
      "agentic_process": [
        "atlas-registry"
      ],
      "task": [
        "identity-review"
      ]
    },
    "occurred_at": "2026-09-01T08:00:00Z",
    "observed_at": "2026-09-01T08:00:00Z",
    "valid_from": "2026-09-01T08:00:00Z",
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "Elena Vega coordina Atlas.",
        "summary_en": "Elena Vega coordinates the Atlas project.",
        "evidence": "S1, Atlas directory, 2026-09-01T08:00:00Z: Elena Vega coordina Atlas."
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"elena_before","arguments":{"about":"example:guide:alias-ownership","ref":"${elena.generated_refs.0}","budget":{"max_bytes":30000}}}
```

S2 belongs to the same registry but does not say Elena authorized the
assignment. Preserve the context with `uses_background`; do not invent an
authorization, dependency or identity link.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "jon",
  "arguments": {
    "about": "example:guide:alias-ownership",
    "actor": "guide-writer",
    "idempotency_key": "guide-alias:jon:v1",
    "source_kind": "human",
    "labels": {
      "account": [
        "@oak"
      ],
      "agentic_process": [
        "atlas-registry"
      ],
      "task": [
        "access-review"
      ]
    },
    "occurred_at": "2026-09-01T10:00:00Z",
    "observed_at": "2026-09-01T10:00:00Z",
    "valid_from": "2026-09-01T10:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${elena.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "Asignamos la cuenta @oak a Jon para operar Atlas.",
        "summary_en": "We assigned the @oak account to Jon to operate Atlas.",
        "evidence": "S2, Atlas access administrator, 2026-09-01T10:00:00Z: Asignamos la cuenta @oak a Jon para operar Atlas desde ahora.",
        "connect_to": [
          {
            "ref": "${elena.generated_refs.0}",
            "rel": "uses_background",
            "class": "evidential",
            "confidence": "high",
            "why": "La asignación y la nota de directorio pertenecen al contexto de Atlas; no hay prueba de que Elena haya autorizado esta asignación.",
            "evidence": "S1: Elena Vega coordina Atlas. S2: Asignamos la cuenta @oak a Jon para operar Atlas desde ahora."
          }
        ]
      }
    ]
  }
}
```

## Declare the supported alias, then catalogue it

S3 supplies explicit identity evidence. The new memory's subject is Elena
under the alias Nora; its target's subject is Elena under her directory name.
The target was inspected. Both refs belong to this about, so this case does
not require a cross-about proposal. Across abouts the writer needs the
`kmp_relate` proposal and the additional identity preconditions.

```json
{"tool":"kmp_wake","save_as":"catalogue_before_alias","arguments":{"about":"example:guide:alias-ownership","budget":{"max_bytes":40000,"detail":"full"}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "alias",
  "arguments": {
    "about": "example:guide:alias-ownership",
    "actor": "guide-writer",
    "idempotency_key": "guide-alias:nora:v1",
    "source_kind": "human",
    "labels": {
      "person": [
        "elena-vega"
      ],
      "alias": [
        "Nora"
      ],
      "agentic_process": [
        "atlas-registry"
      ],
      "task": [
        "identity-review"
      ]
    },
    "occurred_at": "2026-09-02T09:00:00Z",
    "observed_at": "2026-09-02T09:00:00Z",
    "valid_from": "2026-09-02T09:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${elena.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "Elena Vega firma también como Nora; ambos nombres identifican a la misma persona.",
        "summary_en": "Elena Vega also signs as Nora; both names identify the same person.",
        "evidence": "S3, signed directory clarification, 2026-09-02T09:00:00Z: Elena Vega firma también como Nora. Ambos nombres identifican a la misma persona.",
        "connect_to": [
          {
            "ref": "${elena.generated_refs.0}",
            "rel": "same_entity_as",
            "class": "evidential",
            "confidence": "high",
            "why": "La aclaración firmada identifica expresamente al sujeto llamado Nora con Elena Vega, la persona nombrada en el directorio.",
            "evidence": "S3: Elena Vega firma también como Nora. Ambos nombres identifican a la misma persona."
          }
        ]
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"alias_read","arguments":{"about":"example:guide:alias-ownership","ref":"${alias.generated_refs.0}","budget":{"max_bytes":30000}}}
```

The relation carries the proof. Relabelling the older directory memory makes
the confirmed alias convenient to filter; it neither creates that identity
proof nor rewrites the source. Its change timestamp records when the label
was applied, not a new occurrence time for the old memory.

```json
{
  "tool":"kmp_relabel", "save_as":"alias_label",
  "arguments":{
    "about":"example:guide:alias-ownership", "ref":"${elena.generated_refs.0}", "actor":"guide-writer",
    "idempotency_key":"guide-alias:catalogue-nora:v1", "observed_at":"2026-09-02T09:00:00Z", "source_kind":"human",
    "add":{"alias": ["Nora"]},
    "why":"S3 identifica expresamente a Elena Vega con Nora; añadimos la etiqueta ya usada en la memoria de esa aclaración, sin cambiar el texto ni la fecha original del directorio."
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"elena_read","arguments":{"about":"example:guide:alias-ownership","ref":"${elena.generated_refs.0}","budget":{"max_bytes":30000}}}
```

## Keep a same-name mention unresolved

S4 names Nora but supplies no identifying link. Record the mention, not an
assertion that Elena visited. Reusing `alias=Nora` is appropriate for finding
that name; it does not authorize `same_entity_as` or `person=elena-vega`.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "visitor",
  "arguments": {
    "about": "example:guide:alias-ownership",
    "actor": "guide-writer",
    "idempotency_key": "guide-alias:unresolved-visitor:v1",
    "source_kind": "human",
    "labels": {
      "alias": [
        "Nora"
      ],
      "agentic_process": [
        "atlas-registry"
      ],
      "task": [
        "identity-review"
      ]
    },
    "occurred_at": "2026-09-03T11:00:00Z",
    "observed_at": "2026-09-03T11:00:00Z",
    "valid_from": "2026-09-03T11:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${alias.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "El registro de visitas a Atlas menciona Nora sin apellidos ni identificador de persona; no permite confirmar su identidad.",
        "summary_en": "The Atlas visitor log mentions Nora without surnames or a person identifier; it does not establish the visitor's identity.",
        "evidence": "S4, visitor log, 2026-09-03T11:00:00Z: Visita a Atlas: Nora. No se registraron apellidos ni un identificador de persona.",
        "connect_to": [
          {
            "ref": "${alias.generated_refs.0}",
            "rel": "uses_background",
            "class": "evidential",
            "confidence": "high",
            "why": "La aclaración permite comparar el nombre Nora; la mención de visitante carece de los datos necesarios para vincularla a esa persona.",
            "evidence": "S4: Visita a Atlas: Nora. No se registraron apellidos ni un identificador de persona."
          }
        ]
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"visitor_read","arguments":{"about":"example:guide:alias-ownership","ref":"${visitor.generated_refs.0}","budget":{"max_bytes":30000}}}
```

## Replace an assignment, not a person

```json
{"tool":"kmp_inspect","save_as":"jon_read","arguments":{"about":"example:guide:alias-ownership","ref":"${jon.generated_refs.0}","budget":{"max_bytes":30000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "rui",
  "arguments": {
    "about": "example:guide:alias-ownership",
    "actor": "guide-writer",
    "idempotency_key": "guide-alias:rui:v1",
    "source_kind": "human",
    "labels": {
      "account": [
        "@oak"
      ],
      "agentic_process": [
        "atlas-registry"
      ],
      "task": [
        "access-review"
      ]
    },
    "occurred_at": "2026-09-05T10:00:00Z",
    "observed_at": "2026-09-05T10:00:00Z",
    "valid_from": "2026-09-05T10:00:00Z",
    "read_context": {
      "inspected_refs": [
        "${jon.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "Asignamos la cuenta @oak a Rui; esta asignación sustituye la anterior a Jon. Jon y Rui son personas distintas.",
        "summary_en": "We assigned the @oak account to Rui; this assignment replaces the previous assignment to Jon. Jon and Rui are distinct people.",
        "evidence": "S5, Atlas access administrator, 2026-09-05T10:00:00Z: Asignamos la cuenta @oak a Rui desde ahora. Esta asignación sustituye la asignación anterior a Jon. Jon y Rui son personas distintas.",
        "connect_to": [
          {
            "ref": "${jon.generated_refs.0}",
            "rel": "supersedes",
            "class": "evidential",
            "confidence": "high",
            "why": "La resolución cambia el titular de la misma cuenta desde este instante y sustituye la asignación anterior; distingue expresamente a ambas personas.",
            "evidence": "S5: Esta asignación sustituye la asignación anterior a Jon. Jon y Rui son personas distintas."
          }
        ]
      }
    ]
  }
}
```

```json
{"tool":"kmp_inspect","save_as":"rui_read","arguments":{"about":"example:guide:alias-ownership","ref":"${rui.generated_refs.0}","budget":{"max_bytes":30000}}}
```

## Recover the names and walk the account history

Read the label catalogue before filtering. The `alias=Nora` slice includes
the confirmed directory subject, the clarification, and the unresolved
visitor. Goto at the clarification's date only includes the first two: the
visitor occurs later. Near reads both sides and recovers all three in this
small history. A filter selects memories, never resolves their identities. Inspect
their source and relation types before making an identity claim.

```json
{"tool":"kmp_wake","save_as":"catalogue","arguments":{"about":"example:guide:alias-ownership","budget":{"max_bytes":60000,"detail":"full"}}}
```

```json
{"tool":"kmp_goto","save_as":"name_at_alias","arguments":{"about":"example:guide:alias-ownership","at":{"ref":"${alias.generated_refs.0}"},"axis":"occurred","dimensions":{"selectors":[{"key":"alias","op":"in","values":["Nora"]}]},"window":{"before_entries":5,"after_entries":0},"include":{"relations":true,"evidence":true},"budget":{"max_bytes":60000}}}
```

```json
{"tool":"kmp_near","save_as":"name_slice","arguments":{"about":"example:guide:alias-ownership","around":{"ref":"${alias.generated_refs.0}"},"axis":"occurred","dimensions":{"selectors":[{"key":"alias","op":"in","values":["Nora"]}]},"window":{"before_entries":5,"after_entries":5},"include":{"relations":true,"evidence":true},"budget":{"max_bytes":60000}}}
```

```json
{"tool":"kmp_trace","save_as":"identity_path","arguments":{"about":"example:guide:alias-ownership","from":"${alias.generated_refs.0}","to":"${elena.generated_refs.0}","budget":{"max_bytes":30000}}}
```

Follow the account's timeline with `near`, using the catalogue's exact
account value. The two assignment memories should remain distinct. Trace
from the new one to the old one to see the explicit replacement proof.

```json
{"tool":"kmp_near","save_as":"account_history","arguments":{"about":"example:guide:alias-ownership","around":{"ref":"${rui.generated_refs.0}"},"axis":"occurred","dimensions":{"selectors":[{"key":"account","op":"in","values":["@oak"]}]},"window":{"before_entries":5,"after_entries":5},"include":{"relations":true,"evidence":true},"budget":{"max_bytes":50000}}}
```

```json
{"tool":"kmp_trace","save_as":"assignment_path","arguments":{"about":"example:guide:alias-ownership","from":"${rui.generated_refs.0}","to":"${jon.generated_refs.0}","budget":{"max_bytes":30000}}}
```

A dated question uses the validity clock and the account label explicitly.
The expected assignment is Jon on September 3 and Rui on September 6. The
old assignment remains historical evidence; it is not an identity relation
between Jon and Rui. These calls retrieve evidence, not generated answers.

```json
{"tool":"kmp_ask","save_as":"day3","arguments":{"about":"example:guide:alias-ownership","question":"Who was assigned the @oak account?","asked_as":"¿A quién se asignó la cuenta @oak?","axis":"validity","as_of":{"time":"2026-09-03T12:00:00Z"},"dimensions":{"selectors":[{"key":"account","op":"in","values":["@oak"]}]},"budget":{"max_bytes":40000,"detail":"full"}}}
```

```json
{"tool":"kmp_ask","save_as":"day6","arguments":{"about":"example:guide:alias-ownership","question":"Who was assigned the @oak account?","asked_as":"¿A quién se asignó la cuenta @oak?","axis":"validity","as_of":{"time":"2026-09-06T12:00:00Z"},"dimensions":{"selectors":[{"key":"account","op":"in","values":["@oak"]}]},"budget":{"max_bytes":40000,"detail":"full"}}}
```

A later catalogue label can find an earlier memory; it does not prove the
alias was already known on that earlier date. S3 was observed on September 2.
The dated question below therefore has no identity evidence on September 1,
even after the label was added. Expect `UNKNOWN` and stop this question there.

```json
{"tool":"kmp_ask","save_as":"before_alias_known","arguments":{"about":"example:guide:alias-ownership","question":"Who signs as Nora?","asked_as":"¿Quién firma como Nora?","axis":"observed","as_of":{"time":"2026-09-01T12:00:00Z"},"budget":{"max_bytes":30000,"detail":"full"}}}
```

## Check the distinction in ChronoLoom

```json
{"tool":"kmp_view_open","save_as":"view","arguments":{"about":"example:guide:alias-ownership"}}
```

```json
{"tool":"kmp_view_apply_intent","save_as":"framed","arguments":{"expected_revision":"${view.view_revision}","idempotency_key":"guide-alias:view:v1","explanation":"Compare the evidenced person alias with the separate account assignment history","focus":{"time_range":{"axis":"occurred","from":"2026-09-01T00:00:00Z","to":"2026-09-06T00:00:00Z"}},"projection":{"semantic_zoom":"moment","dimensions":["person","alias","account"],"relation_classes":["evidential"]},"selection":"${alias.generated_refs.0}"}}
```

```json
{"tool":"kmp_view_get_state","save_as":"view_state","arguments":{}}
```

Inspect the selected alias, then the unresolved visitor and the two account
assignments. Expect three observations and two decisions, one same-entity
link, one replacement and two background links. The sources justify these
types; do not add other classes just to diversify the picture. Check the
person, alias and account labels, relation direction, literal evidence and
old assignment's historical status. Re-read shared view state after a human
move, and rebase any later intent on that revision.

## Negative cases and limits

- Nora in the visitor log might be Elena or somebody else. Do not choose one
  without evidence, silently merge the memories, or copy the known person
  label to the visitor. The replay asserts no identity link or person label
  was authored for that mention; it does not prove a new LLM will abstain.
- Neither `alias=Nora` nor `account=@oak` establishes identity. The account
  is a resource assigned to a person. S5 explicitly says Jon and Rui differ.
- A nonempty fabricated `why` and `evidence` can satisfy structural checks.
  KMP does not judge their truth; the LLM must justify them from the source.
- Relabel is catalogue maintenance. It is not a temporal ownership change,
  an authorization to edit source text, or a replacement for `supersedes`.
- If S5 only proposed Rui as a possible owner, do not record a completed
  reassignment. If the effective or observed time is unknown, preserve that
  uncertainty; do not borrow this lesson's dates.

For cross-about proposals and following an event through multiple projects,
consult the distributed-incident lesson from the guide index. This same-about
case does not test those preconditions.
