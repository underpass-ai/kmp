# KMP Embedded Edition Roadmap

Last updated: 2026-07-25
Status: active planning document
Milestones: E0–E3 and E6 done. E4 accepted with narrowed scope (Claude Code
and Codex verified live; OpenCode out of scope, Copilot verification parked).
E5 (distribution) is the one still open — no release tag has been cut yet.

## Product Direction

KMP Embedded is the Kernel Memory Protocol packaged as a single self-contained
binary that an agent host can launch as a local MCP stdio server, with zero
external infrastructure: no Neo4j, no Valkey, no NATS, no Kubernetes, no gRPC
endpoint.

Target use case: **context recovery plugin for coding agents** — Claude Code,
Codex CLI, OpenCode, and GitHub Copilot. The host launches the binary, the
agent operates the same KMP tool surface it would use against the
infrastructure kernel (`kmp_wake`, `kmp_ask`, `kmp_near`,
`kmp_trace`, `kmp_inspect`, `kmp_write_memory`, temporal moves), and
memory persists locally per project or per user.

The embedded edition is **the same product, not a fork**:

| Property | Cluster edition | Embedded edition |
| --- | --- | --- |
| Protocol | KMP over gRPC + MCP adapter | Same KMP semantics, MCP stdio in-process |
| Memory model | Abouts, dimensions, temporal moves, typed relations, proof | Identical — enforced by shared application core |
| Storage | Neo4j / Valkey / NATS behind ports | Embedded stores behind the same ports |
| Projection | Async, event-driven, bounded staleness | Synchronous in-process, read-after-write always true |
| Deployment | Kubernetes, mTLS, observability stack | Single binary + one local data directory |
| Audience | Teams, multi-agent fleets, production incident memory | One developer, one machine, many agent sessions |

Advantages the embedded edition must carry over from the infrastructure
edition (these are the product, not optional features):

- temporal movement (`goto`, `near`, `rewind`, `forward`) with known-at-time
  semantics;
- typed relations with rationale, evidence, and provenance;
- inspectable proof (`kmp_inspect`, trace, relation evidence) without raw
  transcripts;
- replay-safe, deduplicated, auditable writes;
- the same MCP tool schemas, so a client config can switch editions by
  changing one environment variable.

Advantages the embedded edition adds:

- read-after-write consistency by construction (synchronous projection);
- zero-deploy adoption: `cargo install` or a downloaded binary, then one MCP
  registration command;
- local-first privacy: memory never leaves the machine unless exported;
- per-project memory that lives next to the repo and travels with the
  developer.

This direction is the continuation of the existing roadmap commitment:
"Continue reducing infrastructure coupling through conformance tests and
backend-independent semantics" ([documentation-catalog.md](../documentation-catalog.md)).

## Current Assets (verified in-repo)

The embedded edition does not start from zero:

- **Hexagonal seam already exists.** All persistence is behind traits in
  `kmp-domain`, re-exported by `kmp-ports`:
  `GraphNeighborhoodReader`, `NodeDetailReader`, `NodeRelationshipReader`,
  `MemoryAboutIndexReader`, `ProjectionWriter`, `ProcessedEventStore`,
  `ProjectionCheckpointStore`, `SnapshotStore`, `ContextEventStore`,
  `TokenEstimator`.
- **In-memory port implementations exist** in
  `crates/kmp-testkit/src/in_memory_stores.rs`. They are test-grade
  (fixture lookups, partial traversal), but they prove the ports are
  implementable without Cypher or external services.
- **The MCP stdio binary exists** (`crates/kmp-mcp`) with an explicit
  backend switch (`src/backend.rs`): `live` (gRPC) and `fixture`. The embedded
  edition is a third backend, not a new binary.
- **Replay-safe projections.** Runtime guarantees already require event
  deduplication by `event_id` and upsert projections safe for replay
  ([runtime-guarantees.md](../runtime-guarantees.md)) — exactly the contract
  an embedded event log needs for crash recovery.
- **Bundle tooling** (`seed_to_bundle`, seed publisher in testkit) as the seed
  of an export/import path between editions.

## Non-Negotiables

| Requirement | Meaning for embedded |
| --- | --- |
| One protocol | No embedded-only tool semantics. A behavior difference between editions is a bug, caught by the conformance suite. |
| Kernel stays a kernel | No LLM calls, no interpretation logic in the binary. Plugins/readers stay above KMP. |
| Fail-fast | Corrupt store, locked store, or version mismatch produce explicit errors, never silent empty memory. |
| Auditable | Local event log is append-only and inspectable; a decision can be reconstructed offline. |
| Small surface | The embedded binary must not link tonic servers, neo4rs, or NATS clients. Feature-gated composition. |

## Milestone E0: Definition, Decisions, ADRs

Priority: P0
Status: **done (2026-07-21)** — delivered as five ADRs (the naming/packaging
decision got its own record):
[ADR-009](../adr/ADR-009-embedded-storage-engine.md) (storage engine: redb,
with spike results),
[ADR-010](../adr/ADR-010-embedded-graph-representation.md) (graph
representation: materialized adjacency),
[ADR-011](../adr/ADR-011-embedded-concurrency-model.md) (concurrency:
single-writer fail-fast lock),
[ADR-012](../adr/ADR-012-embedded-data-directory.md) (data directory
contract),
[ADR-013](../adr/ADR-013-embedded-packaging.md) (packaging: `embedded`
backend in `kmp-mcp`).

Goal:

Fix the decisions that shape everything downstream, each as an ADR.

Deliverables:

- **Storage engine decision.** Spike and compare for the embedded stores:
  `redb` (pure-Rust, single-file, MVCC), `fjall` (LSM), and SQLite via
  `rusqlite` (ubiquitous, but C dependency complicates static musl/Windows
  builds). Evaluate against: single-file or single-dir data layout, crash
  safety, Windows support, binary size, license.
- **Graph representation decision.** The graph port surface is
  neighborhood/projection-shaped (load neighborhood, context path, node
  detail, relations), not general Cypher — decide between materialized
  adjacency in the KV engine vs an in-memory graph (petgraph-style) rebuilt
  from the event log at startup with size limits.
- **Concurrency model decision.** Multiple agent sessions will hit the same
  store (two Claude Code windows on one repo). Options: (a) single-writer
  file lock with fail-fast for the second writer, (b) per-session store with
  merge-on-read, (c) tiny local daemon owning the store, stdio shims
  connecting to it. Recommendation to evaluate first: (a) for v1, with (c) as
  the documented evolution.
- **Data directory contract.** Per-project (`.kernel/` in repo, gitignored)
  vs per-user (XDG data dir keyed by project path), env override, precedence
  rules.
- **Edition naming and packaging.** One binary with backends
  (`kmp-mcp` gains `embedded`) vs a separate `kmp` binary. Default
  recommendation: extend `kmp-mcp`; revisit branding at E5.

Exit criteria:

- Four ADRs merged under `docs/adr/`;
- storage spike results (write throughput, reopen time, file size on a
  representative LongMemEval-scale corpus) recorded in the storage ADR.

## Milestone E1: Backend-Independent Conformance Suite

Priority: P0
Status: **done (2026-07-21)** — `crates/kmp-conformance`: 16 scenarios
expressed only in ports + application services (write→read coherence,
depth-bounded traversal, context paths, ingest idempotency, projection-event
dedup, replay safety, known-at-time temporal navigation, relation proof).
Runs green against (a) the coherent in-memory kernel store
(`kmp-testkit::InMemoryKernelStore`, new — the fixture-style stores
had no write→read coherence) in the `test` job, and (b) the containerized
Neo4j/Valkey adapters in the `integration-conformance` job. Linked as the
executable definition of storage semantics from
[runtime-guarantees.md](../runtime-guarantees.md). Port-leakage audit: clean
on Cypher/Neo4j/Valkey categories; one finding ticketed
([#128](https://github.com/underpass-ai/kmp/issues/128),
NATS publisher + `async-nats` dependency in `kmp-transport-grpc`).

This is the keystone: it is what lets the embedded edition claim "same
product" instead of "similar product", and it protects both editions from
drift forever after.

Goal:

A conformance test suite expressed only in terms of the ports and the KMP
application services, runnable against any adapter set.

Deliverables:

- `kmp-conformance` crate (or module in `kmp-tests-shared`):
  scenario-based tests covering ingest → projection → wake/ask/near/goto/
  rewind/forward/trace/inspect, idempotency, event dedup, replay, known-at-
  time reads, relation proof;
- suite wired to run against (a) the testkit in-memory stores and (b) the
  Neo4j/Valkey/NATS adapters (containerized, existing e2e infra);
- port-leakage audit: no Cypher fragments, Neo4j labels, or NATS subjects in
  request/response types above the adapters; findings fixed or ticketed.

Exit criteria:

- CI job runs the suite against both existing adapter sets and is green;
- the suite is the documented definition of KMP storage semantics
  (linked from `runtime-guarantees.md`).

## Milestone E2: Embedded Storage Adapters

Priority: P0
Status: **done (2026-07-21)** — `crates/kmp-adapter-embedded`:
`EmbeddedKernelStore` implements every persistence port on one redb file
(ADR-009 layout under the ADR-012 data dir: `FORMAT_VERSION` fail-fast +
`store/kernel.redb`), with fsync-durable commits, materialized by-source +
by-target adjacency (ADR-010), an anchor index, the append-only context
event log as source of truth, and
`rebuild_projections(derive)`/`compact_data_dir` as the replay/compaction
tooling (derivation injected from
`kmp_application::projection_mutations_for_context_event`, so the
adapter stays application-free). Exit criteria measured:
- conformance suite green (arm c, 16/16, runs in the plain `test` job);
- `kill -9` mid-write, reopen, replay: acknowledged events all survive, at
  most one in-flight event beyond, no duplicates (`tests/crash_recovery.rs`);
- 100k-event corpus (release, NVMe/btrfs): reopen+first-read **8.9ms**,
  full projection rebuild **3.2s**, durable ingest 125 ev/s with two fsync
  transactions per event, store size 816MB pre-compaction
  (`tests/store_scale.rs`, `--ignored`). Size amplification from 200k tiny
  commits is the known cost; batch ingest + compaction is the improvement
  path.
Depends on: E0 (engine decision), E1 (suite to develop against)

Goal:

`kmp-adapter-embedded`: production-grade local persistence
implementing every port, in one data directory.

Deliverables:

- append-only local event store implementing `ContextEventStore` +
  `ProcessedEventStore` (dedup by `event_id`), with crash-safe fsync policy;
- projection stores implementing `ProjectionWriter`, `NodeDetailReader`,
  `GraphNeighborhoodReader`, `NodeRelationshipReader`,
  `MemoryAboutIndexReader`, `ProjectionCheckpointStore`, `SnapshotStore` on
  the chosen engine;
- **synchronous projection runtime**: in-process equivalent of
  `kmp-server/src/projection_nats_runtime.rs` that applies projection
  mutations inline on ingest — this is what makes
  `read_after_write_ready=true` unconditional in embedded mode;
- store versioning: format version stamped in the data dir, fail-fast on
  mismatch, `--migrate` path from version N to N+1;
- compaction/replay tool: rebuild projections from the event log (recovery
  and migration story in one).

Exit criteria:

- conformance suite green on the embedded adapters;
- kill -9 during a write, reopen, replay: no data loss beyond the in-flight
  event, no duplicate application;
- store survives 100k-event corpus with documented reopen time and file size.

## Milestone E3: Embedded Backend in the MCP Binary

Priority: P0
Status: **done (2026-07-24)** — core landed 2026-07-22:
`KMP_MCP_BACKEND=embedded` runs the kernel in-process.
`kmp-embedded` composition root (ADR-012
data-dir resolution env > project `.kernel/` self-gitignoring > XDG;
single-writer fail-fast via the engine lock per ADR-011);
`kmp-proto-mapping` extracted from `transport-grpc` so the embedded
backend reuses the exact live-mode JSON path (args → proto → application →
proto → JSON: identical tool JSON by construction, no tonic server/neo4rs/nats
in the MCP binary); e2e test proves cross-session memory recovery.
The local quality-telemetry journal landed under ADR-014: bounded buffered
observer → `telemetry/quality.redb`, with OTEL dependencies compile-time
excluded from `kmp-embedded` and enforced by the dependency gate on
2026-07-24. The local log file landed with it: daily-rotating JSON lines under
`<data-dir>/logs/kmp-mcp.log.<date>`, stdout left to MCP JSON-RPC.
Both exit criteria are met — embedded passes the MCP-level e2e flows, and the
fresh-machine install → write → kill session → recover path is verified on
Linux (E4/E5 evidence). Measured: 13.18MiB stripped release binary carrying
both `live` and `embedded` backends (2026-07-25, linux x86_64, rustc 1.90.0),
against the 16MiB budget enforced by `scripts/ci/embedded-binary-gates.sh`.
That is still above the single-digit aspiration in ADR-013; the feature split
is the lever, and ADR-013 already defers the call to E5, where real
distribution artifacts make the trade-off concrete. Carried to E5 as the one
open decision, not a blocker for E3.
Depends on: E2

Goal:

`KMP_MCP_BACKEND=embedded` — the existing stdio binary runs the full
kernel in-process.

Deliverables:

- composition root (`kmp-embedded` crate or feature-gated module):
  wires `kmp-application` + embedded adapters + synchronous
  projection, no tonic/gRPC in the dependency graph;
- third backend in `kmp-mcp/src/backend.rs` next to `live` and
  `fixture`, with data-dir resolution per the E0 contract and fail-fast
  errors (locked store, corrupt store, version mismatch);
- cargo feature split so `--features embedded` (default for the installable
  binary) excludes neo4rs/NATS/tonic-server code; binary size budget recorded
  (target: single-digit MB stripped, measured not promised);
- local observability: structured log file in the data dir (rotation, no
  stdout pollution — stdout belongs to MCP JSON-RPC), optional OTLP export
  reusing `kmp-observability`;
- `kmp_ingest`/`kmp_write_memory` responses report
  `read_after_write_ready=true` in embedded mode.

Exit criteria:

- `kmp-mcp` with backend=embedded passes the MCP-level e2e flows that
  live mode passes today (fixture flows already exist as the template);
- fresh machine test: install binary, register in one MCP client, write
  memory, kill session, new session recovers it — no other process running.

## Milestone E4: Host Integrations (Claude Code, Codex, OpenCode, Copilot)

Priority: P0 (Claude Code, Codex), P1 (OpenCode, Copilot)
Status: **accepted (2026-07-23)** — initial product scope is Claude Code +
Codex + Copilot (owner decision; OpenCode parked). Claude Code TESTED live
(native `kmp_wake` in a real session recovered prior-session memory with
proof); Codex TESTED live (session recovered the incident-resolution
checkpoint); Copilot recipe documented, verification parked. Two-session
demo `scripts/demo/embedded_two_sessions.sh` green; recipes + playbook in
[embedded-hosts.md](../operations/embedded-hosts.md). First external
consumer (incident-resolution agent) migrated to embedded and unblocked.
Depends on: E3

Goal:

The plugin experience: an agent in each host recovers context from KMP with
near-zero setup, and writes memory as it works.

Deliverables:

- per-host registration recipes, tested against real host versions and kept
  in `docs/operations/embedded-hosts.md`:
  - Claude Code: `claude mcp add` config + optionally a plugin (skill +
    SessionStart hook that nudges `kmp_wake` on session open);
  - Codex CLI: `~/.codex/config.toml` MCP server entry;
  - OpenCode: MCP server entry in its config;
  - Copilot: MCP registration for VS Code / Copilot agent mode;
- **context-recovery playbook** shipped as host-facing instructions (skill /
  AGENTS.md snippet / rules file per host): when to `kmp_wake` (session
  start on a known about), when to `kmp_ask` vs `kmp_near`, when to
  `kmp_write_memory` (decisions, constraints, outcomes — not transcripts);
- concurrency behavior under two simultaneous host sessions on one store,
  matching the E0 concurrency ADR (documented, tested, explicit error UX for
  the locked case);
- quickstart demo: one repo, two sessions — session 1 records an
  architectural decision, session 2 (fresh context) recovers it with proof.

Exit criteria:

- scripted end-to-end demo passes on Claude Code and Codex CLI;
- OpenCode and Copilot recipes verified manually and documented;
- playbook reviewed against real sessions: agents choose the right tool
  without per-prompt coaching in the demo scenarios.

## Milestone E5: Distribution

Priority: P1
Status: **in progress (2026-07-24)** — release workflow (5-target matrix,
checksummed artifacts on tag, dispatch-mode verification), checksum-verified
install script printing per-host registration, and the binary↔format
compatibility matrix in
[embedded-release.md](../operations/embedded-release.md). Since 2026-07-23:
https-only transport enforced in the installer and the release curl calls, and
macOS Intel cross-compiled on the arm64 runner, which took the build matrix to
**5/5 green** on main (run 30045762267, 2026-07-23: linux x86_64/aarch64,
macOS arm64/x86_64, windows x86_64). Pending: **no release tag has been cut
yet** — that run was `workflow_dispatch`, so every target builds but the
tag-triggered path in the "a tag produces all artifacts" exit criterion is
still unexercised; macOS and Windows fresh-machine runs; the
crates.io decision (deferred until naming/branding settles); and the ADR-013
binary-size decision inherited from E3.
Depends on: E3 (can overlap E4)

Goal:

Installation is one command on every developer platform.

Deliverables:

- release CI producing prebuilt, stripped, checksummed binaries:
  linux x86_64/aarch64 (musl if the E0 engine choice allows), macOS
  arm64/x86_64, Windows x86_64;
- `cargo install kmp-mcp` path kept working (already documented in
  [mcp-stdio.md](../operations/mcp-stdio.md)); crates.io publication decision;
- install script that drops the binary and prints the per-host registration
  snippet;
- versioning policy: binary version ↔ store format version compatibility
  matrix, embedded edition released from the same tags as the kernel;
- docs: embedded quickstart linked from README and `usage-guide.md`.

Exit criteria:

- a release tag produces all artifacts automatically;
- fresh-machine install-to-first-recovered-context under 5 minutes on Linux,
  macOS, and Windows.

## Milestone E6: Parity Evidence and Promotion Path

Priority: P1
Status: **done (2026-07-24), reframed embedded-first** — with the
infrastructure edition retired as a deployment target (owner decision),
export/import is backup and portability between embedded stores, not
cluster promotion. Delivered: `kmp-mcp export/import <file>` CLI
(the event log as a JSON-Lines bundle with fail-fast header checks,
empty-store-only import, exact revision reproduction) + round-trip test
proving wake, known-at-time and relation-proof parity
(`kmp-embedded/tests/export_import_roundtrip.rs`) + live binary
smoke. Conformance for all three adapter sets already permanent in CI (E1).
Benchmark parity: **LongMemEval deterministic recall run recorded**
(2026-07-23, 470/470 full evidence hits on 500 oracle items in 112.6s —
see [longmemeval-benchmark.md](../research/longmemeval-benchmark.md));
MemoryArena/MemoryAgentBench runs remain optional follow-ups.
Closed 2026-07-24 with binary-level CLI coverage (version, export/import
round trip, error exits) and explicit fail-fast coverage for malformed
bundles. The third exit criterion — the promotion walkthrough — is **void
under the reframe**, not pending: with the infrastructure edition retired as
a deployment target there is no cluster to promote into. The remaining two
are met: round-trip parity is proven by
`export_import_preserves_wake_temporal_and_proof` (wake node ids, known-at-time
`kmp_goto`, and the `supports` relation with its evidence rationale all
survive the bundle), and benchmark parity rests on the recorded LongMemEval
run.
Depends on: E2, E3

Goal:

Prove the embedded edition is the same product, and give it a growth path
into the infrastructure edition.

Deliverables:

- benchmark runners (MemoryArena, MemoryAgentBench primary; LongMemEval
  secondary) executed against the embedded backend; scores compared with the
  infrastructure backend and recorded in `docs/research/`;
- export/import: local store → portable bundle → infrastructure kernel
  ingest, and back (grow `seed_to_bundle` into a supported
  `kernel export` / `kernel import` flow with provenance preserved);
- documented promotion story: same MCP client config, switch
  `KMP_MCP_BACKEND=embedded` → `live` + endpoint, import the bundle —
  a team graduates from laptop memory to shared incident memory without
  changing how agents use the tools;
- conformance suite in CI for all three adapter sets permanently (in-memory,
  embedded, infrastructure) as the anti-drift gate.

Exit criteria:

- benchmark parity within noise between editions on the primary benchmarks;
- round-trip export/import preserves temporal reads and relation proof on a
  conformance scenario;
- promotion walkthrough executed end-to-end at least once and documented.

## Sequencing Summary

```
E0 (decisions/ADRs)
  └── E1 (conformance suite)  ── keystone, unblocks honest parallel work
        └── E2 (embedded adapters + sync projection)
              └── E3 (embedded backend in kmp-mcp)
                    ├── E4 (host integrations: Claude Code, Codex, OpenCode, Copilot)
                    ├── E5 (distribution)
                    └── E6 (parity evidence + promotion path)
```

E0+E1 are deliberately cheap and front-loaded: they convert "embedded
edition" from a rewrite risk into an adapter project. E2 is the bulk of the
engineering. E4/E5/E6 can proceed in parallel once E3 lands.

## Risks

| Risk | Mitigation |
| --- | --- |
| Port leakage (backend assumptions above the ports) discovered late | E1 audit before E2 starts; conformance suite catches regressions. |
| Graph traversal performance without a graph database | Port surface is neighborhood-shaped, not general queries; E0 spike sizes it on a realistic corpus before committing. |
| Two editions drift apart over time | Conformance suite runs against all adapter sets in CI, permanently (E6 exit criterion). |
| Concurrent sessions corrupt or fight over the store | E0 concurrency ADR; v1 single-writer lock with explicit fail-fast; daemon evolution documented, not improvised. |
| C dependencies (SQLite) break static/Windows builds | Prefer pure-Rust engine in E0 evaluation; SQLite only if the spike shows the pure-Rust options fail requirements. |
| Binary quietly grows server dependencies | Feature-gated composition + CI check on binary size and forbidden-dependency list (E3). |
| Hosts change MCP registration details | Recipes are tested artifacts (E4 exit criteria), re-verified per release. |

## Explicitly Out Of Scope (for this roadmap)

- Any LLM, reranking model, or interpretation logic inside the binary —
  plugins and readers stay above KMP per the existing plugin boundary;
- multi-machine sync between embedded stores (the promotion path to the
  infrastructure edition is the answer to "shared memory");
- vector search API — retrieval stays graph/temporal/proof oriented; hybrid
  candidate retrieval, if adopted, arrives behind ports per roadmap item 8 of
  the documentation catalog;
- replacing the infrastructure edition — embedded is an edition of the same
  kernel, and production multi-agent memory remains the infrastructure
  edition's job.
