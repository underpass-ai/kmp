# MCP Stdio Adapter (`kmp-mcp`)

One binary, one tool surface, two operating modes. The KMP tools are
identical in both — same schemas, same JSON by construction — so a client
switches modes by changing environment variables only:

- `kmp_ingest`, `kmp_write_memory`
- `kmp_wake`, `kmp_ask`
- `kmp_goto`, `kmp_near`, `kmp_rewind`, `kmp_forward`
- `kmp_trace`, `kmp_inspect`

## Modes at a glance

| | **Embedded** (primary) | **Live** (infrastructure gRPC) | Fixture (test-only) |
| --- | --- | --- | --- |
| Select with | nothing — it is the default | `KMP_KERNEL_GRPC_ENDPOINT=…` | `KMP_MCP_BACKEND=fixture` |
| Kernel runs | in-process, inside this binary | remote `KernelMemoryService` gRPC | none (canned responses) |
| Storage | one local data dir (`.kernel/`; SQLite for fresh stores, existing redb supported) | Neo4j / Valkey / NATS behind the server | none |
| `read_after_write_ready` | always `true` (synchronous projection) | `true` on live ingest | `false` |
| Requires | nothing | deployed kernel + TLS config | nothing |
| Concurrency | SQLite supports concurrent local hosts; redb remains single-process | server-side | n/a |

With no configuration the binary runs the embedded kernel — the mode the
product is. An endpoint in the environment chooses gRPC instead, which is how
the cluster edition has always been selected; `KMP_MCP_BACKEND` settles it
explicitly when both are present. Asking for `grpc` by name with no endpoint
is the one configuration that still refuses to start, and it says so.

## Embedded mode (primary)

The kernel runs inside the binary: zero infrastructure, per-project memory,
fsync-durable commits.

```bash
kmp-mcp
```

- **Data directory resolution** (ADR-012, winning rule logged at startup):
  `KMP_MCP_DATA_DIR` → project `.kernel/` (walks up to the `.git`
  root; auto-gitignored) → `$XDG_DATA_HOME/kmp/default`.
- **Layout**: `FORMAT_VERSION` (fail-fast on mismatch; names the engine),
  `store/kernel.redb` or `store/kernel.sqlite3`, `logs/` (rotating JSON
  logs; stderr also — stdout is JSON-RPC only), `telemetry/quality.redb`
  (bounded fail-open quality journal, ADR-014).
- **Storage engine** (ADR-018): shipped builds create fresh stores on SQLite
  (WAL), so several agent hosts can share them. Existing redb stores still
  open unchanged and remain single-process until explicitly migrated.

### Sharing one memory between two agent hosts

Fresh stores already use SQLite in the shipped binary. If Claude Code and
Codex CLI point at the same existing redb directory, whichever started first
owns it; SQLite (WAL mode) lets both work on the same store —
readers never block the writer, a second writer waits for the commit lock
instead of being refused.

For an existing redb store the conversion is one command:

```bash
# current crates.io and plugin builds already carry the engine
cargo install kmp-mcp
kmp-mcp share-memory           # snapshots, migrates, verifies, installs, keeps the original
# then restart both hosts
```

`share-memory` exists because doing it by hand is seven steps and three of
them are not obvious: the live store is locked by the session asking for the
migration, so it has to be snapshotted first; nothing verifies that the
migrated store still holds every event; and getting the swap order wrong
leaves you with two live stores or none. It refuses rather than guesses — no
engine in the binary, a leftover working directory, a store that already
carries the sqlite engine, a verification that does not match — and it never
deletes: the original is kept beside the new one under a
`-redb-before-share` name.

The long way, when you want each step in your own hands:

```bash
# 1. the shipped binary carries both engines
cargo install kmp-mcp
kmp-mcp --version              # -> kmp-mcp 0.1.x (store formats 1, 2 (sqlite))

# 2a. starting fresh: SQLite is selected automatically
KMP_MCP_BACKEND=embedded KMP_MCP_DATA_DIR=~/.local/share/kmp/shared kmp-mcp

# 2b. already have history: convert it — the source is left untouched
kmp-mcp migrate ~/.local/share/kmp/default ~/.local/share/kmp/shared --engine sqlite

# 3. point BOTH hosts' KMP_MCP_DATA_DIR at the shared directory
```

`KMP_MCP_BIN` remains a development/pinning override for plugin launchers; it
is no longer part of the SQLite setup because bundled binaries carry both
engines.

`KMP_MCP_ENGINE` only decides what a **fresh** directory becomes. An
existing directory always opens with the engine it was created with; asking
for a different one is refused by name (with the `migrate` command in the
message) rather than quietly opened as the other — that is how a user ends
up on the wrong engine without knowing. `/kmp:doctor` reports which engine a
store is on and whether another process has it open.

What it costs, plainly: ~5× slower point reads and ~30 % slower batched
writes than redb (both far above interactive rates), a slightly larger
binary, and a C dependency in shipped builds. What it buys: two hosts, one
memory, and 2.5× smaller stores. A `--no-default-features` binary that meets
a SQLite store refuses it by name and says the engine is not compiled; a binary
older than the layout refuses it as "newer than this binary supports". No
binary ever opens an empty store beside a real one.

### Maintenance CLI (embedded stores)

Everything is consumed as a process — memory over MCP stdio, maintenance
over CLI subcommands:

```bash
kmp-mcp --version                 # binary + the store formats it opens
kmp-mcp export                   # event log -> .kmp/memory.jsonl (project default)
kmp-mcp export memory.jsonl      # ...or an explicit path
kmp-mcp import                   # .kmp/memory.jsonl -> EMPTY store (fail-fast)
kmp-mcp import memory.jsonl      # ...or an explicit path
kmp-mcp migrate <old> <new> [--engine redb|sqlite]   # replay history into a fresh store
kmp-mcp document project:kmp     # one about -> Markdown on stdout
kmp-mcp document project:kmp --out HOW-THIS-WAS-BUILT.md
kmp-mcp snapshot create pre-release
kmp-mcp snapshot verify pre-release
kmp-mcp snapshot read pre-release kmp_goto '{"about":"project:kmp","at":{"sequence":12}}'
```

### One about, as a document

`document` renders everything stored under one about as Markdown: entries in
temporal order grouped by kind, each with its own evidence beside it and its
ref kept visible so a reader can take it back to `kmp_inspect`; relations
as prose lines carrying their `why`; and two closing sections for what was
superseded and what still contradicts, because one is history and the other
is a live disagreement.

It renders from the event log, which is the only source that carries every
entry, every `why` and every piece of evidence as written. **Nothing in it is
generated** — ordering and grouping are rendering decisions, wording is not.
The other two exits are a recall projection, which is budgeted in bytes for an
agent's context window, and the raw bundle, which buries entry text inside a
`payload_json` string.

Bundle hygiene applies unchanged: whatever is in the payloads lands in the
document, secrets included.

### Memory in the repository

With no path, `export` and `import` use `.kmp/memory.jsonl` at the project
root — the same root the data-directory rule walks up to find. The store
(`.kernel/`) stays machine state and auto-gitignored; the bundle is the event
log in one text file, and committing it is what makes a fresh clone arrive
with the project's decisions rather than an empty memory.

For a project-scoped embedded session this file is maintained, not remembered:
before a write may change the store, KMP creates a durable pending marker;
after the write succeeds, it exports the complete log through a same-directory
atomic replacement and only then clears the marker and returns success. An
ambiguous failure leaves the marker. `info` and `doctor` treat a missing,
invalid, older or pending bundle as a loud durability failure and tell the
operator how to repair it. For a pending marker, stop other KMP sessions, run
`kmp-mcp export`, inspect the recovered diff, then explicitly acknowledge it
with `kmp-mcp export --repair-pending`; the separate acknowledgement avoids
erasing a marker owned by a concurrent SQLite writer. An idempotent retry whose
content digest is already current does not churn the header.

```bash
kmp-mcp export && git add .kmp/memory.jsonl   # explicit repair/checkpoint
kmp-mcp import                                # in a fresh clone
```

Bundle format 2 gives that saved stream an identity: `snapshot_id`,
`created_at_unix_ms`, an inclusive `event_range`, sorted `abouts`, and a
`sha256:` content digest over the exact event lines. Import and snapshot reads
verify all of it before replay. Format-1 bundles remain readable and are
upgraded by the next export; `event_format` names the portable payload and is
deliberately independent of the redb/SQLite layout.

Because it is one JSON object per line in sequence order, an append-only log
diffs as appended lines. Each line carries `requested_by` and each change its
`reason`, so a reviewer reads the rationale of every relation without leaving
the diff.

### Named snapshots and safe historical reads

Named recovery points live at `.kmp/snapshots/<name>.jsonl` and are immutable:
creating an existing name with different contents is refused.

```bash
kmp-mcp snapshot create 2026-08-24-pre-release
kmp-mcp snapshot list
kmp-mcp snapshot verify 2026-08-24-pre-release
kmp-mcp snapshot read 2026-08-24-pre-release kmp_near \
  '{"about":"project:kmp","around":{"sequence":12}}'
```

`snapshot read` verifies the digest, imports into a fresh temporary store,
runs one of the eight existing read tools, and removes that store afterwards.
It never opens, moves or replaces `.kernel/`; `kmp_ingest` and
`kmp_write_memory` are refused on this path.

Two snapshots merge only when their event streams are identical or one is an
exact prefix of the other:

```bash
kmp-mcp snapshot merge branch-a branch-b reconciled
```

That operation is a deterministic fast-forward. If both branches appended at
the same event position, KMP refuses to invent causal order.
`.gitattributes` assigns bundle JSONL files the binary merge driver so git also
surfaces the conflict instead of interleaving lines; reconcile the decision in
a live store and create a new snapshot rather than hand-editing history.

The default only applies to a project-scoped store. An explicit
`KMP_MCP_DATA_DIR` or the per-user default belongs to no repository, and both
commands say so and ask for a path rather than guessing one.

Two limits, stated rather than discovered. **Import requires an empty store**
— it is restore, not merge, because replaying a bundle over existing memory
could duplicate history or interleave two timelines, and neither has an answer
the kernel could pick for you. And **a bundle carries payloads as written**:
a secret in memory is a secret in the committed file.

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
KMP_KERNEL_GRPC_ENDPOINT=http://127.0.0.1:50051 kmp-mcp
```

HTTPS endpoints enable server TLS with system/webpki roots automatically.
Private CAs and mTLS are explicit:

```bash
KMP_KERNEL_GRPC_ENDPOINT=https://kernel.example.svc:50054 \
KMP_KERNEL_GRPC_TLS_MODE=mutual \
KMP_KERNEL_GRPC_TLS_CA_PATH=/var/run/kernel-tls/ca.crt \
KMP_KERNEL_GRPC_TLS_CERT_PATH=/var/run/kernel-tls/tls.crt \
KMP_KERNEL_GRPC_TLS_KEY_PATH=/var/run/kernel-tls/tls.key \
KMP_KERNEL_GRPC_TLS_DOMAIN_NAME=kmp-grpc \
  kmp-mcp
```

Tool → RPC binding: `kmp_ingest`/`kmp_write_memory` →
`Ingest` (write_memory compiles to canonical ingest), `kmp_wake` → `Wake`,
`kmp_ask` → `Ask`, temporal tools → `Goto`/`Near`/`Rewind`/`Forward`,
`kmp_trace` → `Trace`, `kmp_inspect` → `Inspect`.

## Tool semantics (identical in both modes)

- `kmp_ask` returns a deterministic citation-oriented answer or `UNKNOWN`;
  it never generates an LLM answer. Complete bodies occur once in
  `proof.evidence`, joined from `because[].ref` and
  `proof.path[].evidence_refs`. For a retained answer,
  `proof.matched_terms` lists the informative query terms covered by the
  selected evidence and its directly supporting semantic context, while
  `proof.matched_relations` lists the contributing relation types. These are
  eligibility explanations, not scores or internal thresholds.
- Recall output is a deterministic contract projection. Tier 0 is the answer
  or wake core plus every canonical evidence body needed to trust its refs.
  Expandable items use one total order independent of budget: semantic proof,
  additional evidence, support bookkeeping, then structural/raw detail.
  `compact ⊆ balanced ⊆ full`; richer detail adds eligible sections and never
  displaces the core.
- `budget.max_bytes` is the normative ceiling over compact serialized
  `structuredContent` and defaults to 10,000 bytes. `budget.tokens` is an
  advisory cl100k planning hint only: it can bound how much expansion KMP tries
  to include, but it cannot prove a hard ceiling for Claude or other
  tokenizers. Tool metadata advertises `_meta["anthropic/maxResultSizeChars"] =
  10000`.
- If detail remains, `projection.page` reports `returned`, `total`, `has_more`,
  and an opaque `next_cursor`, plus truthful per-section counts. Repeat the
  same recall with `page.cursor`; only page size and byte/token budgets may
  vary. The cursor binds the query, scope, detail, selected core, and response
  snapshot, so changed arguments or changed memory fail as an invalid cursor
  instead of silently recomputing a different answer.
- Temporal tools return deterministic kernel-owned traversal slices with a
  `page` object, so bounded partial reads are visible to operators and
  clients. `kmp_goto` defaults to at most 50 entries when no explicit
  `limit.entries` is supplied.
- Dimension scope is explicit and auditable: omitted → `current_about`;
  `abouts` requires a non-empty list; `all_abouts` traverses every memory
  anchor; `scope_ids` accepts local or namespaced
  `about:<about>:dimension:<id>` ids. Coordinate dimension kinds are checked
  against their declarations during ingest.
- `kmp_inspect` supports object/detail/incoming/outgoing/evidence lookup;
  `include.raw=true` returns typed raw audit refs including dimension
  coordinates. Temporal `include.raw_refs/evidence/relations` are supported.
- Entry metadata and evidence metadata/source round-trip through typed reads;
  evidence `supports` contains the refs reached by stored support relations.
- The relation vocabulary is self-documenting: `tools/list` carries, on
  `connect_to.rel` and the ingest `rel` field, a catalog generated from the
  kernel's writer spec — every type with its quality tier (rich/anemic), its
  allowed semantic classes, and when to use it. A model operating KMP reads
  the doctrine in the schema itself; see the
  [usage guide](../usage-guide.md#relations-carry-the-why) and
  [kernel-write-protocol-plan.md](../product/kernel-write-protocol-plan.md).
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
KMP_MCP_BACKEND=fixture kmp-mcp
```

## Installation

Prebuilt binaries + one-command installer: see
[embedded-release.md](embedded-release.md). From source:

```bash
cargo install kmp-mcp
# or, for the unreleased tip:
cargo install --git https://github.com/underpass-ai/kmp kmp-mcp --locked
# or, in a checkout:
cargo install --path crates/kmp-mcp --locked
```

The repository helper wraps the same install path with pinned refs
(`scripts/mcp/install-kmp-mcp.sh`, `KMP_MCP_TAG=…`). The
crate is not on crates.io yet (pending the ADR-013 branding revisit).

## Client configuration

The [KMP plugin](../../plugins/kmp/README.md) does all of this for you and
adds the discovery aids on top — a skill that teaches the agent when to reach
for memory, and `/kmp:doctor` to diagnose a setup that is not answering:

```
# Claude Code
/plugin marketplace add underpass-ai/plugins
/plugin install kmp@underpass

# Codex CLI
codex plugin marketplace add underpass-ai/plugins
codex plugin add kmp@underpass
```

The manual registrations below remain valid for hosts the plugin does not
cover, or when you explicitly want the server without the plugin. For the
complete standalone Codex workflow use
`bash scripts/mcp/install-kmp-plugin.sh --codex --standalone`.

Embedded (recommended default — per-project memory, zero infrastructure):

```toml
[mcp_servers.kmp]
command = "kmp-mcp"
env = { KMP_MCP_BACKEND = "embedded" }
```

```bash
claude mcp add kmp --scope user \
  --env KMP_MCP_BACKEND=embedded -- ~/.cargo/bin/kmp-mcp
```

Live gRPC (shared deployed kernel):

```toml
[mcp_servers.kmp]
command = "kmp-mcp"
env = { KMP_KERNEL_GRPC_ENDPOINT = "https://kernel.example.com" }
```

## Smoke tests

Embedded end-to-end (three independent processes: write → recover → audit):

```bash
bash scripts/demo/embedded_two_sessions.sh
```

Fixture / live stdio smoke:

```bash
KMP_MCP_BACKEND=fixture KMP_MCP_BIN=kmp-mcp \
  bash scripts/mcp/kmp-stdio-smoke.sh

KMP_KERNEL_GRPC_ENDPOINT=http://127.0.0.1:50051 \
KMP_MCP_BIN=kmp-mcp \
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
  | KMP_MCP_BACKEND=embedded kmp-mcp
```

One JSON-RPC response per input line; logs never touch stdout.
