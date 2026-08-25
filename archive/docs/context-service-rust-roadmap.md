# Context Service Rust Roadmap

> **Historical (superseded).** Point-in-time record; code paths/links below were refactored after this was written.

> Review required. This roadmap was written for the compatibility-era migration
> plan. Parts of it are historical after the `v1beta1` cut and removal of the
> compatibility layer. Revalidate every action against the current kernel
> contract before reusing it.

Status: Superseded — archived roadmap
Source of truth: [`CONTEXT_SERVICE_RUST_MIGRATION_PLAN.md`](./CONTEXT_SERVICE_RUST_MIGRATION_PLAN.md)

## Purpose

Turn the migration RFC into an execution roadmap for this repo.

This roadmap keeps two rules explicit:

1. the internal core stays node-centric
2. compatibility with the existing Context Service is implemented at the edge

External legacy nouns appear in this roadmap only when naming the frozen
boundary contract.

## Non-Negotiable Constraints

- no god objects
- no god files
- DDD first
- hexagonal boundaries
- one main concept per file
- one use case per file
- one command or query entry point per file
- legacy compatibility only at adapters and transport edges
- the core language remains:
  - root node
  - neighbor nodes
  - relationships
  - extended node detail in Valkey

## Current Baseline

`main` already provides the substrate needed before starting compatibility work:

- graph-native bundle domain
- focused application use cases
- split Neo4j adapter
- split Valkey adapter
- split NATS adapter
- split gRPC transport
- external `fleet.context.v1` proto package
- compatibility shell at the gRPC edge
- explicit request, response, and status mapping modules
- container-backed compatibility integration tests
- frozen node-centric kernel contract
- contract CI with `buf breaking`, naming policy, and AsyncAPI checks
- reference ProtoJSON and async event fixtures backed by contract tests
- agentic integration e2e with a runtime-driven basic agent
- event-driven agentic trigger e2e from `context.bundle.generated`
- runtime integration reference for external consumers
- runnable runtime reference client outside tests
- green repo quality gates

That means the next milestone inside this repo is not another core refactor.
The kernel-side milestone is freezing, validating, and exemplifying the
node-centric boundary so external products can adapt to it safely.

## Deferred Kernel Maintenance Milestone

Status: `planned, not started`

Title:

- integration harness consolidation and CI runtime reduction

Why it exists:

- the container-backed CI is reliable enough for release work, but still too
  expensive and too duplicated to treat as a long-term steady state
- `compatibility_integration` in particular is functionally healthy but too
  slow for a PR-gated suite

Scope:

- consolidate duplicated testcontainers helpers for Neo4j, Valkey, and NATS
- reduce repeated fixture startup across container-backed integration targets
- separate smoke integration from heavier full integration where it improves
  feedback time without weakening release confidence
- keep explicit time budgets and failure messages for infrastructure readiness

Non-goals:

- no product-domain changes
- no compatibility-surface changes
- no broad test rewrite unless it directly reduces runtime or flakiness

Exit gate:

- integration helpers are shared instead of forked
- the slowest PR-gated integration target has a materially lower runtime
- container-backed failures surface with deterministic timeout messages

## Strategic LLM Milestone

Status: `planned`

Title:

- LLM response determinism and reasoning-safe interaction model

Why it exists:

- exploratory integration work proved that provider behavior can drift even
  when the transport request is nominally "JSON"
- the future integrating product cannot rely on markdown-fence parsing and raw
  `message.content`
- this repo needs a state-of-the-art interaction contract for reasoning models
  before more agentic surface area is added

Scope:

- provider-neutral response envelope
- schema-first contract registry for agentic tasks
- tool-first interaction where providers support it
- reasoning-safe consumption model
- explicit validation and repair pipeline
- malformed-response evaluation corpus

Reference:

- [`llm-response-determinism-strategy.md`](migration/llm-response-determinism-strategy.md)

## Strategic Security Milestone

Status: `mostly_complete`

Title:

- Transport Security v1

Why it exists:

- the kernel is already strong in contract, CI, packaging, and deployment, but
  still needed first-class transport hardening for standalone deployment
- inbound gRPC TLS and mTLS are now delivered
- outbound NATS TLS and outbound Valkey TLS are now delivered
- Neo4j custom CA wiring in Helm is now delivered
- the remaining transport gap is any future Neo4j client identity plus admin
  hardening if those become necessary

Reference:

- [`transport-security-v1.md`](./transport-security-v1.md)

## Direction Update

From this point forward, `swe-ai-fleet`-specific compatibility should move out
of this repo.

This repo should not keep expanding with:

- `planning.*`
- `orchestration.*`
- `story`
- `task`
- `project`
- other fleet-specific nouns

Those concerns belong in an anti-corruption layer inside `swe-ai-fleet`.

See:

- [`kernel-node-centric-integration-contract.md`](migration/kernel-node-centric-integration-contract.md)
- [`swe-ai-fleet-node-centric-integration-strategy.md`](./swe-ai-fleet-node-centric-integration-strategy.md)
- [`swe-ai-fleet-integration-checklist.md`](./swe-ai-fleet-integration-checklist.md)

## Status by Stream

### Stream A: Internal node-centric core

Status: `done for foundation`

Delivered:

- graph-native domain
- focused application layer
- modular adapters
- modular transport
- quality gates and tests

This stream should reopen only if a compatibility slice exposes a real core
gap.

### Stream B: External contract compatibility

Status: `phase 0 complete, phase 1 complete, phase 2 complete, phase 3 complete for kernel-owned boundary`

Frozen in Phase 0:

- external gRPC inventory
- external NATS subjects
- EventEnvelope rules
- config and startup behavior
- compatibility matrix
- golden test catalog
- reuse boundary

Delivered in Phase 1:

- compatibility proto generation for `fleet.context.v1`
- compatibility transport facade
- focused RPC modules
- focused request mapping modules
- focused response mapping modules
- boundary status mapping
- container-backed integration coverage for the implemented compatibility flows

Delivered after Phase 3 hardening:

- frozen node-centric kernel integration contract
- contract gate with `buf lint`, `buf breaking`, and boundary naming policy
- AsyncAPI contract checks for generic kernel-owned subjects
- reference fixtures for ProtoJSON requests or responses and async events
- conformance tests that prove the fixtures remain aligned with the contract
- runtime-oriented end-to-end proof that a generic agent can consume the kernel
  and drive tool execution through a runtime contract
- event-driven end-to-end proof that `context.bundle.generated` can trigger the
  same generic agent flow
- runtime integration reference spec and example payloads for external consumers

See:

- [`kernel-agentic-integration-e2e.md`](migration/kernel-agentic-integration-e2e.md)
- [`kernel-agentic-event-trigger-e2e.md`](migration/kernel-agentic-event-trigger-e2e.md)
- [`kernel-runtime-integration-reference.md`](migration/kernel-runtime-integration-reference.md)

### Stream C: Rollout and shadow mode

Status: `specified, implementation external`

Specified here:

- shadow comparison model
- rollout gates
- rollback expectations

Missing in an integrating product:

- dual-run strategy
- shadow comparison harness
- canary and rollback procedure

## Phase Roadmap

### Phase 0: Contract Freeze

Status: `complete`

Goal:

- freeze the observable contract of the existing Context Service before further
  implementation work

Outputs:

- [`context-service-phase0-contract-freeze.md`](./context-service-phase0-contract-freeze.md)
- [`context-service-compatibility-matrix.md`](./context-service-compatibility-matrix.md)
- [`context-service-golden-tests.md`](./context-service-golden-tests.md)
- [`rehydration-kernel-reuse-boundary.md`](./rehydration-kernel-reuse-boundary.md)

Exit gate: complete

### Phase 1: Compatibility Shell

Status: `complete`

Goal:

- expose `fleet.context.v1` in Rust without leaking external legacy nouns into
  the node-centric core

Deliverables:

- compatibility proto package and transport module
- one `ContextService` boundary facade
- focused request DTO files
- focused response DTO files
- explicit request mappers
- explicit response mappers
- compatibility status and error mapping
- compatibility bootstrap for external config defaults

Structural rules:

- separate files for DTOs, request mapping, response mapping, and status mapping
- no transport god service
- no compatibility mapper file that owns unrelated RPCs

Exit gate:

- the Rust service boots with the external package and service identity
- boundary error mapping is proven by tests
- the shell routes requests into the current node-centric application layer

Delivered:

- `fleet.context.v1` proto package and generation flow
- compatibility gRPC facade
- focused request mapping modules
- focused response mapping modules
- status mapping for implemented and placeholder RPCs
- transport tests for compatibility routing
- container-backed compatibility integration tests for:
  - `GetContext`
  - `GetGraphRelationships`
  - `RehydrateSession`

### Phase 2: Read-Path Parity

Status: `complete`

Goal:

- serve the external read API from the node-centric core

Scope:

- `GetContext`
- `RehydrateSession`
- `ValidateScope`
- `GetGraphRelationships`

Current implementation state:

- `GetContext`: routed and covered
- `RehydrateSession`: routed and covered
- `GetGraphRelationships`: routed and covered
- `ValidateScope`: routed and covered
- read-path golden tests: implemented
- DTO parity audit: completed
- `GetContext.phase`: mapped to compatibility scope expectations
- `GetContext.subtask_id`: mapped to node-centric focus rendering
- `GetContext.token_budget`: enforced as render budget hint
- `RehydrateSession.ttl_seconds`: defaulted and propagated to snapshot persistence

Deliverables:

- request mapping from external identifiers into node-centric workflows
- response rendering from graph-native outputs into external DTOs
- depth clamp and validation parity for `GetGraphRelationships`
- scope validation parity
- golden tests for all read RPCs

Exit gate:

- read-path golden tests pass
- external DTOs match the frozen contract
- no core module adopts external legacy nouns

### Phase 3: Async NATS Parity

Status: `complete for kernel-owned boundary`

Goal:

- implement the async request or reply and event flows required by the existing
  service

Scope:

- `context.update.request`
- `context.rehydrate.request`
- `context.update.response`
- `context.rehydrate.response`
- `context.events.updated`

Deliverables:

- subject routers per public flow
- EventEnvelope parsing and validation
- request or reply correlation
- `ack` or `nak` parity
- publish envelope parity

Current implementation state:

- `context.update.request`: compatibility consumer implemented
- `context.rehydrate.request`: compatibility consumer implemented
- `context.update.response`: envelope publication implemented
- `context.rehydrate.response`: envelope publication implemented
- `context.events.updated`: publisher and frozen envelope publication implemented
- compatibility JetStream runtime wiring implemented for request consumers and publication sink
- compatibility NATS config implemented with `NATS_URL` default and `ENABLE_NATS` fail-fast behavior
- required `EventEnvelope` parsing and validation implemented at the NATS edge
- golden tests cover:
  - valid request -> publish reply + `ack`
  - invalid JSON -> `ack` and drop
  - invalid envelope -> `ack` and drop
  - non-object payload -> `ack` and drop
  - post-parse service failure -> `nak`
  - runtime JetStream request -> response over a real NATS container
  - runtime `context.updated` publication over a real NATS container

Remaining work previously associated with fleet-specific subjects is now
externalized to `swe-ai-fleet`.

Exit gate:

- async golden tests pass
- envelope behavior matches the baseline
- public subjects are implemented without turning the NATS adapter into a god file

Status note: complete for this repo

Delivered:

- request or reply parity for kernel-owned async subjects
- real JetStream runtime wiring
- container-backed runtime integration coverage
- `context.events.updated` publication support at the kernel edge

### Phase 4: swe-ai-fleet Integration Adapter

Status: `pending and external to this repo`

Goal:

- adapt `swe-ai-fleet` legacy concepts to the node-centric kernel contract

Scope:

- fleet-side request mapping
- fleet-side response mapping
- fleet-side async bridge
- fleet-side subject translation
- fleet-side shadow comparison

Deliverables:

- anti-corruption layer in `swe-ai-fleet`
- mapping matrix from legacy nouns to node-centric structures
- shadow comparison harness in `swe-ai-fleet`
- fleet-owned rollout switches

Exit gate:

- no fleet-specific nouns are added to kernel domain or transport
- `swe-ai-fleet` can call the kernel through a narrow adapter
- legacy-to-node-centric drift is measurable outside this repo

### Phase 5: Rollout

Status: `specified, implementation external`

Goal:

- cut over safely from the Python service to the Rust implementation

Deliverables:

- shadow comparison harness
- canary procedure
- rollback procedure
- cutover checklist

Exit gate:

- shadow comparisons are within accepted drift
- rollback is exercised
- production cutover checklist is signed off

Supporting docs:

- [`kernel-repo-closeout.md`](./kernel-repo-closeout.md)
- [`swe-ai-fleet-shadow-mode-spec.md`](./swe-ai-fleet-shadow-mode-spec.md)

## Immediate Next Slice

Do not add `planning.*` or `orchestration.*` consumers here.

The kernel-side contract hardening milestone now provides:

1. a frozen node-centric integration contract for external consumers
2. a strategic integration document for `swe-ai-fleet`
3. a migration checklist for moving legacy compatibility out of this repo
4. contract CI that protects the generic boundary
5. reference fixtures that external consumers can adopt directly
6. a runnable runtime reference client outside the test harness
7. an event-driven trigger proof from `context.bundle.generated`

The next implementation cut that still belongs in this repo should be optional
kernel developer experience work, not more fleet-specific compatibility.

Artifacts now available:

- [`kernel-node-centric-integration-contract.md`](migration/kernel-node-centric-integration-contract.md)
- [`kernel-repo-closeout.md`](./kernel-repo-closeout.md)
- [`swe-ai-fleet-node-centric-integration-strategy.md`](./swe-ai-fleet-node-centric-integration-strategy.md)
- [`swe-ai-fleet-shadow-mode-spec.md`](./swe-ai-fleet-shadow-mode-spec.md)
- [`swe-ai-fleet-integration-checklist.md`](./swe-ai-fleet-integration-checklist.md)
- [`kernel-agentic-event-trigger-e2e.md`](migration/kernel-agentic-event-trigger-e2e.md)
- [`kernel-runtime-integration-reference.md`](migration/kernel-runtime-integration-reference.md)
