# Kernel Runtime Integration Reference

Status: Active
Scope: reference contract for external runtimes that consume the kernel

## Intent

Document the smallest practical integration shape for an agent runtime that
uses `kmp` as its context engine.

This is not a kernel-owned transport contract. The owned kernel boundary is:

- `underpass.rehydration.kernel.v1beta1` (gRPC)
- `graph.node.materialized` / `node.detail.materialized` (async inbound)

This document defines a recommended **consumer-side runtime shape** proven by
the agentic E2E tests.

## Design Rules

- The runtime sends `root_node_id`, not product-specific ids
- The runtime discovers focus through graph relationships in the context response
- The runtime consumes rendered context and node detail as generic data
- No runtime-specific nouns are added to kernel contracts

## Reference Runtime HTTP Shape

The reference runtime exposes three endpoints:

1. `POST /v1/sessions`
2. `GET /v1/sessions/{session_id}/tools`
3. `POST /v1/sessions/{session_id}/tools/{tool_name}/invoke`

Implemented in:

- [`runtime_http_client.rs`](../../../crates/kmp-transport-grpc/src/agentic_reference/runtime_http_client.rs)
- [`fake_underpass.rs`](../../../crates/kmp-tests-shared/src/runtime/fake_underpass.rs)
- [`main.rs`](../../../crates/kmp-transport-grpc/src/bin/runtime_reference_client/main.rs)

Reference payloads: [`api/examples/runtime-reference/v1/README.md`](../../../api/examples/runtime-reference/v1/README.md)

## Kernel Call Sequence

The reference agent (`basic_context_agent.rs`) follows this sequence:

1. List runtime tools → validate `fs.write`, `fs.read`, `fs.list` exist
2. Call `GetContext` with broad depth → scan neighbors for focus node by kind
3. Call `GetContext` with `focus_node_id` → get rendered context + detail
4. Build artifact from root title + focus detail + rendered context
5. Invoke runtime tools (`fs.write`, `fs.read`, `fs.list`)

Implemented in:

- [`basic_context_agent.rs`](../../../crates/kmp-transport-grpc/src/agentic_reference/basic_context_agent.rs)
- [`config.rs`](../../../crates/kmp-transport-grpc/src/bin/runtime_reference_client/config.rs)

## Reference Client

The repository contains a runnable reference client:

```bash
KERNEL_GRPC_ENDPOINT=http://localhost:50054 \
RUNTIME_BASE_URL=http://localhost:8080 \
ROOT_NODE_ID=node:workspace:my-project \
cargo run -p kmp-transport-grpc --bin runtime_reference_client
```

| Variable | Required | Description |
|:---------|:---------|:------------|
| `KERNEL_GRPC_ENDPOINT` | yes | Kernel gRPC address |
| `RUNTIME_BASE_URL` | yes | Runtime HTTP base URL |
| `ROOT_NODE_ID` | yes | Graph root node |
| `ROOT_NODE_KIND` | no | Node kind for display |
| `AGENT_ROLE` | no | Role (default: implementer) |
| `AGENT_FOCUS_NODE_KIND` | no | Kind to select as focus |
| `AGENT_TOKEN_BUDGET` | no | Token budget |
| `AGENT_SUMMARY_PATH` | no | Path for artifact output |

Prints JSON execution summary.

## What the E2E Proves

- A runtime can consume kernel context without product-specific nouns
- An agent can derive focus from graph structure (neighbor scan by kind)
- An agent can drive tool execution from rendered context + node detail
- A session-tools-invoke runtime shape is sufficient for basic integration

See:

- [`kernel-agentic-integration-e2e.md`](./kernel-agentic-integration-e2e.md)
- [`kernel-agentic-event-trigger-e2e.md`](./kernel-agentic-event-trigger-e2e.md)

## What Is Not Yet Frozen

- Auth headers
- Tenancy propagation
- Long-lived session renewal
- Streaming outputs
- Tool cancellation
- Richer approval workflows

These should be added only when proven necessary by a real consumer.

## Ownership Boundary

The runtime reference shape is **not a kernel-owned protocol commitment**.
The kernel does not expose HTTP paths. The examples here are guidance for
consumer runtimes. The owned kernel boundary remains gRPC + async subjects.
