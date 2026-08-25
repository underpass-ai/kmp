# swe-ai-fleet Node-Centric Integration Strategy

Status: proposed
Scope: `swe-ai-fleet` compatibility plan, not core kernel expansion

## Intent

Keep `rehydration-kernel` generic and node-centric.

Do not keep expanding this repo with `swe-ai-fleet` legacy nouns such as:

- `planning.*`
- `orchestration.*`
- `story`
- `task`
- `project`
- `epic`

The compatibility burden moves to `swe-ai-fleet`.

That means the migration strategy is inverted:

- `rehydration-kernel` exposes a node-centric context service
- `swe-ai-fleet` adapts its own legacy domain into that node-centric contract

## Strategic Decision

The kernel must remain reusable across different agentic systems.

If this repo keeps absorbing `swe-ai-fleet` transport subjects and domain nouns,
it stops being a generic context engine and becomes a fleet-specific service.

The safer design is an anti-corruption layer in `swe-ai-fleet`:

- legacy nouns are translated at the edge
- the kernel only speaks `node`, `relationship`, `detail`, `root_node_id`
- rollout risk is isolated in the integrating product, not in the kernel core

## Integration Thesis

This microservice should be treated as a context kernel, not as a backlog or
workflow service.

Its value proposition for any agentic product should be:

- graph-native context retrieval
- durable extended context per node
- bounded rendering for LLM consumption
- replayable snapshots
- transport and observability that do not assume one product domain

If an integrating product must first rename its whole domain to match the
kernel, then the kernel is not truly reusable.

## Target Integration Architecture

### Inside `rehydration-kernel`

Keep only generic concepts:

- root node
- neighbor nodes
- relationships
- node details
- snapshotting
- rendering
- generic async/publication behavior

No new `swe-ai-fleet` nouns should enter:

- domain
- application
- adapters
- transport contracts that are meant to stay kernel-owned

### Inside `swe-ai-fleet`

Add a dedicated integration layer with small modules:

- `context_kernel_client`
- `legacy_to_node_mapping`
- `node_to_legacy_mapping`
- `async_subject_bridge`
- `shadow_comparison`
- `cutover_routing`

That layer owns all translation between:

- `story/task/decision/...`
- `node/relationship/detail`

## Recommended Integration Shape In swe-ai-fleet

### 1. Read path adapter

Translate legacy read requests into node-centric requests:

- pick `root_node_id`
- derive role
- derive focus node where needed
- derive render budget

Translate kernel responses back only as long as legacy consumers still need them.

### 2. Write path adapter

Translate legacy mutations into generic node-centric mutations:

- node detail updates
- relationship changes
- graph mutations that can be represented generically

Do not reintroduce legacy write DTOs into the kernel.

### 3. Async bridge

`planning.*` and `orchestration.*` stay in `swe-ai-fleet`.

Their handlers should:

- consume fleet-native events
- map them to generic kernel commands or publications
- publish or call the kernel through a narrow adapter

This keeps the kernel free of fleet-specific subjects.

### 4. Shadow mode

Before cutover, `swe-ai-fleet` should call:

- legacy context service
- `rehydration-kernel`

Then compare normalized outputs inside `swe-ai-fleet`.

### 5. Cutover

Use feature flags or routing switches in `swe-ai-fleet` to:

- enable read shadowing
- enable async shadowing
- switch live traffic
- rollback quickly

## Integration Questions For Different Agentic Project Types

The kernel should be tested mentally against several product shapes, not only
`swe-ai-fleet`.

### Coding and workspace agents

Questions:

- Can a workspace, file, module, task, or incident all be modeled as nodes
  without code changes in the kernel?
- Can code intelligence products store large per-node context in Valkey without
  forcing a code-specific schema into the kernel?
- Can the render budget prioritize local relevance such as the focused file,
  failing test, or active issue?

### Research and knowledge agents

Questions:

- Can sources, claims, notes, evidence, and conclusions all live as generic
  nodes and relationships?
- Can the kernel rehydrate context around a claim node without assuming a task
  tree or planning hierarchy?
- Can products attach provenance-rich node detail without the kernel needing to
  understand citation semantics?

### Multi-agent orchestration systems

Questions:

- Can agents, goals, tools, plans, and outcomes all be represented generically?
- Is the async boundary generic enough that another system can publish its own
  domain events and still drive kernel updates through a narrow adapter?
- Can the kernel produce deterministic enough read models for agent handoff and
  replay?

### Customer support or operations agents

Questions:

- Can tickets, accounts, incidents, services, and playbooks all be represented
  as graph nodes and details?
- Can the kernel work with strict tenancy boundaries and auditable snapshot
  keys?
- Can the rendered context be bounded and explainable enough for human review?

### Cross-project questions

- Does integration require only graph mapping, or does it require product
  semantics to leak into the kernel?
- Can the product keep its own write model and only treat the kernel as a
  context engine?
- Can the product adopt the kernel incrementally through shadow reads and
  partial cutover?

## Questions We Should Ask Before Calling The Kernel Reusable

### Domain neutrality

- Can a client integrate using only `root_node_id`, `nodes`, `relationships`, and `details`?
- Does any public kernel path still assume `swe-ai-fleet` nouns?
- Are role and scope semantics generic enough for other agentic products?

### Graph model flexibility

- Can different products define their own node kinds without kernel code changes?
- Can different products define their own relationship kinds without kernel code changes?
- Is there any hidden assumption that all graphs resemble backlog planning hierarchies?

### Detail storage and retrieval

- Is extended context in Valkey represented as generic node detail, not as domain-specific payload?
- Can another product enrich node detail without matching `swe-ai-fleet` schemas?
- Do we need explicit versioning rules for node detail payloads?

### Transport and protocol

- Is the gRPC surface stable enough to be the primary integration contract?
- Which async subjects are truly kernel-owned and generic?
- Should `swe-ai-fleet` integrate first via gRPC only, and keep async bridging in its own code until generic public subjects are frozen?

### Rendering and budget control

- Are focus selection and token budgets product-agnostic?
- Can another agentic product request different render formats without adding domain adapters into the kernel?
- Is the rendered output deterministic enough for shadow comparison?

### Security and tenancy

- How does a caller express tenant, workspace, or environment boundaries?
- Are access-control checks expected upstream, or must the kernel enforce tenancy boundaries itself?
- Does snapshot storage need tenant-aware keying before broader adoption?

### Operability

- Can another system observe hydration quality, drift, token usage, and stale detail rates?
- Are the kernel errors diagnostic enough for external integrators?
- Do we have a contract for retries, idempotency, and replay safety?

## Kernel Readiness Criteria For Broad Adoption

Before calling this service broadly integrable, we should be able to answer
`yes` to these statements:

- A new product can integrate by mapping its own entities to `node`,
  `relationship`, and `detail` without changing kernel code.
- The kernel public contract does not expose `swe-ai-fleet` nouns.
- Extended node detail can carry product-specific structure without creating a
  product-specific schema dependency in the kernel.
- The transport surface is small enough that another team can build an adapter
  in a few focused modules.
- Snapshotting, rendering, and observability are useful even when the caller is
  not a planning system.
- A product can keep its own orchestration events and bridge them at the edge.

If any of these answers is `no`, then the next work should be contract
hardening, not more product-specific integration inside this repo.

## Proposed Migration Plan For swe-ai-fleet

### Phase A: Kernel contract freeze

Freeze what `swe-ai-fleet` is allowed to depend on from this repo:

- canonical gRPC entrypoints
- canonical request and response DTOs
- generic async/publication surface
- error mapping

### Phase B: Anti-corruption layer

Implement in `swe-ai-fleet`:

- request mappers
- response mappers
- event mappers
- small clients and publishers

### Phase C: Read shadowing

Run legacy and node-centric reads side by side.

Compare:

- selected nodes
- rendered context
- budgets
- scope behavior

### Phase D: Async bridging

Consume `planning.*` and `orchestration.*` in `swe-ai-fleet`, then translate to
generic kernel operations.

Do not move those subjects into this repo.

### Phase E: Progressive cutover

Switch:

- read traffic first
- then async traffic
- then legacy service decommissioning

## Recommended Artifact Set For swe-ai-fleet

To make the migration operational, `swe-ai-fleet` should create and maintain:

- one integration RFC that freezes its adapter boundaries
- one mapping matrix from legacy nouns to `node`, `relationship`, and `detail`
- one shadow comparison spec for normalized output comparison
- one rollout checklist with flags, telemetry, and rollback conditions
- one drift dashboard for mismatched reads, publish failures, and stale detail
  rates

Seed artifact in this repo:

- [`swe-ai-fleet-integration-checklist.md`](./swe-ai-fleet-integration-checklist.md)

## What This Repo Should Do Next

This repo should now focus on:

- freezing the kernel-owned node-centric contract
- documenting integration expectations
- hardening generic transport and observability
- avoiding new fleet-specific adapters

This repo should not take on:

- `planning.*` consumers
- `orchestration.*` consumers
- new public DTOs with `swe-ai-fleet` nouns
- direct modeling of `story`, `task`, `project`, or similar legacy concepts

## Exit Criteria For The Strategy

We can treat the kernel as integration-ready when:

- a `swe-ai-fleet` adapter can consume it without changing kernel domain language
- all legacy mapping lives outside this repo
- shadow comparison is possible and deterministic enough
- rollout and rollback are owned by the integrating product
