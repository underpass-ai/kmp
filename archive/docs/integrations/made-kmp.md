# MADE ↔ KMP Integration Guide (Incident Resolution)

Audience: the agent integrating **MADE by Underpass** (as the
incident-resolution coordination plane) with KMP. Everything here exists
in-repo today unless explicitly marked *planned*.

> Supersedes `choreographer-kmp.md`. MADE shipped as *Underpass Choreographer*
> until the rename; that was a naming change only.

> Edition vocabulary used here is the one in [`docs/editions.md`](../../../docs/editions.md):
> **embedded** (in-process, one local data dir) and **cluster** (deployed
> `KernelMemoryService` over gRPC).

## 1. What you are integrating with

KMP (Kernel Memory Protocol) is a graph-temporal memory kernel with two
editions of the **same product** (identical tool semantics, pinned by the
conformance suite in `crates/kmp-conformance` — a behavior
difference between editions is a bug):

| | Cluster edition | Embedded edition |
| --- | --- | --- |
| Runs as | gRPC service on Kubernetes (Neo4j/Valkey/NATS) | Single binary, MCP stdio, one local data dir |
| Use for | Production incident memory, multi-agent | Local dev of this integration; per-machine memory |
| Consistency | Async projection (bounded staleness on event path) | `read_after_write_ready=true` always |

Switching editions is one env var — develop against embedded, promote to
live without changing how you call the tools.

## 2. Connecting

**Embedded (start here):**

```bash
cargo install --path crates/kmp-mcp   # or use the workspace binary
KMP_MCP_BACKEND=embedded \
KMP_MCP_DATA_DIR=/var/lib/made/kmp \
kmp-mcp
```

- stdin/stdout is MCP JSON-RPC; **all logs go to stderr** — never parse stdout
  as anything but JSON-RPC.
- Data dir resolution when `KMP_MCP_DATA_DIR` is unset: project
  `.kernel/` (walks up to `.git`, auto-gitignored) → `$XDG_DATA_HOME/kmp/default`.
  For a service like MADE, always set it explicitly.
- **Single-writer** (ADR-011): one process per data dir; a second open
  fails fast with an explicit error. Plan one kernel per MADE
  instance.

**Cluster (production):**

```bash
KMP_MCP_BACKEND=grpc \
KMP_KERNEL_GRPC_ENDPOINT=https://kernel.example:50051 \
KMP_KERNEL_GRPC_TLS_MODE=mutual \
KMP_KERNEL_GRPC_TLS_CA_PATH=... \
KMP_KERNEL_GRPC_TLS_CERT_PATH=... \
KMP_KERNEL_GRPC_TLS_KEY_PATH=... \
kmp-mcp
```

Native gRPC is also available (proto contract in `crates/kmp-proto`,
`api/`); the MCP tool JSON is byte-equivalent across backends.

## 3. Tool surface (10 tools + aliases)

`kmp_ingest` (aliases `kernel_remember`, `kernel_ingest_context`),
`kmp_write_memory`, `kmp_wake`, `kmp_ask`, `kmp_goto`,
`kmp_near`, `kmp_rewind`, `kmp_forward`, `kmp_trace`,
`kmp_inspect`. Canonical request/response examples:
`api/examples/kernel/v1beta1/kmp/*.json`.

### Incident-memory conventions

- **about** = one incident: `incident:<id>` (e.g. `incident:INC-2431`).
- **dimensions**: declare on first write, e.g. `timeline:<incident-id>`
  (kind `timeline`) for the event sequence; add `service:<name>` scopes if
  you partition observations per affected service.
- **entries**: one entry per *decision, observation, constraint, or
  outcome* — never transcripts. Every entry needs ≥1 coordinate
  (`dimension` + `scope_id`, ideally `occurred_at` + `sequence`); the
  coordinates are what make `goto/near/rewind/forward` (known-at-time
  navigation) work later. Coordinate `dimension` must match the declared
  dimension `kind` for its `scope_id`; mismatches are rejected at ingest.
  Entry `metadata` round-trips through temporal reads and inspect.
- **relations**: typed with proof. Non-structural relations (e.g.
  `caused_by`, `supports` with class `causal`/`evidential`) **require
  `confidence` and `why` or `evidence`** — the kernel rejects anemic causal
  claims by design. The writer vocabulary includes `triggers`, `authorizes`,
  and `verified_by`; custom relation names remain open. In addition to the
  compatibility fields (`why`, `evidence`, `confidence`, `sequence`), a
  relation can carry `motivation`, `method`, `decision_id`,
  `caused_by_node_id`, and its own temporal `coordinate`.
- **evidence**: attach log excerpts/links as evidence items supporting
  entries; their ingested `source`, metadata, and actual `supports` targets
  surface later through temporal, context, and inspect proof views.

### Read playbook

| Moment | Tool |
| --- | --- |
| Incident (re)opened / responder joins | `kmp_wake {about}` — full anchored context with proof |
| Specific question ("did we restart X?") | `kmp_ask {about, question}` |
| "What did we know at 03:20?" | `kmp_goto {about, cursor:{time}}` |
| Context around a decision | `kmp_near {about, around:{ref}}` |
| Step back/forward through the timeline | `kmp_rewind` / `kmp_forward` |
| Why-chain between two refs | `kmp_trace {from, to}` |
| Audit one claim + its proof | `kmp_inspect {ref, include_raw:true}` |

## 4. Guarantees and sharp edges

- Embedded ingest is synchronous: `read_after_write_ready=true` — wake
  immediately after write sees the memory.
- `kmp_goto` returns up to 50 entries by default. Every temporal result
  includes `page.returned`, `page.total`, and `page.has_more`; set
  `limit.entries` explicitly when the caller needs a different bound.
- MCP tool failures retain human-readable `content` and also return
  `structuredContent.error` with a stable category such as `not_found` or
  `invalid_argument`.
- **Idempotency-key retry caveat**: retrying the same `idempotency_key`
  *after* a successful ingest returns an explicit conflict (state intact) —
  it is not an idempotent OK. Treat conflict-on-retry as "already applied";
  generate one key per logical write (e.g. `ingest:<incident>:<step>`).
- Fail-fast everywhere: locked store, corrupt layout, format-version
  mismatch are explicit errors, never silent empty memory
  (`docs/runtime-guarantees.md` + ADR-012).
- The event log is append-only and auditable; projections are rebuildable
  offline (replay tooling in `kmp-adapter-embedded`).

## 5. Quality telemetry for MADE (ADR-014)

Every embedded context, temporal, and trace read (`kmp_wake`, `kmp_ask`,
`kmp_goto`, `kmp_near`, `kmp_rewind`, `kmp_forward`, and
`kmp_trace`) journals a
`QualityTelemetryObservation` (compression ratio, causal density, noise
ratio, detail coverage, raw-equivalent tokens, rpc/about/role, timestamp)
into `<data-dir>/telemetry/quality.redb` — a **separate, fail-open, bounded**
journal (retention-capped; overflow drops observations and counts them, the
kernel is never affected; relaxed durability, so the tail may be lost on
crash — memory never is).

Read surface **today** (in-process Rust):
`kmp_adapter_embedded::RedbQualityTelemetryReader::open(data_dir)`
→ `query_since(millis, rpc, limit)`, `query_between(..)`, `latest(..)`,
`count()`. *Planned*: a CLI query subcommand on the binary (deliberately
**not** an MCP tool for now — the one-protocol rule forbids embedded-only
tool semantics). If MADE needs it over MCP, that requires
specifying it cross-edition first — raise it, don't add it unilaterally.

Use it to detect degrading memory quality per incident (e.g. falling
causal density → responders are logging observations without linking
causes) and to feed MADE's own incident health dashboards.

## 6. References

- Runtime guarantees: `docs/runtime-guarantees.md`
- Executable semantics: `crates/kmp-conformance` (16 scenarios × 3 backends)
- ADRs 009–014: `docs/adr/` (engine, graph, concurrency, data dir, packaging, telemetry)
- Embedded roadmap: `docs/product/kmp-embedded-edition-roadmap.md`
- MCP operations: `docs/operations/mcp-stdio.md`

## 7. Open questions for the MADE side

Answers shape the kernel backlog — reply inline or to the kernel team:

1. **Topology**: one MADE process per data dir is the v1 contract
   (single-writer). Do you need concurrent processes over one store? That
   evidence triggers the documented daemon evolution (ADR-011).
2. **Edition target**: per-node embedded memory, or a shared cluster
   kernel for team-wide incident memory? If both, the export/import bundle
   path (roadmap E6) becomes your promotion story — say so early.
3. **Telemetry access**: is the in-process reader / planned CLI enough, or
   do you need quality snapshots over MCP? The latter requires a
   cross-edition contract change (one-protocol rule) — needs a real case.
4. **Write cadence**: expected writes/sec during an active incident?
   Embedded durable ingest is ~125 ev/s worst case (fsync-bound) — fine for
   human-paced flows; automated observation streams would need batch ingest.
5. **ID scheme**: can you guarantee stable incident/step IDs so idempotency
   keys are `ingest:<incident>:<step>` and entry refs are deterministic?
6. **Client runtime**: do you already speak MCP (which client), or is
   native gRPC (`kmp-proto`) the easier path for you?
