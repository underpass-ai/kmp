# kmp-mcp

The stdio MCP adapter for
[KMP by Underpass](https://github.com/underpass-ai/kmp), the Kernel Memory
Protocol — navigable, temporal, multidimensional memory for AI agents.

```bash
cargo install kmp-mcp
```

For a one-command setup that also teaches the tool surface and diagnoses a
broken wiring, see the
[KMP plugin](https://github.com/underpass-ai/kmp/tree/main/plugins/kmp) for
Codex and Claude Code.

## Three backends

| `KMP_MCP_BACKEND` | What it talks to | What it needs |
|:--|:--|:--|
| `embedded` | the kernel in this process; fresh stores use shareable SQLite, existing redb stores remain compatible | `KMP_MCP_DATA_DIR` (or the default data directory) |
| `grpc` (default) | a deployed kernel | `KMP_KERNEL_GRPC_ENDPOINT`, optionally the `KMP_KERNEL_GRPC_TLS_*` variables |
| `fixture` | the reference examples from the contract | nothing — it answers from embedded fixtures |

`embedded` is the one to start with: no server, no cluster, memory that
survives the session on your own disk. It also brings its own
[viewer](https://crates.io/crates/kmp-viewer) up at `http://127.0.0.1:7317/`
over that same kernel — your memory as a graph, read-only, loopback only, no
flag required. `KMP_VIEWER_ADDR` moves it; `off` declines it.

Current status:

- exposes `kmp_ingest`, `kmp_write_memory`, `kmp_wake`, `kmp_ask`,
  `kmp_goto`, `kmp_near`, `kmp_rewind`, `kmp_forward`,
  `kmp_trace`, and `kmp_inspect`;
- can serve explicit fixture-backed KMP responses, embedded from the
  contract's reference examples;
- can use the live gRPC kernel when `KMP_KERNEL_GRPC_ENDPOINT` is set;
- live mode calls the typed `KernelMemoryService` gRPC API directly;
- live `kmp_ask` returns a deterministic citation-oriented answer or
  `UNKNOWN`, not a generated answer; complete bodies are canonical in
  `proof.evidence`, joined from `because[].ref` and
  `proof.path[].evidence_refs`; successful asks expose retained recall terms
  and contributing semantic relation types in `proof` without exposing scoring
  internals;
- `kmp_ask` and `kmp_wake` preserve a stable core and fill a deterministic
  semantic prefix under `budget.max_bytes` (10,000 by default); expandable
  proof returns `projection.page.next_cursor`, while `budget.tokens` remains an
  advisory cl100k planning hint rather than a cross-model hard ceiling;
- dimension scope defaults to `current_about`; `abouts` requires a non-empty
  about list; `all_abouts` is explicit and uses the kernel memory about index;
- `kmp_inspect` supports typed detail/link lookup and typed raw audit refs
  when `include.raw=true`, including dimension coordinates when the inspected
  object is contained by memory dimensions;
- temporal `include.raw_refs=true` returns typed raw audit refs for the selected
  entries.

Run locally:

```bash
KMP_MCP_BACKEND=fixture cargo run -p kmp-mcp --locked
```

Install the unreleased tip instead of the published version:

```bash
cargo install --git https://github.com/underpass-ai/kmp kmp-mcp --locked
```

Building from source needs `protoc` on `PATH` (`protobuf-compiler` on
Debian/Ubuntu, `protobuf` on Homebrew).

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
Run it with nothing set and it serves the embedded kernel. An endpoint in
the environment chooses gRPC; `KMP_MCP_BACKEND=fixture` chooses the canned
responses, and has to be asked for by name so that memory which is not real
is never a default.

Minimal smoke request:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

Smoke an installed binary, with the script from the repository:

```bash
KMP_MCP_BACKEND=fixture KMP_MCP_BIN=kmp-mcp \
  bash scripts/mcp/kmp-stdio-smoke.sh
```

Tool call example:

```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"kmp_ask","arguments":{"about":"question:830ce83f","question":"Where did Rachel move after her recent relocation?","answer_policy":"evidence_or_unknown","budget":{"detail":"balanced","max_bytes":10000}}}}
```

When `projection.page.has_more` is true, repeat the same tool and bound
arguments with `page.cursor` set to `projection.page.next_cursor`. Only
`page.entries`, `budget.tokens`, and `budget.max_bytes` may change. The cursor
is opaque and remains valid while the selected response snapshot is
byte-identical; changed query/scope/detail or changed memory is rejected.

Live backend mapping:

| Tool | Kernel read/write |
|:-----|:------------------|
| `kmp_ingest` | `KernelMemoryService.Ingest` |
| `kmp_write_memory` | writer-friendly helper that validates relation quality and compiles to `KernelMemoryService.Ingest` |
| `kmp_wake` | `KernelMemoryService.Wake` |
| `kmp_ask` | `KernelMemoryService.Ask` |
| `kmp_goto` | `KernelMemoryService.Goto` |
| `kmp_near` | `KernelMemoryService.Near` |
| `kmp_rewind` | `KernelMemoryService.Rewind` |
| `kmp_forward` | `KernelMemoryService.Forward` |
| `kmp_trace` | `KernelMemoryService.Trace` |
| `kmp_inspect` | `KernelMemoryService.Inspect` |

## License

Apache-2.0.
