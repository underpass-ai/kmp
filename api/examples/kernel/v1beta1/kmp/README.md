# Kernel Memory Protocol Fixtures

Status: executable contract fixtures for the Kernel Memory Protocol public
memory slice.

This folder defines the transport-neutral Kernel Memory Protocol (KMP) shape.
MCP, gRPC, and NATS should carry these same memory moves instead of inventing
separate product models per transport.

Required moves:

| Move | Request fixture | Response fixture |
|:-----|:----------------|:-----------------|
| `ingest` | `ingest.request.json` | `ingest.response.json` |
| `write-memory` | `write-memory.request.json` | `write-memory.response.json` |
| `wake` | `wake.request.json` | `wake.response.json` |
| `ask` | `ask.request.json` | `ask.response.json` |
| `goto` | `goto.request.json` | `goto.response.json` |
| `near` | `near.request.json` | `near.response.json` |
| `rewind` | `rewind.request.json` | `rewind.response.json` |
| `forward` | `forward.request.json` | `forward.response.json` |
| `trace` | `trace.request.json` | `trace.response.json` |
| `inspect` | `inspect.request.json` | `inspect.response.json` |

Schema:

- `kernel-memory-protocol.schema.json`

The protocol contract is intentionally framed as memory moves:

```text
write-memory -> ingest -> wake -> ask -> goto/near/rewind/forward -> trace -> inspect
```

Transport bindings:

- MCP tools expose `kmp_ingest`, `kmp_write_memory`, `kmp_wake`,
  `kmp_ask`, `kmp_goto`, `kmp_near`, `kmp_rewind`,
  `kmp_forward`, `kmp_relate`, `kmp_trace`, and `kmp_inspect`.
- `kmp_write_memory` is a writer helper, not a parallel memory model. It
  validates writer intent and relation quality, then compiles to the canonical
  `kmp_ingest` payload. With `dry_run=true` it returns that payload as
  `ingest_preview`; with `dry_run=false` it forwards the same payload through
  the configured `kmp_ingest` backend.
- Rich external relations in `kmp_write_memory` must carry `read_context`
  evidence in strict mode, so a writer can prove it inspected, traced, asked,
  woke, or temporally traversed the target before claiming a rich semantic
  relation.
- gRPC exposes the canonical memory moves through the typed
  [`KernelMemoryService`](../../../../proto/underpass/rehydration/kernel/v1beta1/memory.proto).
  `write-memory` is bound to gRPC by compiling to `KernelMemoryService.Ingest`;
  it does not introduce a second write path. The gRPC contract is the
  executable binding for this cut.
- NATS KMP subjects such as `kernel.memory.ingest`,
  `kernel.memory.ingested`, and `kernel.memory.rejected` remain design
  guidance; they are not implemented in this cut.

These fixtures are not a replacement for the existing node-centric gRPC and
AsyncAPI contracts. They define the higher-level public memory shape that maps
onto those existing Kernel 1.0 primitives.

Current binding note:

- KMP temporal fixture aliases such as `at` and `from` map to the gRPC
  `TemporalCursor` field on method-specific temporal requests such as
  `GotoRequest`, `RewindRequest`, or `ForwardRequest`.
- KMP temporal responses follow the typed gRPC shape: `temporal.requested`
  carries the submitted cursor and `temporal.resolved` carries the resolved
  coordinate. They do not synthesize transport-specific `at`/`from`/`around`
  response fields.
- `IngestRequest.about` is the default dimension namespace. Logical dimension
  ids in KMP map to internal kernel ids shaped as
  `about:<about>:dimension:<dimension_id>`.
- gRPC `DimensionSelection` defaults to `CURRENT_ABOUT`. Cross-memory traversal
  must use `ABOUTS` with an explicit non-empty about list or `ALL_ABOUTS` to
  traverse every memory anchor from the kernel memory about index.
- `DimensionSelection.scope_ids` is an exact scope filter applied after
  `mode/include/exclude`. It accepts local dimension ids or fully namespaced
  ids shaped as `about:<about>:dimension:<dimension_id>`.
- MCP live mode is a thin `KernelMemoryService` client. It must not call
  `ContextQueryService` or `ContextCommandService` directly for KMP moves.
- `kmp_ask.answer` is deterministic evidence text, or `UNKNOWN`, not a
  generated answer and not an anchor summary.
- `kmp_inspect` supports typed object/detail/link/evidence lookup and typed
  raw audit refs when `include.raw=true`.
- Temporal `include.raw_refs=true` returns typed raw audit refs for selected
  entries without polluting normal semantic reads.
