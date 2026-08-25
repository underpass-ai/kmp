# Kernel Agentic Integration E2E

> **Path update**: Test files referenced below moved from
> `crates/kmp-transport-grpc/tests/` to `crates/kmp-tests-kernel/tests/`
> and support modules to `crates/kmp-tests-shared/src/`. See the
> [benchmark docs](../research/benchmark-paper-use-cases.md) for current paths.

Status: Active

## Goal

Prove that `kmp` is consumable by agentic systems through a
generic node-centric contract, without embedding product-specific nouns into
the core.

This milestone demonstrates an end-to-end path where:

1. generic projection events arrive over NATS
2. the kernel materializes graph context and node detail
3. an agent queries the kernel over gRPC
4. the agent uses that context to drive tool execution in a runtime

## What Is Implemented

The repository now contains a container-backed agentic integration suite in
[`agentic_integration.rs`](../../../crates/kmp-tests-kernel/tests/agentic_integration.rs).

The suite proves two runtime modes:

- `RecordingRuntime`
  - deterministic in-memory runtime for precise assertions
- `UnderpassRuntimeClient`
  - runtime client that talks over an HTTP session-and-tools contract shaped to
    match the current `underpass-runtime` execution model

The test infrastructure is split into small files:

- runtime-agnostic agent logic:
  - [`basic_context_agent.rs`](../../../crates/kmp-transport-grpc/src/agentic_reference/basic_context_agent.rs)
- kernel fixture and projection seeding:
  - [`projection_runtime.rs`](../../../crates/kmp-adapter-nats/src/runtime/projection_runtime.rs)
- runtime abstractions and implementations:
  - [`workspace.rs`](../../../crates/kmp-tests-shared/src/runtime/workspace.rs)
  - [`runtime_http_client.rs`](../../../crates/kmp-transport-grpc/src/agentic_reference/runtime_http_client.rs)
  - [`fake_underpass.rs`](../../../crates/kmp-tests-shared/src/runtime/fake_underpass.rs)
  - [`main.rs`](../../../crates/kmp-transport-grpc/src/bin/runtime_reference_client/main.rs)

## What The E2E Proves

The current agentic proof shows that a consumer can integrate with the kernel
using only generic concepts:

- `root_node_id`
- node kinds
- graph relationships
- node detail
- rendered context

The agent does not need:

- `story`
- `task`
- `planning.*`
- `orchestration.*`
- any other fleet-specific contract

The current flow is:

1. publish `graph.node.materialized`
2. publish `node.detail.materialized`
3. wait until the kernel can answer `GetContext`
4. query `GetGraphRelationships` to select a focus node
5. query `GetContext` for focused context
6. write a runtime artifact using tool invocation
7. read and list runtime files to prove the tool loop completed

## Why This Matters For Agentic Integrability

This proof exercises the properties needed for reuse across different agentic
systems:

- context retrieval is graph-native, not tied to one product taxonomy
- tool execution is runtime-pluggable behind a narrow interface
- the agent can derive focus from relationships instead of hardcoded workflow
  nouns
- detailed context remains externalized in Valkey, so prompt assembly can stay
  bounded and selective

That makes the kernel easier to integrate into:

- coding agents
- research agents
- orchestration runtimes
- support or operations copilots

## Current Limits

This milestone does not yet prove:

- execution against the real `underpass-runtime` binary
- authentication or tenant isolation at the runtime boundary
- long-lived session behavior
- streaming tool output
- approval workflows richer than a simple boolean

The current HTTP runtime proof should be treated as a contract-shape adapter,
not as a claim that the sibling runtime is already integrated end to end.

The repository also exposes that same flow as a runnable reference client
outside `tests/`, so external runtimes can copy the integration shape without
depending on the e2e harness.

The async follow-up is now also implemented and documented in:

- [`kernel-agentic-event-trigger-e2e.md`](./kernel-agentic-event-trigger-e2e.md)
- [`kernel-runtime-integration-reference.md`](./kernel-runtime-integration-reference.md)
