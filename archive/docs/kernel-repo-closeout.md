# Kernel Repo Closeout

> **Historical (superseded).** Point-in-time record; some relative links below
> were repointed after stable artifacts moved to `docs/migration/`.

> Archived on 2026-04-09. This note is retained for traceability, but it is no
> longer a current source of truth for the kernel boundary or product write
> path.

> Review required. This closeout is still directionally useful, but it was
> written before the latest removal of compatibility surfaces and alpha-era
> assumptions. Re-check ownership and handoff statements before relying on it.

Status: complete for this repo
Scope: `rehydration-kernel`

## Purpose

Record what this repo now owns, what has been delivered, and what work is
explicitly handed off to integrating products.

This closeout exists so the repo does not drift back into:

- fleet-specific nouns
- compatibility sprawl
- rollout logic that belongs in the integrating product

## Final Position

`rehydration-kernel` is now positioned as a generic, node-centric context
engine.

Its owned responsibilities are:

- graph-native context storage and retrieval
- snapshotting
- context rendering
- generic gRPC and async boundaries
- contract validation
- integration examples and end-to-end proofs

Its non-responsibilities are:

- `swe-ai-fleet` domain modeling
- legacy noun preservation inside the kernel core
- `planning.*` and `orchestration.*` consumption
- product-side shadow comparison
- product-side rollout and rollback orchestration

## Delivered In This Repo

### Internal architecture

- graph-native domain model centered on:
  - root node
  - neighbor nodes
  - relationships
  - node details
- focused application use cases
- split Neo4j adapter
- split Valkey adapter
- split NATS adapter
- split gRPC transport
- no god objects introduced in the final architecture path

### External kernel boundary

- frozen node-centric contract
- gRPC transport surface for kernel-owned operations
- generic async subjects
- contract fixtures for ProtoJSON and async events
- contract CI with:
  - `buf lint`
  - `buf breaking`
  - naming policy checks
  - AsyncAPI checks

### Compatibility and migration support

- legacy compatibility shell needed to complete the migration slices already
  implemented here
- integration strategy for `swe-ai-fleet`
- integration checklist for `swe-ai-fleet`
- runtime integration reference
- runnable runtime reference client

### Proof of integration

- container-backed gRPC compatibility tests
- container-backed generic NATS tests
- full kernel journey E2E across projection, query, compatibility, command, and
  admin
- full TLS kernel journey E2E across gRPC, NATS, Valkey, and Neo4j in the test
  harness
- agentic end-to-end proof using runtime tool execution
- event-driven agentic end-to-end proof triggered from
  `context.bundle.generated`

## Public Artifacts To Treat As Stable

Consumers may depend on these artifacts as the frozen integration surface of
this repo:

- [`kernel-node-centric-integration-contract.md`](migration/kernel-node-centric-integration-contract.md)
- [`kernel-runtime-integration-reference.md`](migration/kernel-runtime-integration-reference.md)
- [`kernel-agentic-integration-e2e.md`](migration/kernel-agentic-integration-e2e.md)
- [`kernel-agentic-event-trigger-e2e.md`](migration/kernel-agentic-event-trigger-e2e.md)
- [`context-service-rust-roadmap.md`](./context-service-rust-roadmap.md)
- [`api/examples/README.md`](../../api/examples/README.md)
- [`api/examples/runtime-reference/v1/README.md`](../../api/examples/runtime-reference/v1/README.md)

## Explicit Handoff To `swe-ai-fleet`

The following work is no longer a kernel responsibility and must be executed in
`swe-ai-fleet`:

- anti-corruption layer from legacy nouns to:
  - node
  - relationship
  - detail
- request and response translation for legacy callers
- async translation from product-owned subjects to kernel-owned operations
- shadow comparison harness
- rollout flags
- canary process
- rollback process

Authoritative handoff docs:

- [`swe-ai-fleet-node-centric-integration-strategy.md`](./swe-ai-fleet-node-centric-integration-strategy.md)
- [`swe-ai-fleet-integration-checklist.md`](./swe-ai-fleet-integration-checklist.md)
- [`swe-ai-fleet-shadow-mode-spec.md`](./swe-ai-fleet-shadow-mode-spec.md)

## What Should Not Reopen In This Repo

Do not reopen kernel work to add:

- `planning.*` consumers
- `orchestration.*` consumers
- new public DTOs with `swe-ai-fleet` nouns
- direct kernel concepts such as `story`, `task`, `project`, or `epic`
- product-specific rollout logic

If any of those are needed, the change belongs in the integrating product.

## Allowed Future Work In This Repo

Future work here should stay generic and kernel-owned, for example:

- contract hardening
- generic observability improvements
- performance work
- tenancy and access-boundary hardening
- deterministic rendering improvements
- generic runtime integration improvements

## Exit Statement

This repo can be treated as functionally complete for the migration scope that
belongs here when all of these are true:

- `main` stays green under the full quality gate
- the node-centric public boundary remains protected by contract CI
- agentic pull and event-driven proofs remain green
- no new product-specific nouns enter the kernel boundary
- remaining migration and rollout work is executed outside this repo
