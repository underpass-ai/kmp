# Rehydration MCP

Installable stdio MCP adapter for Kernel Memory Protocol (KMP).

Current status:

- exposes `kernel_ingest`, `kernel_write_memory`, `kernel_wake`, `kernel_ask`,
  `kernel_goto`, `kernel_near`, `kernel_rewind`, `kernel_forward`,
  `kernel_trace`, and `kernel_inspect`;
- can serve explicit fixture-backed KMP responses from
  `api/examples/kernel/v1beta1/kmp`;
- can use the live gRPC kernel when `KMP_KERNEL_GRPC_ENDPOINT` is set;
- live mode calls the typed `KernelMemoryService` gRPC API directly;
- live `kernel_ask` returns a deterministic evidence-derived answer or
  `UNKNOWN`, not a generated answer;
- dimension scope defaults to `current_about`; `abouts` requires a non-empty
  about list; `all_abouts` is explicit and uses the kernel memory about index;
- `kernel_inspect` supports typed detail/link lookup and typed raw audit refs
  when `include.raw=true`, including dimension coordinates when the inspected
  object is contained by memory dimensions;
- temporal `include.raw_refs=true` returns typed raw audit refs for the selected
  entries.

Run locally:

```bash
KMP_MCP_BACKEND=fixture cargo run -p kmp-mcp --locked
```

Install from Git:

```bash
cargo install --git https://github.com/underpass-ai/kmp kmp-mcp --locked
```

Install with the repo helper:

```bash
bash scripts/mcp/install-kmp-mcp.sh
```

Live gRPC backend:

```bash
KMP_KERNEL_GRPC_ENDPOINT=http://127.0.0.1:50054 cargo run -p kmp-mcp --locked
```

Public HTTPS endpoint:

```bash
KMP_KERNEL_GRPC_ENDPOINT=https://kmp.underpassai.com cargo run -p kmp-mcp --locked
```

The server reads newline-delimited JSON-RPC requests from stdin and writes
newline-delimited JSON-RPC responses to stdout.
The executable is fail-fast by default: set `KMP_KERNEL_GRPC_ENDPOINT`
for live gRPC mode, or set `KMP_MCP_BACKEND=fixture` explicitly for
fixture mode.

Minimal smoke request:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

Repo smoke script:

```bash
KMP_MCP_BACKEND=fixture KMP_MCP_BIN=kmp-mcp \
  bash scripts/mcp/kmp-stdio-smoke.sh
```

Smoke an installed binary:

```bash
KMP_KERNEL_GRPC_ENDPOINT=https://kmp.underpassai.com \
KMP_MCP_BIN=kmp-mcp \
  bash scripts/mcp/kmp-stdio-smoke.sh
```

Tool call example:

```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"kernel_ask","arguments":{"about":"question:830ce83f","question":"Where did Rachel move after her recent relocation?","answer_policy":"evidence_or_unknown"}}}
```

Live backend mapping:

| Tool | Kernel read/write |
|:-----|:------------------|
| `kernel_ingest` | `KernelMemoryService.Ingest` |
| `kernel_write_memory` | writer-friendly helper that validates relation quality and compiles to `KernelMemoryService.Ingest` |
| `kernel_wake` | `KernelMemoryService.Wake` |
| `kernel_ask` | `KernelMemoryService.Ask` |
| `kernel_goto` | `KernelMemoryService.Goto` |
| `kernel_near` | `KernelMemoryService.Near` |
| `kernel_rewind` | `KernelMemoryService.Rewind` |
| `kernel_forward` | `KernelMemoryService.Forward` |
| `kernel_trace` | `KernelMemoryService.Trace` |
| `kernel_inspect` | `KernelMemoryService.Inspect` |
