# Kernel Write Protocol Plan

Date: 2026-05-06

Status: first MCP writer helper implemented; P1 hardening remains active.

Implementation checkpoint, 2026-05-09:

- `kmp_write_memory` is exposed by the stdio MCP server.
- The helper plans writer-friendly memory, validates relation vocabulary and
  relation quality, returns a canonical `kmp_ingest` preview in dry-run, and
  commits by forwarding the generated ingest payload through the same backend
  path as `kmp_ingest`.
- Strict mode rejects vague relations, missing evidence/why, missing
  `scope.process`, missing `current.evidence`, self-links, and rich external
  relations without prior read context.
- Relation quality diagnostics and metrics are returned in dry-run and commit
  responses.
- This is still an MCP/helper surface above KMP. The canonical write contract
  remains `KernelMemoryService.Ingest` / `kmp_ingest`.

## Problem

`kmp_ingest` is the canonical low-level write operation for Kernel Memory
Protocol. It is correct for adapters and machines, but it is too low-level as
the primary writing surface for LLMs and humans.

Today a writer must provide all of this at once:

- stable ids;
- dimensions;
- temporal coordinates;
- entries;
- relation classes;
- relation `why` and `evidence`;
- evidence refs;
- provenance;
- idempotency keys.

That shape is too much cognitive load for a general LLM. It also pushes the
hardest part of kernel usage into every client: deciding what semantic relation
connects a new node to previous memory.

The kernel is designed to store, validate, traverse, trace, inspect, and render
semantic memory. It should not invent semantic relations itself. The missing
piece is a writer-first protocol above `kmp_ingest` that lets an LLM or
human express what changed, why it matters, and what prior refs it depends on.

## Product Decision

Keep `kmp_ingest` as the canonical low-level contract.

Add a higher-level Kernel Write Protocol for LLM and human writers:

```text
LLM / human
  -> kmp_write_memory
  -> writer planner and validator
  -> kmp_ingest
  -> wake / ask / trace / inspect / temporal moves
```

This protocol belongs above the core memory model. It may be exposed first as
MCP tools and later through SDKs or HTTP. Internally it compiles to normal
`kmp_ingest` requests.

## API/MCP Gap Rule

MCP must not define a second memory API.

Every writer helper is a transport binding over the same Kernel Memory
Protocol contract:

```text
kmp_write_memory request
  -> writer validation and deterministic planning
  -> canonical kmp_ingest request
  -> KernelMemoryService.Ingest in live gRPC mode
```

The public contract must stay aligned across:

- `api/examples/kernel/v1beta1/kmp/write-memory.request.json`;
- `api/examples/kernel/v1beta1/kmp/write-memory.response.json`;
- `api/examples/kernel/v1beta1/kmp/kernel-memory-protocol.schema.json`;
- MCP `tools/list` input schema for `kmp_write_memory`;
- server routing that forwards committed writes through `kmp_ingest`.

If one of these surfaces changes, the others must change in the same slice.
`kmp_write_memory` may reduce authoring complexity, but it must always
return or call the exact low-level ingest shape the API already understands.

## Semantic Integrity Rule

The writer must not create vague or invented relations.

A relation is valid only when the writer can explain both sides:

```text
previous memory -> why this new node exists -> new memory
```

For process memory, the relation is not decorative metadata. It is the semantic
reason the next node belongs after, or because of, prior memory. If the writer
cannot identify the prior ref, the semantic change, and the evidence behind the
change, it must stop and read more context instead of guessing.

This is a hard product rule:

- no vague `related_to`-style relations in writer helpers;
- no invented causal links;
- no relation without a specific target ref;
- no non-structural relation without `why` and `evidence`;
- no state-changing node without awareness of what previous state it changes;
- no confident rich relation when the writer has not inspected enough memory.

The LLM writer is expected to use read tools before writing when context is
insufficient.

If the writer still cannot justify a rich relation after reading enough context,
it may fall back to an anemic relation from the minimal vocabulary:

- `follows` for temporal/process succession;
- `answers` for feedback answering a prior question/turn;
- `uses_background` when a node is scoped to provided background.

This fallback must be explicit and auditable. The relation should be marked as
procedural or evidential, include `why`/`evidence`, and avoid claiming causal,
motivational, or constraint semantics.

## Goals

- Make writing memory easy for LLMs without weakening kernel validation.
- Produce intelligent relations that explain why two nodes are connected.
- Force the writer to be conscious of the previous node, the next node, and the
  semantic relation between them.
- Preserve deterministic refs, coordinates, provenance, and idempotency.
- Make `agentic_process` naturally elongated through process edges, not only a
  star of entries under one anchor.
- Make `trace`, `inspect`, L1/L2/L3 rendering, and temporal traversal useful
  immediately after write.
- Keep domain reasoning and LLM interpretation outside kernel core.

## Non-Goals

- Do not replace `kmp_ingest`.
- Do not add generative reasoning inside kernel core.
- Do not let MCP become the owner of memory behavior.
- Do not create benchmark-specific write rules in core.
- Do not silently create weak relations without `why` and `evidence`.
- Do not block useful memory writes just because a rich relation cannot be
  justified; fall back to explicit anemic process relations when appropriate.

## Public Mental Model

A writer does not "insert graph rows".

A writer records a semantic memory event:

```text
what happened
what changed
why it matters
which previous memory it depends on
which evidence supports it
```

Before writing, the writer must know enough of the graph to decide what the
next node means relative to existing memory. That may require reading beyond
the current episode:

- inspect the immediate previous node;
- inspect the current process path;
- look across other dimensions such as actor, preference, entity, task, or
  benchmark case;
- move temporally with `near`, `rewind`, `forward`, or `goto`;
- trace existing paths before adding a new causal or motivational edge.

The write helper turns that into canonical memory:

```text
dimensions + entries + relations + evidence + provenance + idempotency
```

## First Tool

P0 introduces one general writer-first MCP tool:

```text
kmp_write_memory
```

The tool validates, compiles, and forwards to `kmp_ingest` in one call by
default (`dry_run=false`, also the behavior when the option is omitted).
Validation failure writes nothing. Set `dry_run=true` only for an explicitly
requested preview or payload debugging; that response returns the generated
`kmp_ingest` payload and never commits it.

Specialized helper aliases can come later:

- `kernel_write_turn`;
- `kernel_write_decision`;
- `kernel_write_feedback`;
- `kernel_write_delta`;
- `kernel_link`.

They should compile to the same internal write request shape.

## Request Shape

Initial shape:

```json
{
  "about": "incident:mobile-login",
  "intent": "record_decision",
  "actor": "agent:backend",
  "observed_at": "2026-05-06T10:00:00Z",
  "scope": {
    "task": "incident:mobile-login",
    "process": "incident:mobile-login:resolution",
    "episode": "incident:mobile-login:episode:backend"
  },
  "current": {
    "kind": "decision",
    "summary": "Use token refresh retry instead of widening timeout.",
    "evidence": "Logs show 401 immediately after token refresh."
  },
  "semantic_delta": {
    "from": "The team suspected network timeout.",
    "to": "The evidence points to token refresh race.",
    "why": "The failing requests return 401 immediately after refresh.",
    "evidence": "Auth logs show refresh success followed by 401 on the next request."
  },
  "connect_to": [
    {
      "ref": "incident:mobile-login:observation:401-refresh-race",
      "rel": "chosen_because",
      "class": "causal",
      "why": "The decision addresses the observed token refresh race.",
      "evidence": "The chosen retry targets the refresh race seen in auth logs."
    }
  ],
  "read_context": {
    "inspected_refs": [
      "incident:mobile-login:observation:401-refresh-race"
    ]
  },
  "idempotency_key": "write:incident-mobile-login-decision-v1",
  "options": {
    "dry_run": false,
    "strict": true
  }
}
```

## Required Fields

| Field | Required | Meaning |
| --- | --- | --- |
| `about` | yes | Memory anchor. |
| `intent` | yes | Writer intent: `record_turn`, `record_observation`, `record_decision`, `record_feedback`, or `record_delta`. |
| `actor` | yes | Human, agent, or component producing the write. |
| `observed_at` | yes | RFC3339 timestamp for provenance and default coordinates. |
| `scope.process` | yes | Process dimension scope. |
| `current.kind` | yes | Entry kind to write. |
| `current.summary` | yes | Entry text. |
| `current.evidence` | yes in strict mode | Direct evidence for the new memory entry. |
| `connect_to` | yes in P0 strict mode | At least one explicit relation to an existing or declared ref. |
| `connect_to[].ref` | required for each link | Existing or declared target ref. |
| `connect_to[].rel` | required for each link | Relation type. |
| `connect_to[].class` | required for each link | Relation semantic class. |
| `connect_to[].why` | required for non-structural links | Why the relation exists. |
| `connect_to[].evidence` | required for non-structural links | Evidence for the relation. |
| `read_context` | required for strict rich external relations | Audit refs observed through read tools before writing. |

## Intent Semantics

| Intent | Writes | Typical relations |
| --- | --- | --- |
| `record_turn` | a conversation/process turn | `follows`, `depends_on`, `updates_state` |
| `record_observation` | a fact observed by an agent/tool/human | `supports`, `contradicts`, `updates_state` |
| `record_decision` | a decision node | `chosen_because`, `satisfies_constraint`, `violates_constraint` |
| `record_feedback` | environment/tool/user feedback | `answers`, `confirms_selection`, `supersedes` |
| `record_delta` | semantic difference between old and new state | `semantic_delta_from`, `updates_state`, `contradicts` |

`link_existing` is intentionally not part of P0 because the canonical
`kmp_ingest` contract currently requires at least one entry. A future
`kernel_link` helper must either introduce a proper link-event entry or extend
the low-level API explicitly; it must not silently invent a dummy node.

## Node Kinds

P0 allows a small, stable set of writer-friendly entry kinds:

- `turn`;
- `observation`;
- `decision`;
- `feedback`;
- `semantic_delta`;
- `constraint`;
- `preference`;
- `derived_value`;
- `error_path`;
- `success_path`.

The low-level `kmp_ingest` remains more general. This list only constrains
the first writer helper.

## Relation Vocabulary

The canonical relation vocabulary is a core kernel contract implemented in
`kmp-domain` as relation type/value-object semantics. Adapters,
benchmarks, MCP, and readers must consume that contract instead of carrying
their own ad hoc relation lists.

P0 canonical relation names:

| Relation | Class | Meaning |
| --- | --- | --- |
| `follows` | `procedural` | Next step in a process. |
| `answers` | `evidential` | A response answers a question or turn. |
| `depends_on` | `causal` | Current memory depends on prior memory. |
| `chosen_because` | `causal` or `motivational` | A decision was selected because of prior evidence/reason. |
| `semantic_delta_from` | `causal` | New memory expresses the semantic change from previous memory. |
| `updates_state` | `causal` | New memory changes process state. |
| `supports` | `evidential` | An observation supports a prior or target memory. |
| `supersedes` | `evidential` | New memory replaces prior memory. |
| `contradicts` | `evidential` | New memory invalidates prior memory. |
| `confirms_selection` | `evidential` or `motivational` | Feedback confirms a prior selected option. |
| `satisfies_constraint` | `constraint` | A decision/candidate meets a rule. |
| `violates_constraint` | `constraint` | A decision/candidate breaks a rule. |
| `contributes_to` | `evidential` | A dispersed value participates in an aggregate. |
| `excluded_from` | `constraint` | A seen value is intentionally excluded. |
| `checked_against` | `constraint` | A derived value is compared to a rule/budget/window. |
| `derived_from` | `evidential` | A result was computed from operands/evidence. |
| `restates` | `evidential` | A later node repeats the same fact/value and should not be double-counted. |
| `corrects` | `evidential` | A later node corrects an earlier fact/value. |
| `component_of` | `evidential` | A value is one component of a larger total or set. |
| `total_of` | `evidential` | A value is an aggregate total for component values. |
| `same_event_as` | `evidential` | Two refs describe the same event. |
| `same_entity_as` | `evidential` | Two refs describe the same entity. |
| `qualifies_as` | `evidential` | A ref qualifies as a specific semantic item. |
| `matches_requirement` | `constraint` | A ref satisfies a query or requirement predicate. |

Relations outside this vocabulary are rejected by `kmp_write_memory` in
strict mode unless the caller explicitly opts into an extension namespace. The
first implementation should prefer rejecting an unclear relation over accepting
a weak rich relation.

Anemic relations are allowed as honest fallback relations:

| Relation | Class | When to use |
| --- | --- | --- |
| `follows` | `procedural` | The writer only knows the new node follows a prior node in sequence. |
| `answers` | `evidential` | The writer only knows the new node is feedback/answer to a prior question. |
| `uses_background` | `evidential` | The writer only knows the new node is scoped to background/context. |

Anemic fallback must not be upgraded into causal, motivational, or constraint
language without evidence.

Structural relations accepted by the writer helper are limited to `contains`,
`member_of`, and `scoped_to`. They are classified as `structural` and excluded
from semantic writer quality ratios.

## Relation Quality Metrics

The write path needs first-class relation quality metrics so teams can see
whether the graph is becoming semantically useful or falling back to anemic
process links.

Every accepted relation should be classified at write time:

| Quality | Meaning |
| --- | --- |
| `rich` | Specific semantic relation with non-structural class, target ref, `why`, and `evidence`. Examples: `chosen_because`, `semantic_delta_from`, `updates_state`, `satisfies_constraint`. |
| `anemic` | Honest minimal fallback relation. Allowed names: `follows`, `answers`, `uses_background`. |
| `structural` | Kernel/container relation such as dimension membership or record ownership. Not counted as writer semantic quality. |
| `invalid` | Missing target, unsupported relation name, missing proof, or impossible class/name pairing. Must fail fast. |
| `suspect` | Accepted only in non-strict/advisory mode, but weak: generic wording, low confidence, no inspected prior context, or relation evidence does not mention both endpoints. |

Core counters:

| Metric | Formula | Target |
| --- | --- | --- |
| `relation_total` | accepted writer relations | baseline volume |
| `relation_rich_count` | count quality=`rich` | maximize |
| `relation_anemic_count` | count quality=`anemic` | allowed but visible |
| `relation_invalid_rejected_count` | rejected invalid relations | should be visible, not hidden |
| `relation_suspect_count` | accepted weak relations in advisory mode | trend to zero |
| `relation_rich_ratio` | `rich / (rich + anemic + suspect)` | high for mature writers |
| `relation_anemic_ratio` | `anemic / (rich + anemic + suspect)` | acceptable early, should decrease |
| `relation_explanatory_ratio` | `(causal + motivational + evidential + constraint) / non_structural` | high |
| `relation_proof_coverage` | relations with both `why` and `evidence` / writer relations | 1.0 in strict mode |
| `relation_target_coverage` | relations whose target ref exists or is declared external / writer relations | 1.0 |
| `relation_prior_context_coverage` | rich external relations whose target was read before write / rich external relations | 1.0 in strict mode |
| `relation_endpoint_text_overlap` | relations whose evidence mentions terms from both endpoint texts / writer relations | advisory signal |
| `process_elongation_ratio` | process edges forming a chain / total process entries | high for process memory |
| `star_shape_ratio` | entries only attached structurally to anchor / total entries | should decrease for agentic processes |

Per-relation diagnostic fields:

```json
{
  "rel": "chosen_because",
  "quality": "rich",
  "quality_reason": "non-structural relation has target ref, why, evidence, and supported semantic class",
  "fallback": false,
  "requires_prior_context": true,
  "prior_context_observed": true,
  "prior_context_sources": ["kmp_inspect"]
}
```

For an anemic fallback:

```json
{
  "rel": "follows",
  "quality": "anemic",
  "quality_reason": "writer could prove temporal succession but not a richer semantic dependency",
  "fallback": true,
  "requires_prior_context": false,
  "prior_context_observed": false,
  "prior_context_sources": []
}
```

This metric layer is not an LLM judge. P1 implements deterministic quality
classification from the relation name, class, `why`, `evidence`, confidence,
endpoint refs, and declared read context. Later P2 can add an optional
judge/plugin to audit whether `why` and `evidence` genuinely support the
endpoint texts.

## Read Before Write

The writer protocol is intentionally read/write, not write-only.

When a writer is not certain which prior memory the new node should connect to,
it must use kernel read tools first:

| Need | Tool |
| --- | --- |
| Resume current state | `kmp_wake` |
| Inspect a candidate prior ref | `kmp_inspect` |
| Understand a path between two refs | `kmp_trace` |
| See temporal neighborhood | `kmp_near` |
| Move backward through a process | `kmp_rewind` |
| Move forward after a known ref | `kmp_forward` |
| Reconstruct state at a point | `kmp_goto` |
| Ask for deterministic evidence | `kmp_ask` |

P1 makes this auditable through `read_context`:

```json
{
  "read_context": {
    "inspected_refs": ["incident:mobile-login:observation:401-refresh-race"],
    "trace_paths": [
      {
        "from": "incident:mobile-login:start",
        "to": "incident:mobile-login:observation:401-refresh-race",
        "refs": ["incident:mobile-login:observation:auth-logs"]
      }
    ],
    "temporal_refs": ["incident:mobile-login:observation:401-refresh-race"],
    "wake_refs": ["incident:mobile-login:current-state"],
    "ask_refs": ["incident:mobile-login:evidence:auth-log"]
  }
}
```

In strict mode, a rich relation to an external target ref is rejected unless
that target appears in `read_context`. A relation to a node generated in the
same request is marked with `prior_context_sources=["current_request"]`.
Anemic fallback relations remain allowed without prior-read proof, but the
missing prior context is visible in `relation_quality`.

The planner may later support a "needs more context" diagnostic. The current
implementation is fail-fast: if the writer cannot provide a target relation
with proof and required read context, the write is rejected and the caller must
read more context before retrying. If a memory write is still valuable and the
writer has enough evidence for sequence, answerhood, or background usage, it
may compile an explicit anemic fallback relation instead.

Example diagnostic:

```json
{
  "accepted": false,
  "dry_run": true,
  "diagnostics": [
    "Cannot write chosen_because: target decision ref is unknown. Inspect the latest process node first."
  ],
  "next_suggested_reads": [
    {
      "tool": "kmp_near",
      "about": "incident:mobile-login",
      "around": { "ref": "incident:mobile-login:latest" }
    }
  ]
}
```

## Dispersed Values

For values spread across multiple nodes, the writer must not ask the kernel to
"just sum everything". It must express operand relevance with relations.

Example:

```json
{
  "from": "trip:flight:cost",
  "to": "trip:committed_total",
  "rel": "contributes_to",
  "class": "evidential",
  "why": "The flight is a confirmed trip expense.",
  "evidence": "$120 flight was booked."
}
```

Then a reader/plugin can select operands by graph relation:

```text
contributes_to -> derived_value
excluded_from  -> ignored value with proof
checked_against -> compare derived value with constraint
restates / same_event_as -> duplicate evidence to count once
corrects / supersedes -> choose the latest corrected value
```

This keeps arithmetic outside core while making operand selection auditable.

## Generated Low-Level Memory

`kmp_write_memory` generates:

- dimensions:
  - `task` when provided;
  - `agentic_process` from `scope.process`;
  - `agentic_episode` when provided;
- entries:
  - one current entry;
  - optional `semantic_delta` entry;
- relations:
  - user-provided `connect_to` links;
  - `semantic_delta_from` when delta links to prior refs;
  - `updates_state` when appropriate;
- evidence:
  - entry evidence;
  - relation evidence;
- provenance:
  - `source_kind`;
  - `source_agent`;
  - `observed_at`;
  - correlation and causation ids;
- idempotency:
  - deterministic hash over about, intent, actor, current summary, semantic
    delta, links, and observed_at unless explicitly supplied.

## Response Shape

Dry run response:

```json
{
  "accepted": false,
  "dry_run": true,
  "summary": "Prepared 2 entries, 3 relations, and 2 evidence items.",
  "generated_refs": [
    "incident:mobile-login:decision:use-refresh-retry",
    "incident:mobile-login:delta:timeout-to-refresh-race"
  ],
  "relations": [
    "chosen_because",
    "semantic_delta_from"
  ],
  "ingest_preview": {
    "about": "...",
    "memory": {}
  },
  "diagnostics": [],
  "next_suggested_reads": [
    {
      "tool": "kmp_trace",
      "from": "incident:mobile-login:decision:use-refresh-retry",
      "to": "incident:mobile-login:observation:401-refresh-race"
    }
  ]
}
```

Commit response includes the normal ingest result plus the same generated refs
and suggested reads.

## Validation

The writer helper fails fast when:

- `about` is missing or blank;
- `intent` is unknown;
- `actor` is missing or blank;
- `observed_at` is missing or blank;
- `scope.process` is missing or blank;
- `current.summary` is missing for write intents;
- non-structural relations lack `why`;
- non-structural relations lack `evidence`;
- strict mode has no `connect_to` relation;
- relation class is not a kernel semantic class;
- target refs are blank;
- `semantic_delta` lacks `from`, `to`, `why`, or `evidence`;
- `dry_run=false` cannot produce stable idempotency;
- generated refs collide inside the request.
- relation `rel` is vague, unsupported, or outside the strict vocabulary;
- the writer claims a rich state change without a prior ref or explicit
  external evidence;
- the request admits insufficient context for the proposed rich relation and no
  valid anemic fallback relation is provided.

Strict mode should be the default.

## LLM Usage Loop

The intended LLM loop is:

1. `kmp_wake` the current memory anchor.
2. Use `kmp_near`, `kmp_rewind`, `kmp_forward`, or `kmp_goto` when
   the next node depends on temporal or cross-dimensional context.
3. `kmp_inspect` important refs before creating causal, motivational, or
   constraint relations.
4. `kmp_trace` candidate paths when the writer needs to understand why a
   prior state led to the current one.
5. Write a concise semantic memory event once through `kmp_write_memory`; omit
   `options.dry_run` or set it to `false`. The planner validates before ingest
   and an invalid request writes nothing.
6. Use `options.dry_run=true` instead only when the user requested a preview,
   the compiled payload needs debugging, or a deliberate human review must
   precede mutation. A preview does not authorize a later commit.
7. Use `kmp_trace` or `kmp_near` to verify the new graph shape when needed.

This gives the LLM a validated single-call writing workflow without requiring
it to manually construct the full `kmp_ingest` payload or pay for a redundant
preview round trip.

## MCP Implementation Plan

P0.1:

- [x] add this planning document;
- [x] link it from the documentation index;
- [x] document that `kmp_ingest` remains canonical.

P0.2:

- [x] add `kmp_write_memory` to MCP `tools/list`;
- [x] implement a writer planner module in `kmp-mcp`;
- [x] support `dry_run=true`;
- [x] reject vague or unsupported relation names in strict mode;
- [x] fail fast when the writer cannot justify at least one target relation for the
  next node;
- [x] allow explicit anemic fallback relations (`follows`, `answers`,
  `uses_background`) when a rich relation is not justified;
- [x] classify every relation as `rich`, `anemic`, `structural`, `invalid`, or
  `suspect` and return relation quality metrics in the write response;
- [x] generate the canonical `kmp_ingest` payload;
- [x] validate fail-fast before forwarding;
- [x] return generated refs, relations, preview, and suggested reads.

P0.3:

- [x] when `dry_run=false`, call the same backend path as `kmp_ingest`;
- [x] preserve backend independence: fixture and gRPC both use the same planner;
- [x] add tests for dry-run, invalid relation, missing process scope, and stable
  idempotency.

P1:

- [x] enforce `read_context` evidence for strict rich external relations;
- [x] return `prior_context_sources` per relation and prior-context coverage
  metrics;
- [ ] add an offline GPT-5.5 teacher pipeline for writer datasets, with exact
  model provenance, structured relation output, cited KMP evidence, and
  fail-fast behavior when the teacher is unavailable;
- [ ] keep 0.5B Operator writer mode scoped to KMP operation, prepared-write
  execution, escalation policy, and bounded tool calls; do not treat it as a
  rich-relation author;
- [ ] add specialized helper aliases for common writer tasks;
- [x] add schema-constrained LLM writer prompts/templates outside core
  (`api/examples/inference-prompts/kmp-write-memory.txt` and
  `kmp-write-memory.request.json`);
- [ ] add broader read-before-write writer policies across dimensions and temporal
  windows;
- [ ] add graph-quality metrics for process elongation and explanatory relation
  density;
- [ ] add optional relation-quality judge/plugin to audit whether relation evidence
  really supports both endpoints;
- [x] run MemoryArena with writer-first ingestion and compare trace quality against
  the flat adapter.

## Success Criteria

- A general LLM can write a decision or semantic delta without hand-authoring
  low-level coordinates.
- The generated ingest request is inspectable and deterministic.
- The resulting graph has meaningful causal/motivational/constraint relations.
- `kmp_trace` explains why a node follows from a prior node.
- `kmp_ask` can surface the right evidence without relying only on blobs.
- The core remains free of LLM-specific behavior.
