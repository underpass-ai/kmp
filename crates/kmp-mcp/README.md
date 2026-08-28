# kmp-mcp

<!-- kmp:public-overview:begin -->
KMP gives Codex and Claude Code local-first memory that preserves what
happened, when and why. It stores decisions and evidence, not transcripts,
on embedded SQLite, and exposes them through ten memory tools plus three
semantic view tools over a shared ChronoLoom view.

Ask **“Show me the memory behind this decision.”** The agent retrieves the
evidence, opens ChronoLoom at the relevant moment and lights up the proof path.
You can click, filter, pan, undo or take control of the same view at any time.
<!-- kmp:public-overview:end -->

- MCP Registry name: `mcp-name: io.github.underpass-ai/kmp`

```bash
cargo install kmp-mcp
```

For a one-command setup that also teaches the tool surface and diagnoses a
broken wiring, see the
[KMP plugin](https://github.com/underpass-ai/kmp/tree/main/plugins/kmp) for
Codex and Claude Code.

## Local by default

Run `kmp-mcp` with nothing configured and the kernel stays in this process,
the memory stays on your disk and the viewer binds to loopback. There is no
KMP account, API key or service to contact. A remote kernel is used only when
you configure one.

## Three backends

| `KMP_MCP_BACKEND` | What it talks to | What it needs |
|:--|:--|:--|
| `embedded` (default) | the kernel in this process; SQLite memory and quality telemetry | nothing; `KMP_MCP_DATA_DIR` optionally selects the directory |
| `grpc` | a deployed kernel | `KMP_KERNEL_GRPC_ENDPOINT`, optionally the `KMP_KERNEL_GRPC_TLS_*` variables |
| `fixture` | the reference examples from the contract | nothing — it answers from embedded fixtures |

`embedded` is the one to start with: no server, no cluster, memory that
survives the session on your own disk. It also brings its own
[ChronoLoom visualizer](https://crates.io/crates/kmp-viewer) up at
`http://127.0.0.1:7317/` over that same kernel — your memory as a graph,
read-only and loopback only, no flag required. If another session owns 7317,
this process binds a free per-session loopback port. The process prints a random
one-session capability link; `kmp_view_open` and `kmp_view_get_state` return
that same link when a host hides server output. The browser exchanges it for
an HttpOnly cookie and removes the token from its URL. `KMP_VIEWER_ADDR` moves
it to an explicit address; `off` declines it. `kmp_view_open` also reports
`viewer_available`, so a semantic view can never masquerade as a browser the
session did not mount.

Ask Codex or Claude to **“show me the memory behind this decision.”** The agent
can open ChronoLoom at the relevant moment, select the evidence and light up
the proof path. It declares semantic intent through `kmp_view_apply_intent`;
the browser follows by long-poll, with the move explained and undoable.

![Codex asks KMP about a decision and ChronoLoom answers with the live memory and proof path](https://raw.githubusercontent.com/underpass-ai/kmp/v0.3.0/docs/assets/kmp-agent-loom.gif)

**The view is shared:** the agent can steer ChronoLoom for you; you can click,
filter, pan, undo or take control yourself at any time.

Current status:

- exposes `kmp_ingest`, `kmp_write_memory`, `kmp_wake`, `kmp_ask`,
  `kmp_goto`, `kmp_near`, `kmp_rewind`, `kmp_forward`,
  `kmp_trace`, `kmp_inspect`, `kmp_view_open`, `kmp_view_apply_intent`, and
  `kmp_view_get_state`;
- can serve explicit fixture-backed KMP responses, embedded from the
  contract's reference examples;
- can use the live gRPC kernel when `KMP_KERNEL_GRPC_ENDPOINT` is set;
- live mode calls the typed `KernelMemoryService` gRPC API directly;
- live `kmp_ask` returns a deterministic citation-oriented answer or
  `UNKNOWN`, not a generated answer; complete bodies are canonical in
  `proof.evidence`, joined from `because[].ref` and
  `proof.path[].evidence_refs`; successful asks expose retained recall terms
  and contributing semantic relation types in `proof` without exposing scoring
  internals; entry claims and stored evidence are both searchable and remain
  distinguishable through `proof.evidence[].metadata.proof_role`;
- `kmp_ask` and `kmp_wake` preserve a stable core and fill a deterministic
  semantic prefix under `budget.max_bytes` (10,000 by default); expandable
  proof returns `projection.page.next_cursor`, while `budget.tokens` remains a
  reported compatibility hint and never filters canonical structured content;
  `budget.max_bytes` is the cross-host hard ceiling;
- dimension scope defaults to `current_about`; `abouts` requires a non-empty
  about list; `all_abouts` is explicit and uses the kernel memory about index;
- `kmp_inspect` keeps the typed object stable and pages evidence, outgoing
  links, incoming links and typed raw audit refs under `budget.max_bytes`;
  partial pages report the complete size and an opaque continuation cursor, so
  callers can continue or raise the ceiling without probing;
- temporal `include.raw_refs=true` returns typed raw audit refs for the selected
  entries, and an optional `axis` selects occurred, observed, ingested or
  validity time without fallback;
- clients that negotiate `io.modelcontextprotocol/ui` receive ChronoLoom as a
  self-contained `ui://kmp/chronoloom.html` MCP App. Its visual projection data
  tool is app-only and returns only a bounded receipt in model-visible text.

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
KMP_KERNEL_GRPC_ENDPOINT=https://kmp.example.com cargo run -p kmp-mcp --locked
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
| ChronoLoom app data | `KernelMemoryService.ProjectVisual` (hidden from model tool discovery) |

## License

Apache-2.0.
