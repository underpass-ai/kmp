# Queryable Agentic Memory Layer

Date: 2026-05-06
Status: product direction

## Thesis

KMP should expose temporal, multidimensional memory as a queryable
persistence layer for agentic processes.

The product is not a graph database API, a vector database API, or a chat QA
engine. The product is a memory navigation API that lets humans and LLMs ask:

```text
What was known?
Where was it known?
When was it known?
Why was a decision made?
Which evidence supported it?
Which paths failed?
Which path succeeded?
Which path was fastest, safest, cheapest, or best supported?
```

Infrastructure backends are replaceable implementations. The stable surface is
the memory model, temporal and dimensional traversal semantics, evidence/proof
contract, and Kernel Memory Protocol API.

## Non-Negotiables

The memory layer must be:

| Property | Meaning |
| --- | --- |
| Observable | Operators can see requests, event flow, projection lag, traversal scope, proof quality, and agent-facing outcomes. |
| Auditable | Humans can reconstruct what was known, what was decided, why it was decided, and which evidence supported it at that time. |
| Secure | Memory access, transport, secrets, sensitive details, and audit logs are protected by explicit boundaries and fail closed when trust is missing. |

These properties are product requirements, not deployment extras.

## Product Promise

If an agent or application records memory with dimensions, time, relations,
evidence, and provenance, the kernel should later make that process searchable,
navigable, replayable, and auditable.

This is stronger than "retrieve context". The kernel should let a caller
inspect a process:

- search memory across one or many dimensions;
- inspect what was known at a concrete time, sequence, or memory ref;
- identify which decision was taken at a point in the process;
- explain why that decision was taken;
- show which evidence was available then;
- find failed paths and wrong decisions;
- find the final successful path;
- compare paths by speed, risk, cost, confidence, or evidence quality;
- replay the resolution from symptom to outcome;
- audit whether a decision was justified with the memory available at the time.

Observability is part of this promise. A queryable memory layer is not useful
for agents if operators cannot see what was recorded, when projections caught
up, why a proof was weak, or where a traversal spent time.

Auditability is also part of the promise. If a decision cannot be traced back
to stable refs, evidence, temporal state, and provenance, the kernel should not
present it as proven.

Security is part of the promise as well. The kernel must be useful to LLMs and
humans without exposing uncontrolled memory scopes, secrets, credentials, or raw
details to callers that should not see them.

## Mental Model

The caller should not need to learn the storage topology.

The user-facing model is:

```text
about       -> the memory world or aggregate being inspected
dimensions  -> planes of memory: agent, session, workflow, incident, entity, source
entries     -> observations, actions, decisions, tool calls, states, evidence
relations   -> support, causality, contradiction, update, dependency, sequence
time        -> timestamp, sequence, cursor, known-at-time view
proof       -> refs, evidence, relations, provenance, conflicts, missing data
```

The infrastructure model is hidden behind ports:

```text
application / agent / human client
        |
       KMP
        |
temporal multidimensional memory semantics
        |
ports
        |
adapters: in-memory, Neo4j, Valkey, NATS, Postgres, object store, ...
```

The same semantic query should behave the same way regardless of backend.

## Event Source And Observability

The event source is not only infrastructure. It is the audit spine of agentic
memory.

Every important memory change should be explainable from:

- the accepted ingest request;
- the event id or stream position;
- the idempotency key and outcome;
- the projection status;
- the resulting entries, relations, evidence, dimensions, and provenance;
- the query or trace that later consumed that memory.

The system should make these questions observable:

```text
Was this memory accepted?
Was it projected?
When did it become queryable?
Which query used it?
Which evidence was missing?
Which decision path did it support?
Did the agent receive enough proof or only a weak summary?
```

OTel, structured logs, and event-source metadata are therefore part of the
product surface for operators. They do not change memory semantics, but they
make the memory layer trustworthy.

### Audit Requirements

For every important answer, decision, or replay, the kernel should expose:

- stable refs used to build the response;
- evidence refs and evidence text or inspect handles;
- relation path and relation classes;
- temporal cursor or known-at-time boundary;
- selected dimensions and selected abouts;
- provenance for nodes, and later relationships;
- conflicts, missing evidence, and weak-proof warnings;
- raw inspect handles for authorized callers.

The audit trail should make a decision reproducible without reading raw prompts
or reverse engineering backend storage.

### Agentic Metrics

Metrics that matter for agents:

| Metric family | Why it matters |
| --- | --- |
| KMP request count, latency, and errors by move | Shows whether agents can reliably call `ingest`, `wake`, `ask`, `goto`, `trace`, and `inspect`. |
| Ingest accepted counts | Entries, relations, evidence, dimensions, and rejected items per request. |
| Projection lag | Consumer processing time (pull -> apply), not full publish-to-queryable latency. |
| Idempotency outcomes | Detects duplicate writes, replay behavior, and unsafe retries. |
| Traversal scope | Selected abouts, dimensions, temporal windows, path length, and hop count. |
| Proof quality | Evidence count, missing count, warning count, conflict count, weak-proof count. |
| Known-at-time correctness | Whether queries were bounded to the memory available at the requested point. |
| Replay completeness | Failed attempts, successful terminal states, and final path coverage. |
| Context quality | Raw-equivalent tokens, rendered tokens, compression ratio, causal density, noise ratio, detail coverage. |
| Consumer outcome | Unknown rate, missing-evidence rate, answer-with-proof rate, trace-used rate. |
| Security and audit | Auth mode, rejected requests, denied scopes, redaction counts, inspect/raw access counts. |

Some of these exist today as bundle quality metrics and KMP structured logs.
Others are backlog and should be added before making strong public claims about
agentic memory operations.

### Trace Shape

For an agentic query, the ideal trace spans:

```text
client/tool call
  -> KMP boundary
  -> event append or read request
  -> projection/read-model status
  -> traversal and render
  -> proof construction
  -> response returned to agent or human
```

For benchmark and demo runs that include an LLM consumer, the trace should
continue:

```text
KMP response
  -> LLM prompt/context
  -> model call
  -> model answer/action
  -> optional write-back into memory
```

This makes it possible to debug whether a failure came from ingestion,
projection lag, retrieval scope, weak proof, prompt consumption, model
reasoning, or downstream action execution.

## Easy For LLMs And Humans

KMP must be easy to use from both LLM tools and human-facing APIs.

Design rules:

- use a small set of stable memory moves;
- keep request JSON simple and explicit;
- provide safe defaults;
- fail fast on ambiguous scope or invalid coordinates;
- return compact summaries for immediate use;
- return proof handles for audit;
- make refs stable and readable enough for humans;
- avoid hidden fallback behavior;
- say `unknown` or `missing` when evidence is insufficient;
- keep the same mental model across gRPC, MCP, future HTTP, and SDKs.

Every response should have two layers:

| Layer | Purpose |
| --- | --- |
| Usable layer | `summary`, `answer`, `next`, or compact state for the caller. |
| Proof layer | refs, evidence, relations, provenance, conflicts, missing data, and inspect handles. |

Rule:

```text
Every KMP response must be directly usable by an LLM and directly inspectable by a human.
```

## Core Memory Moves

The core API remains small:

| Move | Meaning |
| --- | --- |
| `ingest` | Record memory with entries, dimensions, relations, evidence, time, and provenance. |
| `wake` | Return the compact state needed to continue work. |
| `ask` | Recover deterministic evidence and answer text from memory, or say what is missing. |
| `goto` | Jump to a time, sequence, or memory ref across selected dimensions. |
| `near` | Inspect the temporal neighborhood around a time, sequence, or ref. |
| `rewind` | Move backward through memory across selected dimensions. |
| `forward` | Move forward through memory across selected dimensions. |
| `trace` | Explain a relationship, causal, evidential, or decision path. |
| `inspect` | Show typed detail, direct links, evidence, and raw audit refs. |

These moves are persistence-layer primitives. Higher-level product questions can
be implemented as composed queries over them.

## Process Queries

The following are the target query shapes for agentic memory. Some can already
be expressed with current KMP moves; others should live as composed query
helpers above the core API.

| Query | Meaning | Current primitive shape |
| --- | --- | --- |
| `search_memory(scope, query)` | Find relevant memory across dimensions. | `ask` plus evidence projection, later reranker/search helper. |
| `known_at(time/ref, scope)` | Show what was known at a point in time. | `goto` plus `inspect`. |
| `decision_at(time/ref, scope)` | Find the decision active at a point in the process. | `goto` plus relation/evidence filtering. |
| `why(decision_ref)` | Explain why a decision was taken. | `trace` plus `inspect`. |
| `evidence_for(ref)` | Show evidence supporting an entry or decision. | `inspect` plus direct evidence links. |
| `failed_paths(scope)` | Find attempts that did not resolve the goal. | composed query over relations and temporal order. |
| `successful_paths(scope)` | Find paths that reached a verified outcome. | composed query over relations and terminal states. |
| `final_path(scope)` | Reconstruct the path from first symptom to final fix. | `trace`, `goto`, `forward`, `inspect`. |
| `compare_paths(criteria)` | Compare paths by speed, cost, risk, confidence, or evidence. | plugin/query helper above KMP. |
| `best_path(criteria)` | Select the best path under explicit criteria. | plugin/query helper above KMP. |

Important boundary: `best_path` and `compare_paths` are process-analysis
queries, not storage magic. The kernel should provide the temporal and
evidential substrate. A policy/plugin layer should define what "best" means for
a domain.

## SOTA Techniques Worth Incorporating

The goal is not to copy every agent-memory product. The goal is to adopt the
techniques that improve queryable, temporal, auditable process memory while
keeping the kernel infrastructure-independent and domain-agnostic.

### P0: Hybrid Candidate Retrieval

Worth it.

Use several cheap retrieval signals before asking any LLM to reason:

- lexical search for exact names, ids, dates, amounts, and tool outputs;
- embedding search for semantic recall;
- graph traversal for related entries and paths;
- temporal expansion around candidate refs;
- cross-encoder or reranker for final candidate ordering.

Why it rents:

- our LongMemEval probe already showed retrieval-side embeddings recover almost
  all gold evidence before the reader fails;
- Graphiti/Zep, Graphonomous, Hindsight, and Supermemory all point toward
  hybrid retrieval rather than pure vector search;
- this improves recall without putting benchmark operators in core.

Boundary:

- core should expose refs, dimensions, time, relations, and proof;
- retrieval scoring should live behind an inference/search port or sidecar;
- KMP must not become a vector database API.

### P0: Temporal Validity And Supersession

Worth it.

Facts and decisions need validity intervals and update semantics:

- `valid_from`;
- `valid_until`;
- supersedes / superseded-by;
- active fact at time `t`;
- stale or contradicted fact warnings;
- known-at-time response boundaries.

Why it rents:

- it is central to temporal memory, not a benchmark hack;
- Zep/Graphiti explicitly compete on temporal context graphs and invalidating
  outdated facts;
- our product promise depends on answering "what was known then?" and "why was
  this decision reasonable at that time?".

Boundary:

- core owns temporal coordinates and proof;
- applications own the truth of supplied facts;
- conflict resolution beyond explicit evidence remains a higher-level policy.

### P0: End-To-End Agent Traceability

Worth it.

Adopt OpenTelemetry/OpenInference-style spans for agentic memory flows:

```text
agent/tool call -> KMP -> event append/read -> projection -> traversal -> proof -> LLM/tool consumer -> optional write-back
```

Why it rents:

- LangSmith, Phoenix, and OTel GenAI conventions show this is becoming the
  operational baseline for serious agent systems;
- without traces, memory failures are hard to classify: ingestion, projection,
  retrieval, proof, prompt consumption, model reasoning, or action execution;
- it strengthens observable/auditable/secure as product requirements.

Boundary:

- OTel, Loki, Grafana, Phoenix, or LangSmith are adapters;
- the stable kernel contract is which memory events, proof metrics, and
  traversal attributes are emitted.

### P0: Agentic Retrieval Mode

Worth it.

Support a mode where the caller, often an LLM agent, can perform multiple memory
moves before answering:

- `wake`;
- `search_memory`;
- `goto`;
- `trace`;
- `inspect`;
- `near`;
- `ask` only after evidence is sufficient.

Why it rents:

- Hindsight's benchmark framing separates single-query and agentic retrieval;
- complex process questions often need more than one retrieval call;
- this fits KMP naturally because the API is already a set of memory moves.

Boundary:

- core exposes deterministic moves;
- the agent or application decides when it has enough evidence;
- no hidden verifier/repair loop inside core.

### P1: Observation And Reflection Layer

Worth it, but not in core first.

Mastra's Observational Memory shows the value of a stable, append-only
observation log plus periodic reflection/consolidation.

Use this as a plugin/application layer:

- background observer turns raw events into dense observations;
- reflector condenses or restructures old observations;
- generated observations are written back as derived memory with provenance;
- raw memory remains inspectable.

Why it rents:

- reduces prompt bloat;
- gives stable context for agents;
- complements event sourcing instead of replacing it.

Boundary:

- observations are derived memory, not truth;
- every observation must keep `derived_from` refs;
- core must preserve raw auditability.

### P1: Checkpoint, Replay, And Fork Semantics

Worth it, but scoped.

LangGraph and Temporal prove the value of checkpointing, replay, durable state,
and time-travel debugging.

For KMP, this should mean:

- inspect memory at checkpoint/ref/time;
- replay memory context, not necessarily re-execute external side effects;
- fork a derived analysis path without mutating historical truth;
- compare paths that share a common ancestor.

Why it rents:

- directly supports wrong-path analysis and final-path reconstruction;
- aligns with agentic incident/process demos;
- avoids becoming a workflow orchestrator.

Boundary:

- Temporal owns durable execution;
- LangGraph owns graph runtime checkpoints;
- Kernel owns queryable process memory and audit state.

### P1: Static/Dynamic Memory Profiles

Worth it.

Supermemory and Letta both separate durable user/profile context from dynamic
episodic memory. We need the same concept, but generalized:

- static facts;
- dynamic facts;
- episodic observations;
- procedural patterns;
- derived summaries;
- current task state.

Why it rents:

- improves retrieval salience;
- helps agents resume with the right layer of memory;
- works across chat, incidents, workflows, and benchmarks.

Boundary:

- these are memory classes or dimensions, not hard-coded app schemas;
- applications can define domain-specific classes above the kernel.

### P1: Benchmark Dimensions Beyond Accuracy

Worth it.

Hindsight's AMB framing is correct: accuracy alone is not enough. Track:

- accuracy or task success;
- latency;
- cost/tokens;
- setup complexity;
- replay completeness;
- proof quality;
- observability coverage;
- safety/audit failures.

Why it rents:

- our product is not only "gets answer right";
- it is "can agents and humans trust, inspect, and operate the memory layer?".

## SOTA Techniques Not To Pull Into Core Now

Do not put these in kernel core:

- aggregate QA operators such as `sum`, `count`, `max_by`, money, and date math;
- generic LLM verifier/repair as a hidden second pass;
- vector database semantics as public KMP;
- application-specific entity ontologies as mandatory global schema;
- workflow orchestration or side-effect replay;
- closed benchmark-specific prompts or scoring logic.

These may exist as plugins, adapters, benchmarks, or application policies, but
the kernel core should stay focused on temporal multidimensional memory,
deterministic traversal, proof, trace, inspect, and event-source auditability.

## Example: Incident Replay

For a multi-agent incident, a caller should be able to ask:

```text
What did the coordinator know at 10:12?
Why was rollback selected?
Which mitigation attempts failed before rollback?
Which evidence connected the iOS failures to jwt-parser-v3?
What was the final successful path?
Was rollback the fastest path with evidence available at that time?
```

These should map to memory navigation:

```text
goto(10:12, dimensions=[coordinator, mobile, auth, release])
trace(decision:rollback)
ask("failed attempts before rollback")
trace(symptom:ios-login-failure -> outcome:recovered)
inspect(attempt:scale-auth-api)
compare_paths(criteria=speed, scope=incident)
```

The output should be readable without raw prompt archaeology:

- short summary;
- ordered timeline;
- included dimensions;
- evidence refs;
- failed attempts;
- final path;
- missing or weak proof;
- inspect handles for raw detail.

## Boundaries

### Not Core

The core must not own:

- business-specific definitions of "best", "safe", "cheap", or "fast";
- aggregate QA operators such as `sum`, `count`, `max_by`, or money/date logic;
- application schemas;
- benchmark scoring rules;
- generated final-answer reasoning.

Those belong in plugins, applications, or readers above the kernel.

The core also must not silently bypass security policy. If a caller lacks scope,
identity, or permission to inspect raw memory, the kernel should fail closed or
return a redacted response with explicit warnings.

### Core

The core should own:

- stable memory refs;
- `about` scoping;
- dimension identities and selection;
- temporal coordinates;
- deterministic traversal;
- relation and evidence storage;
- proof surfaces;
- trace and inspect behavior;
- known-at-time semantics.

Security-sensitive capabilities should be explicit in the API contract:

- caller identity and scope, once application identity is introduced;
- raw inspect permission;
- cross-about permission;
- redaction policy;
- audit log emission;
- fail-fast behavior for ambiguous or unauthorized scope.

## Medium-Term Direction

The medium-term goal is infrastructure independence:

- define a conformance suite for temporal multidimensional memory semantics;
- make the same KMP behavior pass against in-memory and production adapters;
- keep storage-specific behavior behind ports;
- treat Neo4j, NATS, Valkey, Kubernetes, MCP, and gRPC as replaceable bindings
  or distributions, not as the product itself;
- allow future adapters without changing the user-facing memory model.

The public formulation should be:

```text
KMP is not Neo4j plus NATS plus Valkey.
That is one distribution.
KMP is the memory model, traversal semantics, proof contract,
ports, and Kernel Memory Protocol.
```

Observability must follow the same rule. OTel, Loki, Grafana, event streams,
and dashboards are replaceable adapters. The stable contract is what the kernel
reports about memory acceptance, projection, traversal, proof quality, and
agent consumption.

## Next Product Cut

The next product proof should not be another flat chat QA benchmark. It should
show a real agentic process:

1. multiple agents or sessions record observations, attempts, decisions, and
   evidence;
2. the kernel exposes what was known at each point;
3. the caller can inspect wrong paths and successful paths;
4. the final decision can be traced back to evidence available at the time;
5. a human can navigate the same process visually or through the API;
6. an LLM can use the same API without hidden infrastructure knowledge.
7. operators can observe event-source health, projection lag, traversal scope,
   proof quality, and agent-facing response quality through OTel/logs.

MemoryArena is the best external benchmark candidate for this direction.
