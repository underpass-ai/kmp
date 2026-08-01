# MCP Stdio Adapter (`rehydration-mcp`)

One binary, one tool surface, two operating modes. The KMP tools are
identical in both — same schemas, same JSON by construction — so a client
switches modes by changing environment variables only:

- `kernel_ingest`, `kernel_write_memory`
- `kernel_wake`, `kernel_ask`
- `kernel_goto`, `kernel_near`, `kernel_rewind`, `kernel_forward`
- `kernel_trace`, `kernel_inspect`

## Modes at a glance

| | **Embedded** (primary) | **Live** (infrastructure gRPC) | Fixture (test-only) |
| --- | --- | --- | --- |
| Select with | `REHYDRATION_MCP_BACKEND=embedded` | `REHYDRATION_KERNEL_GRPC_ENDPOINT=…` (backend defaults to `grpc`) | `REHYDRATION_MCP_BACKEND=fixture` |
| Kernel runs | in-process, inside this binary | remote `KernelMemoryService` gRPC | none (canned responses) |
| Storage | one local data dir (`.kernel/`, redb) | Neo4j / Valkey / NATS behind the server | none |
| `read_after_write_ready` | always `true` (synchronous projection) | `true` on live ingest | `false` |
| Requires | nothing | deployed kernel + TLS config | nothing |
| Concurrency | single writer per data dir (ADR-011) | server-side | n/a |

The binary is fail-fast: with no configuration it exits with guidance
instead of guessing a mode.

## Embedded mode (primary)

The kernel runs inside the binary: zero infrastructure, per-project memory,
fsync-durable commits.

```bash
REHYDRATION_MCP_BACKEND=embedded rehydration-mcp
```

- **Data directory resolution** (ADR-012, winning rule logged at startup):
  `REHYDRATION_MCP_DATA_DIR` → project `.kernel/` (walks up to the `.git`
  root; auto-gitignored) → `$XDG_DATA_HOME/rehydration-kernel/default`.
- **Layout**: `FORMAT_VERSION` (fail-fast on mismatch), `store/kernel.redb`,
  `logs/` (rotating JSON logs; stderr also — stdout is JSON-RPC only),
  `telemetry/quality.redb` (bounded fail-open quality journal, ADR-014).
- **Single writer**: a second session on the same data dir fails fast with
  an explicit error; the tools then do not appear in the host's inventory.
  Open the host in a project with a free store or close the other session.

### Maintenance CLI (embedded stores)

Everything is consumed as a process — memory over MCP stdio, maintenance
over CLI subcommands:

```bash
rehydration-mcp --version                 # binary + store format version
rehydration-mcp export memory.jsonl      # event log -> portable bundle
rehydration-mcp import memory.jsonl      # bundle -> EMPTY store (fail-fast)
```

See [embedded-release.md](embedded-release.md) for the bundle format and the
binary↔store-format compatibility matrix, and
[embedded-hosts.md](embedded-hosts.md) for per-host registration recipes and
the context-recovery playbook.

## Live mode (infrastructure gRPC)

The adapter calls the typed gRPC `KernelMemoryService` of a deployed kernel.
The MCP process owns JSON-RPC parsing, tool schemas, JSON/proto conversion
and TLS configuration; it never calls lower-level query/command services for
KMP moves.

```bash
REHYDRATION_KERNEL_GRPC_ENDPOINT=http://127.0.0.1:50051 rehydration-mcp
```

HTTPS endpoints enable server TLS with system/webpki roots automatically.
Private CAs and mTLS are explicit:

```bash
REHYDRATION_KERNEL_GRPC_ENDPOINT=https://kernel.example.svc:50054 \
REHYDRATION_KERNEL_GRPC_TLS_MODE=mutual \
REHYDRATION_KERNEL_GRPC_TLS_CA_PATH=/var/run/kernel-tls/ca.crt \
REHYDRATION_KERNEL_GRPC_TLS_CERT_PATH=/var/run/kernel-tls/tls.crt \
REHYDRATION_KERNEL_GRPC_TLS_KEY_PATH=/var/run/kernel-tls/tls.key \
REHYDRATION_KERNEL_GRPC_TLS_DOMAIN_NAME=rehydration-kernel-grpc \
  rehydration-mcp
```

Tool → RPC binding: `kernel_ingest`/`kernel_write_memory` →
`Ingest` (write_memory compiles to canonical ingest), `kernel_wake` → `Wake`,
`kernel_ask` → `Ask`, temporal tools → `Goto`/`Near`/`Rewind`/`Forward`,
`kernel_trace` → `Trace`, `kernel_inspect` → `Inspect`.

## Tool semantics (identical in both modes)

- `kernel_ask` returns a deterministic evidence-derived answer or `UNKNOWN`;
  it never generates an LLM answer.
- Temporal tools return deterministic kernel-owned traversal slices with a
  `page` object, so bounded partial reads are visible to operators and
  clients. `kernel_goto` defaults to at most 50 entries when no explicit
  `limit.entries` is supplied.
- Dimension scope is explicit and auditable: omitted → `current_about`;
  `abouts` requires a non-empty list; `all_abouts` traverses every memory
  anchor; `scope_ids` accepts local or namespaced
  `about:<about>:dimension:<id>` ids. Coordinate dimension kinds are checked
  against their declarations during ingest.
- `kernel_inspect` supports object/detail/incoming/outgoing/evidence lookup;
  `include.raw=true` returns typed raw audit refs including dimension
  coordinates. Temporal `include.raw_refs/evidence/relations` are supported.
- Entry metadata and evidence metadata/source round-trip through typed reads;
  evidence `supports` contains the refs reached by stored support relations.
- Tool failures set `isError=true` and include
  `structuredContent.error.{code,message}` while retaining textual MCP
  content for compatibility.
- Cross-mode parity is not aspirational: the embedded backend reuses the
  live JSON path (shared proto mapping), and the conformance suite pins the
  storage semantics across all backends in CI.

## Fixture mode (test-only)

Deterministic canned responses for client wiring and demos; must be selected
explicitly:

```bash
REHYDRATION_MCP_BACKEND=fixture rehydration-mcp
```

## Installation

Prebuilt binaries + one-command installer: see
[embedded-release.md](embedded-release.md). From source:

```bash
cargo install --git https://github.com/underpass-ai/rehydration-kernel rehydration-mcp --locked
# or, in a checkout:
cargo install --path crates/rehydration-mcp --locked
```

The repository helper wraps the same install path with pinned refs
(`scripts/mcp/install-rehydration-mcp.sh`, `REHYDRATION_MCP_TAG=…`). The
crate is not on crates.io yet (pending the ADR-013 branding revisit).

## Client configuration

Embedded (recommended default — per-project memory, zero infrastructure):

```toml
[mcp_servers.kernel-memory]
command = "rehydration-mcp"
env = { REHYDRATION_MCP_BACKEND = "embedded" }
```

```bash
claude mcp add kernel-memory --scope user \
  --env REHYDRATION_MCP_BACKEND=embedded -- ~/.cargo/bin/rehydration-mcp
```

Live gRPC (shared deployed kernel):

```toml
[mcp_servers.rehydration-kernel]
command = "rehydration-mcp"
env = { REHYDRATION_KERNEL_GRPC_ENDPOINT = "https://kernel.example.com" }
```

## Smoke tests

Embedded end-to-end (three independent processes: write → recover → audit):

```bash
bash scripts/demo/embedded_two_sessions.sh
```

Fixture / live stdio smoke:

```bash
REHYDRATION_MCP_BACKEND=fixture REHYDRATION_MCP_BIN=rehydration-mcp \
  bash scripts/mcp/kmp-stdio-smoke.sh

REHYDRATION_KERNEL_GRPC_ENDPOINT=http://127.0.0.1:50051 \
REHYDRATION_MCP_BIN=rehydration-mcp \
KMP_MCP_SMOKE_REF=node:mission:engine-core-failure \
  bash scripts/mcp/kmp-stdio-smoke.sh
```

Real-kernel integration smoke (containerized live kernel):

```bash
bash scripts/ci/integration-mcp-real-kernel.sh
```

## Manual JSON-RPC check

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | REHYDRATION_MCP_BACKEND=embedded rehydration-mcp
```

One JSON-RPC response per input line; logs never touch stdout.
