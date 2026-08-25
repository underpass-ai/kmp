# Kernel Memory Protocol And MCP Action Plan

Date: 2026-04-30

Status: first typed gRPC and MCP live cut implemented on
`feat/kernel-memory-service-grpc`. This document is now the roadmap and product
context for the memory protocol, not a claim that every planned capability is
still missing.

## Purpose

This document defines the next capability the kernel must provide: a public
memory protocol for agents and humans.

The kernel should let any Underpass project ingest memory and later wake up,
ask, goto, near, rewind, forward, trace, or inspect that memory by:

- anchor;
- scope;
- dimension;
- time;
- sequence;
- relation path;
- provenance.

Time is a transversal axis. It can move through one dimension, a chosen set of
dimensions, or all dimensions of a memory.

This must work for conversations, user history, production incidents, agent
actions, workflow attempts, benchmark records, and future applications.

The core rule is:

```text
Memory moves outside, rich traversal inside.
```

## Public Status

This is a Kernel 1.0 evolution plan, not a separate rewrite.

Kernel 1.0 already has:

- a node-centric graph read path;
- async projection ingestion;
- gRPC query APIs;
- Neo4j-backed relationship traversal;
- Valkey-backed detail retrieval;
- rendered context bundles with semantic relationship metadata;
- typed `KernelMemoryService` over Kernel Memory Protocol moves;
- API-owned memory ingest mapped through the application command path;
- domain-owned temporal traversal over `contains_entry` coordinates;
- about-scoped dimension identities shaped as
  `about:<about>:dimension:<dimension_id>`;
- explicit dimension scope selection: `CURRENT_ABOUT`, `ABOUTS`, and
  `ALL_ABOUTS`;
- an installable stdio MCP adapter that calls `KernelMemoryService` live.

Kernel 1.0 still does not have:

- generated `Ask` answers; `Ask` is deterministic evidence recovery plus proof;
- inferred conflict resolution beyond explicit relation proof;
- KMP-specific NATS subjects such as `kernel.memory.ingest`;
- a crates.io-published MCP package;
- benchmark re-measurement proving which categories improved.

This plan closes that gap using the existing Kernel 1.0 architecture. It does
not require an embedded database, a new storage stack, or a GPU-bound traversal
pipeline.

## Product Thesis

The kernel remains application-agnostic, but it must become traversal-aware.

Applications should not need to reconstruct context from their own databases
before the kernel can be useful. They may keep their own domain databases, but
once memory is recorded into the kernel, the kernel must be able to wake a
caller up, answer with proof, trace why, and show what is missing.

The kernel should behave like:

```text
ingest memory with dimensions, entries, relations, evidence, time, and provenance
wake a caller with the state needed to continue
answer with proof or say unknown
goto a concrete moment across selected dimensions
find memory near a concrete moment across selected dimensions
rewind or forward through time across selected dimensions
trace the causal/evidential path
inspect the raw substrate when needed
```

The kernel should not behave like:

```text
dump a flat bundle of text and force the caller to understand the storage graph
```

## Public Promise

The honest public promise is:

```text
If you ingest memory with dimensions, entries, relations, evidence, time, and provenance, the
kernel can later wake up a caller, answer with proof, trace why, and show what
is missing.
```

The promise is not:

```text
The kernel magically understands every application database.
The kernel solves memory by vector search alone.
The kernel answers every question without an LLM.
```

Basic traversal should be deterministic CPU/I/O. LLMs remain useful for
relationship extraction, summarization, reranking, and final answer generation.

## Kernel Vocabulary

`session` is not a kernel-level concept.

The kernel-level concept is:

```text
scope
```

A scope is any bounded region of context that can be traversed. An application
may call that region a session, conversation, attempt, incident phase, workflow
run, tool-call group, benchmark haystack segment, or deployment window. Those
names belong to the application layer.

Canonical kernel vocabulary:

```text
anchor
dimension
scope
entry
sequence
timestamp
relation
detail
provenance
```

Application vocabulary is mapped into kernel vocabulary at ingestion time:

```text
application "session"       -> kernel dimension kind "conversation" and scope
application "turn"          -> kernel entry with sequence index
application "attempt"       -> kernel dimension kind "attempt" and scope
application "incident step" -> kernel entry in dimension kind "incident"
```

This keeps the kernel agnostic while still allowing precise traversal.

## First Public Slice

The first public slice was intentionally small and is now implemented:

1. Define the `anchor/dimension/scope/entry/time/relation/detail/provenance`
   contract.
2. Add JSON schemas and examples for KMP moves.
3. Add typed gRPC `KernelMemoryService` over domain/application memory
   behavior.
4. Add MCP tools as a thin live client of the typed memory service.
5. Prove temporal and multi-about retrieval with deterministic integration and
   Helm lifecycle tests.
6. Publish the limitation: this is traversal and evidence recovery, not a claim
   that every benchmark category is solved.

The demo should answer questions like:

```text
Was this statement before or after the earlier one?
Which later entry superseded the previous fact?
Which entries across scopes support this answer?
What did memory know before this timestamp across all dimensions?
What changed after this entry in only the workflow and incident dimensions?
```

And it should return:

- the answer;
- the scope and entry position;
- the dimension coverage;
- the timestamp/order;
- the path when a semantic path exists;
- the evidence text;
- warnings when context is missing or ambiguous.

## Design Principles

### 1. Domain Agnostic, Not Traversal Blind

The kernel should not hard-code LongMemEval, PIR, incident management, or any
single application schema.

It should understand generic traversal coordinates:

```text
where is this context anchored?
which scope contains this entry?
what happened before or after this entry?
which relation path proves this answer?
which detail payload supports this fact?
which source produced it?
```

### 2. One Mental Model For Ingestion And Reading

The same concepts must round-trip:

```text
ingest anchor          -> read anchor
ingest dimension       -> read dimension
ingest scope           -> read scope
ingest entry           -> read entry
ingest relation        -> read path
ingest detail          -> read evidence
ingest timestamp/order -> read before/after position
```

The caller should not learn one model for writing and another model for reading.

### 3. Dimensions Are Graph Paths

A dimension is not just a property on an entry. A dimension is a first-class
path through the graph.

The entry:

```text
claim:rachel-chicago
```

can be placed on several memory paths at the same time:

```text
conversation:answer-0b1a0942 -> claim:rachel-chicago
person:rachel                -> claim:rachel-chicago
question:830ce83f            -> claim:rachel-chicago
```

Each path can carry its own traversal coordinates. `sequence` in a
conversation, `valid_from` for an entity, and ranking inside a benchmark record
are not global properties of the claim. They are properties of the position of
that claim within a specific dimension/scope path.

Graph shape:

```text
anchor
  -[:HAS_DIMENSION]-> dimension/scope
  -[:RECORDS]-> entry

dimension/scope
  -[:CONTAINS_ENTRY {
      dimension,
      scope_id,
      sequence,
      occurred_at,
      valid_from,
      valid_until
    }]-> entry
```

Node properties can duplicate selected coordinates for indexing, but they are
not the source of truth for traversal. The relation from dimension/scope to
entry is where the entry's position in that memory path lives.

This design lets the kernel traverse:

```text
rewind one conversation
forward one entity history
near a timestamp across selected dimensions
goto the state of all dimensions at a moment
trace why two entries are connected
```

### 4. Simple Public Memory Moves

Most callers should only need:

```text
kmp_ingest
kmp_wake
kmp_ask
kmp_goto
kmp_near
kmp_rewind
kmp_forward
kmp_trace
kmp_inspect
```

These are not CRUD endpoints. They are memory moves:

- `ingest` accepts memory with dimensions, entries, relations, evidence, and
  provenance;
- `wake` returns the compact state needed to continue;
- `ask` answers with proof or says unknown;
- `goto` jumps to memory state at a concrete timestamp, sequence, or ref;
- `near` returns the temporal neighborhood around a timestamp, sequence, or ref;
- `rewind` moves backward through one, several, or all dimensions of a memory;
- `forward` moves forward through one, several, or all dimensions of a memory;
- `trace` explains the relationship path;
- `inspect` exposes typed stored facts, direct links, evidence, and typed raw
  audit refs when requested.

Lower-level tools can exist for advanced/debug use, but the common path must
feel like memory, not storage plumbing.

### 5. CPU/I/O By Default

Graph traversal is not a GPU workload.

Traversal by anchor, dimension, scope, timestamp, sequence, relation type, or
provenance path is CPU and storage/index work.

GPU is only needed when the flow explicitly asks for model inference:

- relation extraction;
- evidence atom extraction;
- summarization;
- semantic reranking;
- final generative answer.

Basic context exploration should be cheap and scalable without GPU when the
graph has the required structure and indexes.

### 6. MCP Is A First-Class Protocol

Existing event and gRPC paths remain valid.

MCP becomes a first-class interface for:

- agents;
- IDE/tool clients;
- eval harnesses;
- demos;
- production tools;
- cross-project inspection.

The MCP server should call the same kernel application services used by gRPC. It
must not duplicate kernel logic.

## Required Traversal Capabilities

### Temporal Traversal

Time is not a standalone application dimension. It is the axis that lets the
kernel move through all memory dimensions.

The kernel must answer:

```text
what happened before this?
what happened after this?
what was the memory state at this exact time?
what happened near this time?
what was known at this time?
which fact is latest before this timestamp?
which later fact supersedes this earlier fact?
what changed after this point in only these dimensions?
what did all dimensions know before this timestamp?
```

Supported forms:

```text
n entries before
n entries after
between timestamps
before timestamp
after timestamp
latest before timestamp
latest after timestamp
goto timestamp
goto sequence
goto ref
near timestamp
near sequence
near ref
rewind one dimension
rewind selected dimensions
rewind all dimensions
forward one dimension
forward selected dimensions
forward all dimensions
```

### Dimensional Traversal

The kernel must traverse by application-neutral dimensions:

```text
scope
conversation
action
attempt
incident
workflow
question
tool_call
deployment
agent
benchmark_record
```

If an application calls a grouping a "session", that term stays outside the
kernel contract and is mapped to a kernel dimension plus a scope.

Dimension selection is explicit:

```json
{ "mode": "all" }
```

```json
{ "mode": "only", "include": ["conversation", "entity"] }
```

```json
{ "mode": "except", "exclude": ["raw"] }
```

### Semantic Traversal

The kernel must traverse by relation type and semantic class.

Examples:

```text
EVIDENCE_FOR_ANSWER
AGGREGATES_WITH
UPDATES_PREVIOUS_FACT
SUPERSEDES
TEMPORAL_CONTEXT
DISAMBIGUATING_CONTEXT
CAUSED_BY
MITIGATED_BY
RETRIED_AS
NEXT_ENTRY
HAS_SCOPE
```

### Raw Inspection Traversal

Semantic traversal is not enough. Debugging and audit require raw inspection.

Raw context should be traversable without polluting normal semantic reads. In
the current typed gRPC/MCP cut, raw expansion returns typed raw audit refs:

```text
raw_context -> scope -> entry -> next entry
incident_raw -> alert -> log excerpt -> metric sample
workflow_raw -> action -> tool call -> result
```

## Conversation Example

A caller should be able to ask:

```text
For this graph, use all available dimensions and tell me whether this data point
was said before or after the referenced conversation. Identify the scope, entry,
timestamp, and path that prove it.
```

Expected KMP evidence/proof shape:

```json
{
  "answer": "My friend Rachel actually just moved back to the suburbs again.",
  "because": [
    {
      "claim": "Rachel moved back to the suburbs.",
      "evidence": "My friend Rachel actually just moved back to the suburbs again.",
      "ref": "claim:rachel-suburbs"
    }
  ],
  "proof": {
    "path": [
      {
        "from": "claim:rachel-suburbs",
        "to": "claim:rachel-chicago",
        "rel": "supersedes",
        "class": "evidential",
        "confidence": "high"
      }
    ],
    "evidence": [
      {
        "id": "evidence:rachel-suburbs",
        "supports": ["claim:rachel-suburbs"],
        "text": "My friend Rachel actually just moved back to the suburbs again.",
        "source": "conversation turn"
      }
    ],
    "conflicts": [],
    "missing": [],
    "confidence": "high"
  }
}
```

The important behavior is not the final text alone. The kernel must explain
where the datum lives, whether it is before or after another datum, and which
scope/dimension/path proves it. Temporal position is reported through
`goto`/`near`/`rewind`/`forward` entries and their namespaced coordinates, not
as a synthetic `proof.position` field on `ask`.

## Memory Ingest Contract

Primary write move:

```text
kmp_ingest
```

Minimal input:

```json
{
  "about": "question:830ce83f",
  "memory": {
    "dimensions": [
      {
        "id": "conversation:answer-0b1a0942",
        "kind": "conversation",
        "title": "Rachel relocation discussion"
      },
      {
        "id": "person:rachel",
        "kind": "entity"
      }
    ],
    "entries": [
      {
        "id": "claim:rachel-chicago",
        "kind": "claim",
        "text": "She moved to Chicago.",
        "coordinates": [
          {
            "dimension": "conversation",
            "scope_id": "conversation:answer-0b1a0942",
            "sequence": 4,
            "occurred_at": "2023-05-24T22:23:00Z"
          },
          {
            "dimension": "entity",
            "scope_id": "person:rachel",
            "valid_from": "2023-05-24T22:23:00Z"
          }
        ]
      }
    ],
    "relations": [],
    "evidence": []
  },
  "idempotency_key": "ingest:830ce83f:1"
}
```

Optional semantic link:

```json
{
  "from": "claim:rachel-suburbs",
  "to": "claim:rachel-chicago",
  "rel": "supersedes",
  "class": "evidential",
  "why": "The later statement updates the earlier location.",
  "evidence": "My friend Rachel actually just moved back to the suburbs again.",
  "confidence": "high"
}
```

Ingest defaults:

- preserve producer-provided temporal coordinates inside each dimension and
  scope;
- require callers to provide `sequence`/`rank` when they want those ordering
  coordinates; the current cut does not auto-assign missing sequence values;
- store entry text in the memory entry payload and store evidence only when the
  request supplies explicit evidence records;
- namespace dimension ids by `about` and project dimension, entry, relation,
  and evidence records into the read model for traversal;
- accept producer-provided semantic relations;
- allow later enrichment to add relations through future memory or projection
  writes.

### Ingest Modes

Raw mode:

- caller provides dimensions and entries only;
- kernel stores traversable typed memory entries with producer-provided
  coordinates; requested raw expansion returns typed audit refs rather than
  opaque storage payloads;
- use for transcripts, logs, histories, and benchmark haystacks.

Semantic mode:

- caller provides relations and selected evidence;
- kernel stores sparse, answer-oriented graph paths;
- use when an app or LLM already computed relevant relations.

Hybrid mode:

- caller provides raw entries plus selected semantic relations;
- kernel keeps auditability while normal reads stay proof-oriented and scoped.

## Wake And Ask Contract

Primary continuation move:

```text
kmp_wake
```

Minimal input:

```json
{
  "about": "question:830ce83f",
  "intent": "answer the relocation question",
  "budget": {
    "tokens": 1200
  }
}
```

`wake` should return:

- objective;
- current state;
- causal/evidential spine;
- open loops;
- next actions;
- guardrails;
- proof and missing evidence.

Primary question move:

```text
kmp_ask
```

Minimal input:

```json
{
  "about": "question:830ce83f",
  "question": "Where did Rachel move after her recent relocation?",
  "answer_policy": "evidence_or_unknown"
}
```

Current live `kmp_ask` behavior is deterministic and bounded:

- omitted dimension selection defaults to all dimensions in the current `about`;
- callers may select one, several, or all abouts through `dimensions`;
- the kernel reads the selected memory bundle and builds `answer` from selected
  evidence reasons;
- `show_conflicts` follows the same deterministic evidence path and surfaces
  explicit conflict relations in `proof.conflicts`;
- `best_effort` does not generate fallback text;
- hidden conflict inference and conflict resolution require future proof-model
  work.

Optional constraints:

```json
{
  "about": "question:830ce83f",
  "question": "Was the suburbs statement before or after the Chicago statement?",
  "answer_policy": "evidence_or_unknown",
  "dimensions": {
    "mode": "all"
  },
  "include": {
    "path": true,
    "evidence": true
  },
  "budget": {
    "tokens": 4096
  }
}
```

Expected output:

```json
{
  "summary": "Deterministic memory answer from 1 evidence item for: Was the suburbs statement before or after the Chicago statement?",
  "answer": "My friend Rachel actually just moved back to the suburbs again.",
  "because": [
    {
      "claim": "Rachel moved back to the suburbs.",
      "evidence": "My friend Rachel actually just moved back to the suburbs again.",
      "ref": "claim:rachel-suburbs"
    }
  ],
  "proof": {
    "path": [
      {
        "from": "claim:rachel-suburbs",
        "to": "claim:rachel-chicago",
        "rel": "supersedes",
        "class": "evidential",
        "confidence": "high"
      }
    ],
    "evidence": [
      {
        "id": "evidence:rachel-suburbs",
        "supports": ["claim:rachel-suburbs"],
        "text": "My friend Rachel actually just moved back to the suburbs again.",
        "source": "conversation turn"
      }
    ],
    "conflicts": [],
    "missing": [],
    "confidence": "high"
  },
  "warnings": []
}
```

In the current live cut, `ask.answer` is evidence text. It does not synthesize a
fresh temporal conclusion from the question; callers use temporal traversal and
`proof.path` to audit before/after semantics.

Reading defaults:

- `dimensions.mode="all"` and `dimensions.scope="current_about"` unless
  restricted;
- proof path first;
- raw neighboring claims are not expanded in this cut;
- bounded output by token budget;
- current/superseding facts are visible when producer-provided relations carry
  that path;
- `proof.conflicts` includes explicit conflict relations such as `contradicts`
  or `conflicts_with`; the kernel does not infer hidden contradictions.

## Public Memory Protocol Surface

Detailed request and response shapes are specified in
[`kernel-context-api-design.md`](kernel-context-api-design.md).

### `kmp_ingest`

Ingests memory: dimensions, entries, relations, evidence, and provenance.

Input:

```json
{
  "about": "question:830ce83f",
  "memory": {
    "dimensions": [],
    "entries": [],
    "relations": [],
    "evidence": []
  },
  "idempotency_key": "ingest:830ce83f:1"
}
```

Output:

```json
{
  "summary": "Ingested 2 entries, 1 relation, and 1 evidence item for question:830ce83f.",
  "memory": {
    "about": "question:830ce83f",
    "memory_id": "memory:830ce83f:1",
    "read_after_write_ready": true
  },
  "warnings": []
}
```

### `kmp_wake`

Default continuation tool. Use this when a human or agent needs to resume work.

Input:

```json
{
  "about": "memory:kernel-memory-protocol",
  "role": "implementer",
  "intent": "continue designing the public API",
  "budget": {
    "tokens": 1600
  }
}
```

### `kmp_ask`

Evidence-backed question tool. Use this when the caller needs an answer from
memory.

Input:

```json
{
  "about": "question:830ce83f",
  "question": "Where did Rachel move after her recent relocation?",
  "answer_policy": "evidence_or_unknown"
}
```

### `kmp_goto`

Temporal traversal tool. Use this when the caller needs memory state at a
concrete timestamp, sequence, or ref.

Input:

```json
{
  "about": "question:830ce83f",
  "at": {
    "time": "2026-04-12T15:03:00Z"
  },
  "dimensions": {
    "mode": "all"
  }
}
```

### `kmp_near`

Temporal traversal tool. Use this when the caller needs the temporal
neighborhood around a timestamp, sequence, or ref.

Input:

```json
{
  "about": "question:830ce83f",
  "around": {
    "time": "2026-04-12T15:03:00Z"
  },
  "window": {
    "before_entries": 2,
    "after_entries": 2
  },
  "dimensions": {
    "mode": "only",
    "include": ["conversation", "entity"]
  }
}
```

### `kmp_rewind`

Temporal traversal tool. Use this when the caller needs to know what memory knew
before a cursor, timestamp, or sequence point.

Input:

```json
{
  "about": "question:830ce83f",
  "from": {
    "ref": "claim:rachel-austin"
  },
  "dimensions": {
    "mode": "only",
    "include": ["conversation", "entity"]
  },
  "limit": {
    "entries": 5
  }
}
```

`kmp_rewind` can apply to:

- one dimension;
- a selected set of dimensions;
- all dimensions in the memory.

### `kmp_forward`

Temporal traversal tool. Use this when the caller needs to know what changed
after a cursor, timestamp, or sequence point.

Input:

```json
{
  "about": "question:830ce83f",
  "from": {
    "ref": "claim:rachel-denver"
  },
  "dimensions": {
    "mode": "all"
  },
  "limit": {
    "entries": 5
  }
}
```

Temporal tool outputs include a `page` object with `returned`, `total`,
`has_more`, and `next_cursor`. A partial temporal read is therefore visible to
both humans and LLM clients instead of being confused with a complete traversal.

### `kmp_trace`

Proof-path tool. Use this when the caller needs to know why a memory answer is
true.

Input:

```json
{
  "from": "claim:rachel-austin",
  "to": "claim:rachel-denver",
  "goal": "explain_update"
}
```

### `kmp_inspect`

Raw audit tool for an anchor, scope, claim, entry, relation path, node id, or
correlation id.

Input:

```json
{
  "ref": "claim:rachel-austin",
  "include": {
    "incoming": true,
    "outgoing": true,
    "details": true,
    "raw": false
  }
}
```

### Advanced/Alias Tools

Advanced tools or migration aliases may exist in future cuts, but should not be
the normal caller path. Beyond the nine canonical moves, the current MCP live
adapter exposes two ingest aliases plus one distinct writer-friendly tool, for
ten tools total:

```text
kernel_remember           -> alias for kmp_ingest
kernel_ingest_context     -> alias for kmp_ingest
kmp_write_memory       -> distinct writer-friendly tool with its own schema;
                             validates writer intent and relation quality, then
                             compiles to canonical kmp_ingest
```

## Storage And Indexing Requirements

Minimum indexed coordinates:

```text
node_id
anchor_id
root_node_id
correlation_id
causation_id
scope_id
dimension
dimension_id
entry_index
sequence
rank
timestamp / observed_at / occurred_at / valid_from / valid_until
relation_type
semantic_class
source_agent
source_kind
```

These are coordinates, not domain coupling. They let the kernel traverse context
without knowing application meaning.

## Architecture Direction

MCP should be an adapter over the typed memory API.

Current implemented shape:

```text
client / agent / eval harness
        |
        v
stdio MCP adapter
        |
        v
KernelMemoryService gRPC
        |
        v
kernel application ports
        |
        +--> graph/query services
        +--> command/event ingestion
        +--> detail store
```

Events and gRPC remain supported. MCP is added as the simple tool-facing
protocol without owning traversal logic.

## Compute Model

Basic graph exploration should be CPU/I/O:

```text
index lookup
graph traversal
relation filtering
time-window scan
scope/entry ordering
path reconstruction
detail fetch
```

This is index and traversal work. It should not require GPU.

GPU or external model API is only required for explicit inference:

```text
relation extraction
evidence atom extraction
summarization
semantic reranking
generative final answer
```

Operational expectation:

```text
explore/read graph path -> CPU/I/O
generate/enrich/reason with LLM -> GPU or external model API
```

## Underpass Adoption

All Underpass projects should treat the kernel as the shared context layer.

Migration direction:

1. Existing event and gRPC paths remain supported.
2. New interactive, agentic, and evaluation clients should prefer MCP.
3. Projects should stop assuming context reconstruction must happen from their
   own local databases.
4. Project-specific schemas remain valid, but must map into kernel anchor,
   scope, entry, dimension, relation, detail, and provenance.
5. Cross-project inspection should use the same MCP tools.

Affected projects/classes:

- PIR;
- runtime/orchestrator services;
- LLM/vLLM services;
- evaluation harnesses;
- production incident tooling;
- demos;
- benchmark adapters;
- future Underpass apps that need memory, inspection, or rehydration.

## Implementation Plan

### Phase 0: Public Alignment

Status: delivered for the first memory slice.

Delivered:

- this action plan in public docs;
- a short README/index link so readers can find it;
- explicit non-goals and current limitations;
- implementation status in the active docs.

Exit criteria:

- the plan says what exists today and what does not;
- the plan does not imply benchmark success before the traversal API exists;
- the plan keeps Kernel 1.0 architecture as the base.

### Phase 1: Contract

Status: delivered for KMP fixtures and typed gRPC surface. Domain-specific
example packs can still be added without changing the protocol.

Delivered:

- schemas for `kmp_ingest`, `kmp_wake`, `kmp_ask`, `kmp_goto`,
  `kmp_near`, `kmp_rewind`, `kmp_forward`, `kmp_trace`, and
  `kmp_inspect` under
  [`api/examples/kernel/v1beta1/kmp`](../../api/examples/kernel/v1beta1/kmp);
- migration note for legacy API names that still say "session"; new kernel
  vocabulary should use "scope".

Remaining follow-up:

- optional formal ADR if the KMP contract needs a decision record separate from
  these product/API docs;
- richer conversation, incident, workflow, and benchmark example packs.

### Phase 2: Kernel Query Capabilities

Status: delivered for the gRPC/MCP cut where backed by memory projection and
existing query ports.

Delivered application behavior:

- context by anchor;
- context by dimension;
- temporal goto/near/rewind/forward over one, selected, or all dimensions;
- context path by relation type;
- typed scoped inspection of object/detail/direct links/evidence.

Remaining follow-up:

- richer semantic conflict inference and resolution;
- additional indexes only where live workloads show query pressure.

### Phase 3: Typed gRPC API Before MCP

Status: delivered.

Exposed `KernelMemoryService` over gRPC:

- `kmp_ingest`;
- `kmp_wake`;
- `kmp_ask`;
- `kmp_goto`;
- `kmp_near`;
- `kmp_rewind`;
- `kmp_forward`;
- `kmp_trace`;
- `kmp_inspect`;

Also delivered:

- domain-owned temporal and multidimensional traversal;
- application use cases over existing query/command ports;
- typed protobuf contract and descriptor tests;
- direct gRPC service tests for every memory move;
- deployment smoke through the public gRPC endpoint.

### Phase 4: MCP Server

Status: delivered for stdio live mode.

Delivered:

- MCP service binary or crate;
- local stdio adapter;
- fixture-backed mode;
- live mode via `KernelMemoryService`;
- CI smoke proving MCP tools read from a real containerized kernel through the
  typed memory service;
- auth/TLS/Kubernetes story where remote MCP is needed.

The MCP adapter must not call `ContextQueryService` or
`ContextCommandService` directly for KMP moves after this migration.

### Phase 5: Underpass Adoption

Status: follow-up.

Deliver:

- PIR context inspection/retrieval via MCP;
- eval harness access via MCP;
- production incident context retrieval via MCP;
- demos for temporal and dimensional traversal;
- documentation in affected projects.

### Phase 6: Evaluation

Status: follow-up after the implemented KMP slice is stable in consumers.

Evaluate against:

- conversation multi-scope aggregation;
- knowledge update / supersession;
- temporal ordering;
- temporal goto/near/rewind/forward across one, selected, and all dimensions;
- production incident reconstruction;
- agent attempt/retry reconstruction;
- LongMemEval balanced subsets.

Success criteria:

```text
single-scope recall remains high
multi-scope aggregation improves materially
knowledge-update chooses newest/superseding facts
incident reconstruction can traverse before/after deployment/action/attempt
goto/near/rewind/forward can scope to one dimension, selected dimensions, or all dimensions
MCP clients can retrieve the same context as gRPC clients
basic traversal remains CPU/I/O, not GPU-bound
```

## Experimental Evidence

This section records why the capability is needed. It should not define the
kernel contract.

### LongMemEval Pilot

The LongMemEval adapter exposed the need for multi-scope traversal.

Observed balanced 40-question subset:

| Run | Accuracy | single bounded context | multi-scope | temporal | update |
| --- | ---: | ---: | ---: | ---: | ---: |
| semantic-repair-balanced40-v2 | 25/40 = 0.625 | 10/10 | 3/10 | 7/10 | 5/10 |
| semantic-retrieval-balanced40-v3 | 26/40 = 0.650 | 10/10 | 2/10 | 7/10 | 7/10 |

Interpretation:

- A single bounded conversation works well.
- Update questions improve when retrieval is more aware of recency.
- Multi-scope aggregation remains weak without first-class evidence collection
  and traversal.
- The pre-KMP kernel retrieved what had been connected semantically. The current
  KMP cut adds generic temporal and dimension traversal, but the benchmark still
  needs to be re-run before claiming category-level improvement.

### Concrete Failure Shape

For questions such as:

```text
How many items do I need to pick up or return?
```

The answer requires several evidence atoms across several scopes. A flat context
bundle or one semantic edge is not enough. The kernel must be able to collect
and return each evidence atom with provenance.

For questions such as:

```text
Where did Rachel move after her recent relocation?
```

The answer requires knowing that a later statement supersedes an earlier one.
The kernel must be able to traverse update/supersession paths and prefer the
current fact when the path proves it.

## Non-Goals

The kernel should not hard-code:

- LongMemEval logic;
- PIR logic;
- incident business rules;
- app-specific schemas as mandatory global schema.

Applications map their domain into kernel memory. The kernel provides ingest,
wake, ask, goto, near, rewind, forward, trace, inspect, and provenance.

## Decision

The kernel remains domain-agnostic, but becomes traversal-aware.

Events and gRPC remain valid transports.

Kernel Memory Protocol becomes the product surface. MCP, gRPC, and NATS become
bindings for the same memory moves across Underpass.

The first public step is now narrow and verifiable: Kernel 1.0 exposes memory
moves as a simple protocol through typed gRPC and MCP. The next product proof is
to measure the benchmark again against that capability. Do not claim the
benchmark is solved before measurement.

The principle to implement:

```text
Any memory recorded by the kernel must be recoverable as wake state, answer
proof, trace path, and raw inspection scope through one protocol.
```
