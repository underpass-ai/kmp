# swe-ai-fleet Integration Checklist

Status: draft
Scope: operational checklist for adopting `rehydration-kernel` from
`swe-ai-fleet`

## Purpose

Turn the integration strategy into an execution checklist that a team can work
through without reintroducing `swe-ai-fleet` domain language into the kernel.

This checklist assumes:

- `rehydration-kernel` stays generic and node-centric
- `swe-ai-fleet` owns the anti-corruption layer
- rollout safety is enforced through shadowing, telemetry, and explicit
  rollback switches

## Non-Negotiable Rules

- keep all `swe-ai-fleet` legacy nouns out of kernel domain and transport
- keep the adapter in `swe-ai-fleet` split into small modules
- do not create a god integration service
- keep mapping explicit at the edge
- keep read parity measurable before cutover
- keep rollback one switch away

## Checklist

### 1. Freeze The Kernel Contract

- [ ] Freeze the exact gRPC methods that `swe-ai-fleet` may call.
- [ ] Freeze the request fields that are considered stable integration inputs.
- [ ] Freeze the response fields that are considered stable integration outputs.
- [ ] Freeze the generic async subjects that are kernel-owned.
- [ ] Freeze error mapping expectations for gRPC and async flows.
- [ ] Freeze snapshot, render budget, and focus semantics used by the adapter.
- [ ] Record the exact kernel version or commit range that `swe-ai-fleet` will
      target first.

Exit gate:

- [ ] The adapter team can point to one stable contract document and one stable
      implementation target.

### 2. Inventory swe-ai-fleet Touchpoints

- [ ] List every current caller of the legacy Context Service.
- [ ] List every inbound `planning.*` subject currently handled by the legacy
      service.
- [ ] List every inbound `orchestration.*` subject currently handled by the
      legacy service.
- [ ] List every outbound publication that downstream systems expect.
- [ ] List every config dependency, timeout, retry policy, and environment
      assumption tied to the current integration.
- [ ] Mark each touchpoint as read path, write path, or async bridge.

Exit gate:

- [ ] No integration surface remains implied or tribal.

### 3. Define The Anti-Corruption Layer In swe-ai-fleet

- [ ] Create a dedicated integration package or module boundary in
      `swe-ai-fleet`.
- [ ] Split it into focused modules such as:
- [ ] `context_kernel_client`
- [ ] `legacy_to_node_mapping`
- [ ] `node_to_legacy_mapping`
- [ ] `async_subject_bridge`
- [ ] `shadow_comparison`
- [ ] `cutover_routing`
- [ ] Define module ownership so one file does not absorb mapping, transport,
      routing, and comparison logic together.
- [ ] Keep product nouns local to this adapter boundary.

Exit gate:

- [ ] The adapter shape is explicit and does not depend on a future god class.

### 4. Define The Mapping Matrix

- [ ] Map each legacy root entity to `root_node_id`.
- [ ] Map each legacy focus concept to a node-centric focus input.
- [ ] Map each legacy read role or scope to generic role or scope inputs.
- [ ] Map each legacy payload family to `node detail`, `relationship`, or graph
      mutation concepts.
- [ ] Map each legacy response DTO to the subset of kernel output it really
      needs.
- [ ] Mark every field as:
- [ ] direct map
- [ ] derived map
- [ ] unsupported
- [ ] deferred
- [ ] Capture every place where semantic loss is acceptable versus blocking.

Exit gate:

- [ ] Every legacy noun that still matters has a documented translation rule.

### 5. Implement The Read Adapter

- [ ] Route legacy read calls through `context_kernel_client`.
- [ ] Translate legacy identifiers into `root_node_id`.
- [ ] Translate legacy focus selectors into node-centric focus inputs.
- [ ] Translate legacy budget knobs into render budget hints.
- [ ] Normalize kernel responses back into the minimal legacy shape still
      needed by `swe-ai-fleet`.
- [ ] Keep rendering decisions outside of kernel domain objects.
- [ ] Add integration tests around each read path.

Exit gate:

- [ ] Legacy read callers can obtain equivalent context through the adapter.

### 6. Implement The Write Adapter

- [ ] Route legacy update operations into generic kernel mutations where they
      exist.
- [ ] Keep unsupported legacy write flows explicitly marked as unsupported or
      deferred instead of silently approximated.
- [ ] Ensure each write path declares whether it affects node detail,
      relationship state, or snapshot behavior.
- [ ] Add integration tests for each mapped write path.

Exit gate:

- [ ] Every write flow in scope either maps cleanly or is explicitly excluded.

### 7. Implement The Async Bridge

- [ ] Keep `planning.*` consumers inside `swe-ai-fleet`.
- [ ] Keep `orchestration.*` consumers inside `swe-ai-fleet`.
- [ ] Translate inbound fleet events into kernel calls or kernel-owned
      publications through focused handlers.
- [ ] Keep async subject translation outside of kernel code.
- [ ] Preserve request correlation, retries, and failure visibility at the
      adapter edge.
- [ ] Add tests for:
- [ ] valid event to kernel action
- [ ] invalid payload handling
- [ ] retry or negative-ack behavior
- [ ] publication failure behavior

Exit gate:

- [ ] Fleet async flows work without adding fleet subjects to the kernel.

### 8. Add Shadow Comparison

- [ ] Run legacy and kernel-backed reads side by side.
- [ ] Normalize outputs before comparison.
- [ ] Compare selected nodes, rendered text, budget outcomes, and key metadata.
- [ ] Record tolerated drift explicitly.
- [ ] Emit telemetry for mismatch rate and mismatch classes.
- [ ] Keep shadow mode read-only until drift is understood.

Exit gate:

- [ ] Drift is measurable and low enough to support controlled cutover.

### 9. Add Telemetry And Operability

- [ ] Emit metrics for adapter request rate, latency, failures, and retries.
- [ ] Emit metrics for shadow mismatches by category.
- [ ] Emit metrics for stale or missing node detail.
- [ ] Emit metrics for snapshot hits, misses, and TTL behavior.
- [ ] Log enough normalized identifiers to debug issues without leaking
      sensitive payloads.
- [ ] Define dashboards and alerts before live cutover.

Exit gate:

- [ ] Operators can explain failures, drift, and context quality in production.

### 10. Roll Out Safely

- [ ] Add a flag for read shadowing.
- [ ] Add a flag for read cutover.
- [ ] Add a flag for async cutover.
- [ ] Define the rollback switch and document it.
- [ ] Run canary rollout on a narrow traffic slice first.
- [ ] Review mismatch, error, and latency telemetry before widening.
- [ ] Decommission the legacy path only after sustained healthy operation.

Exit gate:

- [ ] Rollout and rollback are operationally boring.

## Questions To Answer Before Implementation

- [ ] Can `swe-ai-fleet` map its current domain to `node`, `relationship`, and
      `detail` without asking the kernel to adopt product language?
- [ ] Is `root_node_id` enough as the integration anchor for every read path in
      scope?
- [ ] Which legacy responses truly need backward-compatible shaping, and which
      callers can move to node-centric outputs directly?
- [ ] Are there legacy flows whose semantics do not fit a graph-native context
      engine and should therefore stay outside the migration?
- [ ] Can the adapter remain useful if `swe-ai-fleet` evolves its internal
      domain again later?

## Questions To Answer Before Claiming Broad Reusability

- [ ] Can another coding or workspace product integrate by graph mapping alone?
- [ ] Can a research or knowledge system use the same kernel contract without
      domain-specific extensions?
- [ ] Can a multi-agent orchestrator use the kernel without forcing its own
      event vocabulary into the kernel?
- [ ] Can a support or operations platform apply tenancy, audit, and bounded
      rendering safely through the same public surface?
- [ ] Is the kernel still useful if the integrating product has no backlog or
      planning concepts at all?

## Done Definition

The migration can be called structurally sound when all of these are true:

- [ ] the kernel remains generic and node-centric
- [ ] `swe-ai-fleet` owns all legacy mapping
- [ ] read parity is proven through shadow comparison
- [ ] async bridging is product-owned and observable
- [ ] rollout and rollback are both rehearsed
- [ ] another agentic product could plausibly adopt the same kernel without
      changing its domain model
