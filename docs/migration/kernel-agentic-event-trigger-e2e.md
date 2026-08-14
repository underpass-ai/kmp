# Kernel Agentic Event Trigger E2E

> **Path update**: Test files referenced below moved from
> `crates/kmp-transport-grpc/tests/` to `crates/kmp-tests-kernel/tests/`
> and support modules to `crates/kmp-tests-shared/src/`. See the
> [benchmark docs](../research/benchmark-paper-use-cases.md) for current paths.

Status: Active

## Goal

Prove that an external runtime can be triggered by the generic kernel-owned
subject `context.bundle.generated`, not only by a pull-style gRPC caller.

This milestone extends the existing agentic proof with an async entrypoint:

1. projection events materialize graph state and node detail
2. a generic bundle-generated event is published on NATS
3. a runtime-side trigger consumer receives that event
4. the trigger consumer calls the shared agent flow
5. the agent queries the kernel over gRPC and drives runtime tools

## What Is Implemented

The repository now contains a dedicated container-backed suite in
[`agentic_event_integration.rs`](../../crates/kmp-transport-grpc/tests/agentic_event_integration.rs).

The suite proves two runtime modes:

- `RecordingRuntime`
  - deterministic in-memory runtime used to assert the triggered artifact
- `UnderpassRuntimeClient`
  - HTTP runtime client reusing the same session-and-tools reference contract

The event-driven support stays split into small files:

- event publisher and parser:
  - [`context_bundle_generated_event.rs`](../../crates/kmp-transport-grpc/tests/support/context_bundle_generated_event.rs)
- trigger consumer:
  - [`event_driven_runtime_trigger.rs`](../../crates/kmp-transport-grpc/tests/support/event_driven_runtime_trigger.rs)
- shared agent logic:
  - [`basic_context_agent.rs`](../../crates/kmp-transport-grpc/src/agentic_reference/basic_context_agent.rs)
- shared runtime client:
  - [`runtime_http_client.rs`](../../crates/kmp-transport-grpc/src/agentic_reference/runtime_http_client.rs)

## What The E2E Proves

The event-driven proof shows that:

- `context.bundle.generated` is enough to bootstrap a runtime action
- the trigger consumer only needs `root_node_id` and role hints from the event
- the detailed runtime workflow still stays node-centric and generic
- the same agent flow can be reused across pull and async entrypoints

The tested flow is:

1. publish `graph.node.materialized`
2. publish `node.detail.materialized`
3. wait until the kernel can answer `GetContext`
4. subscribe to `rehydration.context.bundle.generated`
5. publish `rehydration.context.bundle.generated`
6. derive `root_node_id` and role from the event
7. call `GetGraphRelationships`
8. call `GetContext`
9. invoke runtime tools and verify the resulting artifact

## Why This Matters

This closes a practical gap for reusable agentic integrations:

- a runtime no longer needs a human or orchestrator to call the kernel first
- generic async events can trigger work without fleet-specific subjects
- the trigger path stays outside kernel domain logic
- the same runtime contract can be reused for both sync and async entrypoints

That makes the kernel easier to integrate into:

- workspace runtimes
- automation runtimes
- orchestration layers
- event-driven copilots

## Current Limits

This proof still does not claim:

- execution against the real `underpass-runtime` binary
- durable consumer state or redelivery strategy for the trigger consumer
- auth or tenancy propagation across the async boundary
- a frozen runtime-owned event trigger contract

The owned kernel guarantee remains the generic subject and payload semantics,
not the consumer implementation strategy.
