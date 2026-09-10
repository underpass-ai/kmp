# Worked example: a decision changes, its history remains

Use this lesson when a reader needs to reconstruct a decision and its reasons
at different dates. The LLM interprets the sources, chooses the memory kinds
and justifies each relation. KMP validates and preserves that interpretation;
it does not infer the replacement from database names or dates.

This is a fictional teaching history for `example:guide:decision-history`.
Never mix it into a real project's about. In a writer evaluation, give the LLM
the guide and sources available at each step, without future sources or reader
questions. Replaying these authored calls checks the contract, not whether
another LLM learned the lesson.

## Sources and choices

All timestamps below are explicit UTC. The source was received when it
occurred, and took effect immediately. Ingestion happens when you run the
example; do not backdate that clock.

| Source | Time | Literal source |
| --- | --- | --- |
| S1, product owner | 2026-09-01T09:00:00Z | Para Atlas, la bitácora debe funcionar sin red. El componente es journal y el entorno es field. |
| S2, architecture decision | 2026-09-02T09:00:00Z | Elegimos SQLite para la bitácora de Atlas porque debe funcionar sin red. |
| S3, product owner | 2026-09-05T09:00:00Z | Desde ahora, la bitácora de Atlas debe ser central y compartida. Retiramos el requisito de funcionar sin red. Se mantienen el componente journal y el entorno field. |
| S4, architecture decision | 2026-09-05T09:05:00Z | Para cumplir el nuevo requisito de bitácora central y compartida de Atlas, elegimos PostgreSQL. Esta decisión sustituye la elección anterior de SQLite desde ahora. |

S1 and S3 are **constraints**; S2 and S4 are **decisions**. Declare that meaning
directly in `memories[].kind`.
`chosen_because` has class `motivational`: the source explicitly gives a
reason. `supersedes` has class `evidential`: the source explicitly withdraws
the old requirement or replaces the old decision. Evidence belongs on both
classes. This does not establish that only PostgreSQL could meet S3.

The about, process and task are declared teaching identifiers. `component`
and `environment` labels come from S1 and S3, and are reused throughout this
single component's decision history. Labels organize retrieval; they do not
prove a reason or a replacement.

## Call notation

Each JSON block contains a tool name, its `arguments`, and a local `save_as`
binding. Send only the arguments to that tool. A string such as
`${offline.generated_refs.0}` means copy that exact value from the earlier
response; never send the placeholder, construct a ref, or reuse a ref from
another run. The repository replay resolves these bindings and records the
actual requests and responses. `expect_error` names an intentional negative
result; every other error or incomplete page stops this bounded replay.

Read the brief installed `guide/AGENT.md` and the live tool schemas first. Consult extended verbs on first use and reuse them while in context. The lesson
starts by recovering its own about and label catalogue. On this fresh store,
the about does not exist: the embedded engine returns `not_found`, not an
empty successful wake packet. That expected result permits the first write.
If the about already exists, inspect its memory and labels; do not treat it
as empty to bypass strict relation validation. Other errors need diagnosis.

```json
{"tool":"kmp_wake","save_as":"initial","expect_error":"not_found","arguments":{"about":"example:guide:decision-history","budget":{"max_bytes":20000,"detail":"full"}}}
```

## Write the requirement, then the decision it motivates

The first strict write creates the about, so it has no prior relation. Leave
`memories[].ref` out. S1 does not say when this requirement will end; do not give
it a future `valid_until` learned from S3.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "offline",
  "arguments": {
    "about": "example:guide:decision-history",
    "actor": "guide-writer",
    "idempotency_key": "guide-history:offline:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "environment": [
        "field"
      ],
      "agentic_process": [
        "atlas-planning"
      ],
      "task": [
        "journal-decision"
      ]
    },
    "occurred_at": "2026-09-01T09:00:00Z",
    "observed_at": "2026-09-01T09:00:00Z",
    "valid_from": "2026-09-01T09:00:00Z",
    "source_kind": "human",
    "memories": [
      {
        "id": "current",
        "kind": "constraint",
        "summary": "La bitácora de Atlas debe funcionar sin red.",
        "summary_en": "The Atlas journal must work without a network connection.",
        "evidence": "S1, product owner, 2026-09-01T09:00:00Z: Para Atlas, la bitácora debe funcionar sin red. El componente es journal y el entorno es field."
      }
    ]
  }
}
```

Inspect the returned ref before choosing a relation. Read its complete text,
evidence and coordinates; `read_context` records your actual prior reads,
and is not a substitute for doing them.

```json
{"tool":"kmp_inspect","save_as":"offline_read","arguments":{"about":"example:guide:decision-history","ref":"${offline.generated_refs.0}","budget":{"max_bytes":30000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "sqlite_review",
  "arguments": {
    "about": "example:guide:decision-history",
    "actor": "guide-writer",
    "idempotency_key": "guide-history:sqlite:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "environment": [
        "field"
      ],
      "agentic_process": [
        "atlas-planning"
      ],
      "task": [
        "journal-decision"
      ]
    },
    "occurred_at": "2026-09-02T09:00:00Z",
    "observed_at": "2026-09-02T09:00:00Z",
    "valid_from": "2026-09-02T09:00:00Z",
    "source_kind": "human",
    "read_context": {
      "inspected_refs": [
        "${offline.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "Elegimos SQLite para la bitácora de Atlas porque debe funcionar sin red.",
        "summary_en": "We chose SQLite for the Atlas journal because it must work without a network connection.",
        "evidence": "S2, architecture decision, 2026-09-02T09:00:00Z: Elegimos SQLite para la bitácora de Atlas porque debe funcionar sin red.",
        "connect_to": [
          {
            "ref": "${offline.generated_refs.0}",
            "rel": "chosen_because",
            "class": "motivational",
            "confidence": "high",
            "why": "La decisión identifica el funcionamiento sin red como motivo para elegir SQLite.",
            "evidence": "S2: Elegimos SQLite para la bitácora de Atlas porque debe funcionar sin red."
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
{"tool":"kmp_write_memory","save_as":"sqlite","arguments":"${sqlite_review.next_actions.0.arguments}"}
```

## Record the explicit change without rewriting the past

S3 now arrives. Recover the catalogue before writing; reuse its existing
labels. The new constraint supersedes S1, not the SQLite decision: S4 is the
separate evidence for changing the decision.

```json
{"tool":"kmp_wake","save_as":"before_change","arguments":{"about":"example:guide:decision-history","budget":{"max_bytes":40000,"detail":"full"}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "shared_review",
  "arguments": {
    "about": "example:guide:decision-history",
    "actor": "guide-writer",
    "idempotency_key": "guide-history:shared:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "environment": [
        "field"
      ],
      "agentic_process": [
        "atlas-planning"
      ],
      "task": [
        "journal-decision"
      ]
    },
    "occurred_at": "2026-09-05T09:00:00Z",
    "observed_at": "2026-09-05T09:00:00Z",
    "valid_from": "2026-09-05T09:00:00Z",
    "source_kind": "human",
    "read_context": {
      "inspected_refs": [
        "${offline.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "constraint",
        "summary": "La bitácora de Atlas debe ser central y compartida; se retira el requisito de funcionar sin red.",
        "summary_en": "The Atlas journal must be central and shared; the requirement to work without a network connection is withdrawn.",
        "evidence": "S3, product owner, 2026-09-05T09:00:00Z: Desde ahora, la bitácora de Atlas debe ser central y compartida. Retiramos el requisito de funcionar sin red. Se mantienen el componente journal y el entorno field.",
        "connect_to": [
          {
            "ref": "${offline.generated_refs.0}",
            "rel": "supersedes",
            "class": "evidential",
            "confidence": "high",
            "why": "El responsable retira explícitamente el requisito anterior y establece el nuevo desde este instante.",
            "evidence": "S3: Desde ahora, la bitácora de Atlas debe ser central y compartida. Retiramos el requisito de funcionar sin red."
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
{"tool":"kmp_write_memory","save_as":"shared","arguments":"${shared_review.next_actions.0.arguments}"}
```

```json
{"tool":"kmp_inspect","save_as":"shared_read","arguments":{"about":"example:guide:decision-history","ref":"${shared.generated_refs.0}","budget":{"max_bytes":30000}}}
```

```json
{"tool":"kmp_inspect","save_as":"sqlite_read","arguments":{"about":"example:guide:decision-history","ref":"${sqlite.generated_refs.0}","budget":{"max_bytes":30000}}}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "postgres_review",
  "arguments": {
    "about": "example:guide:decision-history",
    "actor": "guide-writer",
    "idempotency_key": "guide-history:postgres:v1",
    "labels": {
      "component": [
        "journal"
      ],
      "environment": [
        "field"
      ],
      "agentic_process": [
        "atlas-planning"
      ],
      "task": [
        "journal-decision"
      ]
    },
    "occurred_at": "2026-09-05T09:05:00Z",
    "observed_at": "2026-09-05T09:05:00Z",
    "valid_from": "2026-09-05T09:05:00Z",
    "source_kind": "human",
    "read_context": {
      "inspected_refs": [
        "${shared.generated_refs.0}",
        "${sqlite.generated_refs.0}"
      ]
    },
    "memories": [
      {
        "id": "current",
        "kind": "decision",
        "summary": "Elegimos PostgreSQL para la bitácora central y compartida de Atlas; esta decisión sustituye la elección anterior de SQLite.",
        "summary_en": "We chose PostgreSQL for the central shared Atlas journal; this decision replaces the previous SQLite choice.",
        "evidence": "S4, architecture decision, 2026-09-05T09:05:00Z: Para cumplir el nuevo requisito de bitácora central y compartida de Atlas, elegimos PostgreSQL. Esta decisión sustituye la elección anterior de SQLite desde ahora.",
        "connect_to": [
          {
            "ref": "${shared.generated_refs.0}",
            "rel": "chosen_because",
            "class": "motivational",
            "confidence": "high",
            "why": "El nuevo requisito se cita como motivo explícito para esta elección de PostgreSQL.",
            "evidence": "S4: Para cumplir el nuevo requisito de bitácora central y compartida de Atlas, elegimos PostgreSQL."
          },
          {
            "ref": "${sqlite.generated_refs.0}",
            "rel": "supersedes",
            "class": "evidential",
            "confidence": "high",
            "why": "La decisión sustituye explícitamente la elección anterior de SQLite desde este instante; no la borra ni la declara inválida en fechas anteriores.",
            "evidence": "S4: Esta decisión sustituye la elección anterior de SQLite desde ahora."
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
{"tool":"kmp_write_memory","save_as":"postgres","arguments":"${postgres_review.next_actions.0.arguments}"}
```

## Reconstruct the period, then audit the reasons

The requested period is [2026-09-01T09:00:00Z, 2026-09-06T00:00:00Z).
`forward` excludes its starting cursor. Capture the inclusive boundary with
`goto`, retain only entries exactly on it, then traverse later entries. In
this four-entry example the generous limits return complete pages. In larger
histories, follow the returned opaque continuation until complete; merge and
deduplicate refs, and discard entries at or after the end. Do not infer that
an empty page means the complete period is empty.

```json
{"tool":"kmp_goto","save_as":"start","arguments":{"about":"example:guide:decision-history","axis":"occurred","at":{"time":"2026-09-01T09:00:00Z"},"window":{"before_entries":0,"after_entries":0},"include":{"relations":true,"evidence":true},"budget":{"max_bytes":50000}}}
```

```json
{"tool":"kmp_forward","save_as":"later","arguments":{"about":"example:guide:decision-history","axis":"occurred","from":{"time":"2026-09-01T09:00:00Z"},"limit":{"entries":10},"include":{"relations":true,"evidence":true},"budget":{"max_bytes":50000}}}
```

Walk backwards from the new decision. This recovers the new requirement,
older decision and older requirement; it does not roll back the store.

```json
{"tool":"kmp_rewind","save_as":"earlier","arguments":{"about":"example:guide:decision-history","axis":"occurred","from":{"ref":"${postgres.generated_refs.0}"},"limit":{"entries":10},"include":{"relations":true,"evidence":true},"budget":{"max_bytes":50000}}}
```

Audit the new decision and both paths. Read relation direction, class, `why`
and `evidence`: one path establishes replacement, the other motivation.

```json
{"tool":"kmp_inspect","save_as":"postgres_read","arguments":{"about":"example:guide:decision-history","ref":"${postgres.generated_refs.0}","budget":{"max_bytes":30000}}}
```

```json
{"tool":"kmp_trace","save_as":"replacement","arguments":{"about":"example:guide:decision-history","from":"${postgres.generated_refs.0}","to":"${sqlite.generated_refs.0}","budget":{"max_bytes":30000}}}
```

```json
{"tool":"kmp_trace","save_as":"motivation","arguments":{"about":"example:guide:decision-history","from":"${postgres.generated_refs.0}","to":"${shared.generated_refs.0}","budget":{"max_bytes":30000}}}
```

A dated semantic question is another entry point. Compare the same question
at two instants, using the validity clock explicitly. Read canonical evidence
and lifecycle proof rather than accepting the answer wrapper as synthesis.

```json
{"tool":"kmp_ask","save_as":"day3","arguments":{"about":"example:guide:decision-history","question":"Which database was chosen for the Atlas journal?","asked_as":"¿Qué base de datos se eligió para la bitácora de Atlas?","axis":"validity","as_of":{"time":"2026-09-03T12:00:00Z"},"budget":{"max_bytes":50000,"detail":"full"}}}
```

```json
{"tool":"kmp_ask","save_as":"day6","arguments":{"about":"example:guide:decision-history","question":"Which database was chosen for the Atlas journal?","asked_as":"¿Qué base de datos se eligió para la bitácora de Atlas?","axis":"validity","as_of":{"time":"2026-09-06T12:00:00Z"},"budget":{"max_bytes":50000,"detail":"full"}}}
```

Expected conclusion: on day 3, SQLite was chosen because of offline work.
On day 6, PostgreSQL replaces it because of the new shared-journal
requirement. The day-3 proof must not apply the day-5 replacement to the past.
Between 09:00 and 09:05 on day 5 the requirement has changed but S4 has not
yet replaced the decision. Do not claim the new choice was already made.

## Inspect the same history in ChronoLoom

Open once, use its revision, frame the period, and select the new decision.
The graph should expose two memory kinds and the motivational and evidential
classes; structural containment is ordinary background scaffolding. Selecting
one relation class intentionally hides the other, so restore both when
auditing the whole story. The visual state frames proof and never replaces it.

```json
{"tool":"kmp_view_open","save_as":"view","arguments":{"about":"example:guide:decision-history"}}
```

```json
{
  "tool":"kmp_view_apply_intent", "save_as":"framed",
  "arguments":{
    "expected_revision":"${view.view_revision}", "idempotency_key":"guide-history:view:v1",
    "explanation":"Read the decision change beside its old choice and new requirement",
    "focus":{"time_range":{"axis":"occurred","from":"2026-09-01T00:00:00Z","to":"2026-09-06T00:00:00Z"}},
    "projection":{"semantic_zoom":"moment","dimensions":["component","environment"],"relation_classes":["motivational","evidential"]},
    "selection":"${postgres.generated_refs.0}"
  }
}
```

```json
{"tool":"kmp_view_get_state","save_as":"view_state","arguments":{}}
```

If the human moves first and the revision conflicts, get state and rebase the
intent; never retry blind. Open the returned viewer URL and check the actual
memory kinds, relation classes, dates, selection and evidence. `moment` makes
the selected text readable. Move to `episode` or `atlas` to inspect grouping,
then return to `moment` to read individual proof. Moving the view does not
change the scope, clock or interval of a later MCP retrieval call.

## Negative cases and limits

- If S4 says only “we are considering PostgreSQL”, preserve that uncertainty;
  it does not justify a replacement or a committed decision. KMP validates
  the request shape and supplied proof, not the truth of the LLM's reading.
- If S3 arrives late, set `observed_at` to the real receipt time. Comparing
  occurred and observed asks different questions. This lesson deliberately
  aligns these clocks; it is not the late-evidence lesson.
- Removing relation evidence or choosing an unsupported class must fail a
  strict write. Do not turn strict mode off to force an unsupported claim in.
- An exact logical-write retry keeps the same idempotency key and payload.
  Inspect after an uncertain response; a changed payload needs a new logical
  write. Do not reassign an old ref to make a replacement look like an edit.
- No source gives the journal's production latency or cost. Those facts
  remain unknown; labels, a proof path and the database choice do not supply
  them. If a semantic Ask lacks evidence after the allowed selections, stop.

This lesson covers one advanced story, not every tool, kind or relation.
Its source interpretation is reviewable teaching material. To measure writer
learning, use a new history, blind reader questions and independently checked
evidence after the guide is complete.
