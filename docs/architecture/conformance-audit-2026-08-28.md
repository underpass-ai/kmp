# Architecture conformance audit — 2026-08-28

This is the baseline for issue #292. It records what holds, what does not and
the smallest safe order for changing it. It does not change runtime behaviour.

## Scope and method

The snapshot has 25 crate directories, 452 Rust files and 141,537 Rust lines;
ChronoLoom has 9,149 Rust and JavaScript lines. These are reproducible counts,
not estimates:

```bash
find crates -mindepth 1 -maxdepth 1 -type d | wc -l
rg --files crates -g '*.rs' | wc -l
rg --files crates -g '*.rs' | xargs wc -l | tail -1
rg --files crates/kmp-viewer -g '*.rs' -g '*.js' | xargs wc -l | tail -1
```

`kmp-testkit` is in scope. It is a workspace library with reusable adapters,
evaluators and dataset models even though it is not published
(`crates/kmp-testkit/Cargo.toml:6-19`; `crates/kmp-testkit/src/lib.rs:24-105`).
Test bodies and private provider-response DTOs come after production seams;
their types do not cross a product boundary
(`crates/kmp-testkit/src/llm_evaluator.rs:383-466`).

| Axis | Verdict | Shortest evidence |
|:--|:--|:--|
| Hexagonal ports and adapters | Holds at the dependency boundary; outer orchestration needs splitting. | `crates/kmp-domain/Cargo.toml:17-19`; `crates/kmp-application/src/memory/service.rs:45-51`; `crates/kmp-embedded/src/kernel.rs:79-95`; `crates/kmp-mcp/src/server.rs:385-542` |
| DDD without primitive obsession | Partial. Temporal requests and relation vocabulary are typed; stored coordinates, relation input and projection policy still leak strings. | `crates/kmp-domain/src/value_objects/temporal_cursor.rs:17-83`; `crates/kmp-domain/src/value_objects/temporal_coordinate.rs:4-14`; `crates/kmp-application/src/memory/types.rs:49-97`; `crates/kmp-application/src/memory/visual_projection.rs:523-527` |
| SOLID | Dependency inversion and narrow ports hold. SRP is the active failure; OCP has deliberate closed vocabularies, and no LSP failure surfaced at the inspected ports. | `crates/kmp-application/src/memory/service.rs:28-51`; `crates/kmp-domain/src/repositories/graph_neighborhood_reader.rs:6-18`; `crates/kmp-viewer/src/routes.rs:68-130`; `crates/kmp-viewer/src/view_state.rs:59-206` |
| One file — one class | Does not hold in six production hotspots or the reusable test library. | `crates/kmp-plugin-api/src/lib.rs:31-362`; `crates/kmp-application/src/memory/types.rs:12-240`; `crates/kmp-viewer/src/views.rs:22-541`; `crates/kmp-testkit/src/memoryarena.rs:13-176` |

## One file — one class

The count is a hotspot detector. The finding is whether the declarations have
different reasons to change.

| File | Evidence | Finding |
|:--|:--|:--|
| `kmp-plugin-api/src/lib.rs` | Evidence interpretation starts at `crates/kmp-plugin-api/src/lib.rs:31`, money/date values at `crates/kmp-plugin-api/src/lib.rs:190`, plugin ports at `crates/kmp-plugin-api/src/lib.rs:253` and derivation contracts at `crates/kmp-plugin-api/src/lib.rs:270`. | Four change reasons share one 509-line root module. Split modules and keep root re-exports. |
| `kmp-application/src/memory/types.rs` | Ingest contracts start at `crates/kmp-application/src/memory/types.rs:12`, wake and ask at `crates/kmp-application/src/memory/types.rs:138`, temporal at `crates/kmp-application/src/memory/types.rs:166`, trace at `crates/kmp-application/src/memory/types.rs:181` and inspect at `crates/kmp-application/src/memory/types.rs:206`. | Five use-case contract families share one file. |
| `kmp-viewer/src/views.rs` | Graph responses start at `crates/kmp-viewer/src/views.rs:22`, inspect at `crates/kmp-viewer/src/views.rs:371`, timeline at `crates/kmp-viewer/src/views.rs:420`, trace at `crates/kmp-viewer/src/views.rs:489` and process metadata at `crates/kmp-viewer/src/views.rs:536`. | Wire views for unrelated routes share one file. |
| `kmp-domain/src/projection/events.rs` | Envelope and relation payloads are at `crates/kmp-domain/src/projection/events.rs:10-62`, three event families at `crates/kmp-domain/src/projection/events.rs:64-125`, and the handler port at `crates/kmp-domain/src/projection/events.rs:145-166`. | Data contracts, event sum type and port share one file. They are cohesive, so this is lower risk than the application and viewer hotspots. |
| `kmp-application/src/memory/visual_projection.rs` | Request policy is at `crates/kmp-application/src/memory/visual_projection.rs:14-60`, output DTOs at `crates/kmp-application/src/memory/visual_projection.rs:63-195`, and projection mechanics at `crates/kmp-application/src/memory/visual_projection.rs:206-535`. | Contract and algorithm change for different reasons. |
| `kmp-viewer/src/view_state.rs` | State DTOs are at `crates/kmp-viewer/src/view_state.rs:59-176`, transport errors/results at `crates/kmp-viewer/src/view_state.rs:179-196`, and the concurrent registry starts at `crates/kmp-viewer/src/view_state.rs:198-206`. | Aggregate state, registry, history and transport views share one file. |

`kmp-testkit` is not exempt: dataset contracts occupy
`crates/kmp-testkit/src/memoryarena.rs:13-176` and
`crates/kmp-testkit/src/memoryagentbench.rs:14-176`, while provider wire DTOs
sit beside evaluation policy at `crates/kmp-testkit/src/llm_evaluator.rs:20-217`
and `crates/kmp-testkit/src/llm_evaluator.rs:383-466`. Apply the same module
shape after production code establishes it.

## Hexagonal ports and adapters

### What holds

- The domain has only `kmp-plugin-api` and `serde` dependencies
  (`crates/kmp-domain/Cargo.toml:17-19`). The application depends inward on the
  domain and serialization/tokenization libraries, not storage or transport
  adapters (`crates/kmp-application/Cargo.toml:17-25`).
- Storage and projection seams are traits. The graph read port has two graph
  operations (`crates/kmp-domain/src/repositories/graph_neighborhood_reader.rs:6-18`),
  while detail, relationship, checkpoint, projection, snapshot and token ports
  each live behind their own traits
  (`crates/kmp-domain/src/repositories/node_detail_reader.rs:6-10`;
  `crates/kmp-domain/src/repositories/node_relationship_reader.rs:12-20`;
  `crates/kmp-domain/src/repositories/projection_checkpoint_store.rs:6-17`;
  `crates/kmp-domain/src/repositories/projection_writer.rs:6-13`;
  `crates/kmp-domain/src/repositories/snapshot_store.rs:6-17`;
  `crates/kmp-domain/src/repositories/token_estimator.rs:6-9`).
- The application facade is generic over those ports
  (`crates/kmp-application/src/memory/service.rs:28-51`). The embedded
  composition root supplies its concrete store only at wiring time
  (`crates/kmp-embedded/src/kernel.rs:79-95`).
- Infrastructure points inward: the embedded adapter owns `redb` and
  `rusqlite`, then depends on domain, application and ports
  (`crates/kmp-adapter-embedded/Cargo.toml:24-34`). No adapter type appears in
  the application facade signature
  (`crates/kmp-application/src/memory/service.rs:28-51`).
- Visual projection stays on the application facade and calls the typed
  temporal use case before projecting; it does not add a storage-specific read
  (`crates/kmp-application/src/memory/service.rs:165-177`). This preserves the
  ADR-017 same-facade rule (`archive/docs/adr/ADR-017-embedded-memory-viewer.md:32-37`).

### What needs work

- `kmp-ports` is a compatibility facade over domain-owned ports, not a second
  port model (`crates/kmp-ports/src/lib.rs:1-15`). Keep one authority; do not
  copy traits into the crate to satisfy its name.
- `kmp-mcp` is correctly outside the application boundary, but its dependency
  set combines application/domain with gRPC, TLS and the viewer
  (`crates/kmp-mcp/Cargo.toml:24-39`). `KernelMcpServer` also validates refs,
  queries projection capability, mutates shared view state and records tool
  telemetry in one method family (`crates/kmp-mcp/src/server.rs:385-542`). That
  is orchestration debt, not an inward dependency violation.
- The binary root handles CLI dispatch, logging, backend selection, viewer
  composition and the stdio loop in one file
  (`crates/kmp-mcp/src/main.rs:12-105`; `crates/kmp-mcp/src/main.rs:107-170`).
  Move those policies into focused outer modules; do not move transport into
  domain or application.

## DDD without primitive obsession

### What holds

- `TemporalAxis`, `TemporalCursor` and `TemporalWindow` are explicit domain
  types with constructor validation
  (`crates/kmp-domain/src/value_objects/temporal_cursor.rs:17-83`).
  `TemporalCoordinate::cursor_time` exhaustively maps an axis to exactly one
  clock (`crates/kmp-domain/src/value_objects/temporal_coordinate.rs:40-58`).
- Application temporal requests carry the domain direction, axis, cursor,
  dimension selection and window instead of parallel strings
  (`crates/kmp-application/src/memory/types.rs:166-178`).
- Paging now has a domain concept: `TemporalTraversalRequest` owns the entry
  limit and validates it, while `TemporalTraversalPage` owns returned/total and
  boundary flags
  (`crates/kmp-domain/src/model/temporal_memory/mod.rs:20-70`;
  `crates/kmp-domain/src/model/temporal_memory/mod.rs:182-212`). Do not replace
  this with a generic wrapper unless a second domain use case shares the same
  invariants.
- Relation vocabulary is typed by `MemoryRelationType` and
  `RelationSemanticClass`; ingest converts strings once at the application
  boundary (`crates/kmp-domain/src/value_objects/relation_type.rs:52-76`;
  `crates/kmp-application/src/memory/ingest.rs:205-216`).
- The current path algorithm returns `None` until the requested target is
  reached, and its tests cover both a complete reverse path and an unreachable
  target (`crates/kmp-domain/src/model/relationship_path.rs:11-54`;
  `crates/kmp-domain/src/model/relationship_path.rs:72-94`).

### What needs work

- `TemporalCoordinate` still stores dimension, scope and five clocks as
  `String`/`Option<String>` (`crates/kmp-domain/src/value_objects/temporal_coordinate.rs:4-14`).
  The boundary DTO repeats all five clocks as strings
  (`crates/kmp-application/src/memory/types.rs:49-68`). Introduce validated
  `MemoryDimension`, `MemoryScopeId` and `TemporalInstant` values behind the
  existing serialized contract.
- `MemoryRelationData` accepts refs, relation, class and confidence as strings
  (`crates/kmp-application/src/memory/types.rs:71-97`). Boundary strings are
  unavoidable JSON; they should not survive `translate_memory_ingest`, which
  already parses relation and class there
  (`crates/kmp-application/src/memory/ingest.rs:205-216`). Parse confidence and
  refs in that same translation.
- `RelationExplanation` types the semantic class but holds fourteen optional
  strings for rationale, provenance and temporal identity
  (`crates/kmp-domain/src/value_objects/relation_explanation.rs:6-24`). Group
  temporal identity and confidence as values. Keep rationale, motivation,
  method and evidence as prose; wrapping free text adds ceremony, not domain
  safety.
- `directed_relationship_path` exposes `Option<Vec<&BundleRelationship>>`
  (`crates/kmp-domain/src/model/relationship_path.rs:11-15`). A `ProofPath`
  value can own non-empty ordered hops and the requested endpoints, making the
  already-tested target invariant impossible to bypass.
- Visual projection translates typed coordinates and relations back to string
  DTOs (`crates/kmp-application/src/memory/visual_projection.rs:89-134`), then
  application policy counts causal relations by comparing the string
  `"causal"` (`crates/kmp-application/src/memory/visual_projection.rs:523-527`).
  Keep typed values through policy and serialize only at the output edge.

## SOLID

### Single responsibility

SRP is the main failure. `MemoryViewerServer::route` owns HTTP safety and route
selection, `answer` dispatches fourteen handlers, and the same file parses
query policy (`crates/kmp-viewer/src/routes.rs:68-130`;
`crates/kmp-viewer/src/routes.rs:471-640`). The wire parser and response writer
are already separated in `http.rs` (`crates/kmp-viewer/src/http.rs:15-225`), so
the next seam is query decoding, not a framework rewrite.

`view_state.rs` combines state, patch, errors, results, entry history and the
concurrent registry (`crates/kmp-viewer/src/view_state.rs:59-206`). Those types
have separate change reasons even though optimistic concurrency belongs to the
registry aggregate.

### Open/closed

KMP has deliberately closed protocol vocabularies. The semantic zoom input is
the three-rung enum at the MCP boundary
(`crates/kmp-mcp/src/protocol.rs:484-500`) and the application uses the matching
`VisualLevelOfDetail` enum
(`crates/kmp-application/src/memory/visual_projection.rs:18-25`). Adding a rung
must change both because it changes the public contract; that is not extension
through an untrusted string. The remaining risk is drift, so keep the protocol
contract test that pins the enum (`crates/kmp-mcp/src/protocol.rs:1687-1697`).

### Liskov substitution

No LSP failure surfaced at the inspected graph seam. `GraphNeighborhoodReader`
defines one result shape for every adapter and provides transparent `Arc<T>`
and `&T` forwarding implementations
(`crates/kmp-domain/src/repositories/graph_neighborhood_reader.rs:6-18`;
`crates/kmp-domain/src/repositories/graph_neighborhood_reader.rs:21-66`). Both
embedded and Neo4j implement that same port
(`crates/kmp-adapter-embedded/src/adapter/graph_read.rs:133`;
`crates/kmp-adapter-neo4j/src/adapter/load_neighborhood.rs:64`). Preserve their
shared conformance suite when values move behind new types.

### Interface segregation

The repository ports are narrow: node detail, relationships, projection writes,
snapshots and token estimation are separate traits
(`crates/kmp-domain/src/repositories/node_detail_reader.rs:6-10`;
`crates/kmp-domain/src/repositories/node_relationship_reader.rs:12-20`;
`crates/kmp-domain/src/repositories/projection_writer.rs:6-13`;
`crates/kmp-domain/src/repositories/snapshot_store.rs:6-17`;
`crates/kmp-domain/src/repositories/token_estimator.rs:6-9`).
`KernelMcpToolBackend` is also a narrow outer port with identity and one tool
call (`crates/kmp-mcp/src/backend.rs:29-37`). No interface split is justified
until a consumer can name a smaller capability it needs.

### Dependency inversion

The application service depends on port bounds, while the embedded composition
root constructs concrete adapters (`crates/kmp-application/src/memory/service.rs:28-51`;
`crates/kmp-embedded/src/kernel.rs:79-95`). This axis holds. The remediation
must keep constructors at composition roots and must not expose `rusqlite`,
`redb`, `tonic` or HTTP types to application signatures
(`crates/kmp-adapter-embedded/Cargo.toml:24-34`;
`crates/kmp-mcp/Cargo.toml:24-39`).

## ChronoLoom

### What holds

- `loom-core.js` is DOM-free pure logic and exposes one small namespace
  (`crates/kmp-viewer/ui/loom-core.js:1-5`;
  `crates/kmp-viewer/ui/loom-core.js:489-513`). Extend this seam.
- The browser loads only local scripts
  (`crates/kmp-viewer/ui/index.html:120-123`) under a CSP with
  `script-src 'self'` (`crates/kmp-viewer/src/http.rs:190-207`). Native ES
  modules fit that boundary.
- The shipped path does not run npm; Pixi is a hash-pinned vendored artifact
  (`crates/kmp-viewer/ui/vendor/VENDOR.md:1-18`). Keep that supply-chain shape.
- The MCP App is assembled into one self-contained document by replacing the
  local script tags with embedded source
  (`crates/kmp-viewer/src/mcp_app.rs:18-38`). Any module split must preserve
  that packaging test (`crates/kmp-viewer/src/mcp_app.rs:41-54`).

### What needs work

`loom.js` owns API calls and three mutable view-model objects
(`crates/kmp-viewer/ui/loom.js:17-415`; `crates/kmp-viewer/ui/loom.js:130-175`),
Pixi renderer globals and drawing (`crates/kmp-viewer/ui/loom.js:440-1339`),
navigation and drag state (`crates/kmp-viewer/ui/loom.js:1340-1502`), detail and
trace interaction (`crates/kmp-viewer/ui/loom.js:1503-1825`), and agent
long-poll synchronization (`crates/kmp-viewer/ui/loom.js:1993-2245`). One
script has five change reasons.

Move pure state transitions and semantic intent framing first. Move rendering
and gestures only after the new module boundary runs in both the loopback page
and self-contained MCP App. Do not add a bundler, npm install or runtime fetch;
those contradict the vendored path
(`crates/kmp-viewer/ui/vendor/VENDOR.md:1-18`) and ADR-017's dependency decision
(`archive/docs/adr/ADR-017-embedded-memory-viewer.md:21-28`).

## Ordered PR plan

Every PR runs these base gates:

```bash
bash scripts/ci/documentation-spine.sh
bash scripts/ci/mcp-registry.sh
python3 scripts/ci/kmp-capability-contract.py
bash scripts/ci/kmp-plugin-voice.sh
bash scripts/ci/kmp-plugin-install-smoke.sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The order keeps wire contracts stable while seams move underneath them. A new
crate is not needed; the current publish chain is explicit and ordered
(`scripts/ci/publish-crates.sh:1-52`).

| PR | Axis | Behaviour-preserving change | Files | Gates beyond the base set |
|:--|:--|:--|:--|:--|
| 1 | One file / SRP | Split ingest, recall, temporal, trace and inspect contracts into application modules; re-export every existing public name. | `crates/kmp-application/src/memory/types.rs`, `crates/kmp-application/src/memory/mod.rs`, new `crates/kmp-application/src/memory/*_types.rs` | `cargo test -p kmp-application -p kmp-mcp -p kmp-proto-mapping` |
| 2 | One file / SRP | Split evidence, value, plugin-port and derivation contracts behind unchanged `kmp-plugin-api` re-exports. | `crates/kmp-plugin-api/src/lib.rs`, new `crates/kmp-plugin-api/src/evidence.rs`, `values.rs`, `plugins.rs`, `derivation.rs` | `cargo test -p kmp-plugin-api -p kmp-interpretation`; `bash scripts/ci/check-publish-chain.sh` |
| 3 | One file / SRP | Separate projection envelope/payloads, events and handler port; keep serialized names unchanged. | `crates/kmp-domain/src/projection/events.rs`, `crates/kmp-domain/src/projection/mod.rs`, new `event_data.rs`, `event.rs`, `event_handler.rs` | `cargo test -p kmp-domain -p kmp-adapter-embedded -p kmp-adapter-neo4j -p kmp-transport-grpc` |
| 4 | Temporal regression shield | Extend the conformance fixture so every entry has at least two dimensions, two entries tie on an instant and an odd page limit cuts across coordinate rows. Traverse every `next_cursor` and assert that the ordered union contains the complete timeline exactly once. | `crates/kmp-conformance/src/scenarios/memory_flows.rs`, `crates/kmp-conformance/tests/*`, `crates/kmp-tests-kernel/tests/conformance_integration.rs` | `bash scripts/ci/integration-conformance.sh`; `bash scripts/ci/integration-kernel-full-journey.sh`; run against embedded and Neo4j adapters |
| 5 | DDD / primitive obsession | Add validated dimension, scope and temporal-instant values; migrate `TemporalCoordinate` internals while preserving JSON/protobuf strings at adapters. | `crates/kmp-domain/src/value_objects/*`, `crates/kmp-application/src/memory/ingest.rs`, `crates/kmp-proto-mapping/src/v1beta1/memory_mapping/*` | `cargo test -p kmp-domain -p kmp-application -p kmp-proto-mapping`; `bash scripts/ci/integration-conformance.sh`; `bash scripts/ci/integration-kernel-full-journey.sh` |
| 6 | DDD / primitive obsession | Parse relation refs and confidence at ingest and add `ProofPath` with endpoint invariants; keep MCP/protobuf response shapes unchanged. | `crates/kmp-domain/src/model/relationship_path.rs`, `crates/kmp-domain/src/value_objects/*`, `crates/kmp-application/src/memory/ingest.rs`, `crates/kmp-application/src/queries/get_context_path.rs` | `cargo test -p kmp-domain -p kmp-application -p kmp-mcp`; `bash scripts/ci/integration-kernel-full-journey.sh` |
| 7 | One file / SRP | Split viewer route DTOs by endpoint, then split aggregate state, registry and errors; preserve public re-exports and wire JSON. | `crates/kmp-viewer/src/views.rs`, `crates/kmp-viewer/src/view_state.rs`, `crates/kmp-viewer/src/lib.rs`, new `crates/kmp-viewer/src/views/*`, `view/*` | `cargo test -p kmp-viewer -p kmp-mcp`; `bash scripts/ci/embedded-binary-gates.sh` |
| 8 | SOLID / hexagonal | Extract viewer query decoding from route handlers and MCP view orchestration from server dispatch. Keep authorization, ref checks and optimistic concurrency in the same order. | `crates/kmp-viewer/src/routes.rs`, new `crates/kmp-viewer/src/query_params.rs`; `crates/kmp-mcp/src/server.rs`, new `crates/kmp-mcp/src/view_orchestration.rs` | `cargo test -p kmp-viewer --test viewer_http_smoke`; `cargo test -p kmp-mcp`; `bash scripts/ci/integration-mcp-real-kernel.sh` |
| 9 | SOLID / one file | Extract CLI parsing, startup composition and stdio serving from the binary root without changing stderr/stdout. | `crates/kmp-mcp/src/main.rs`, new `crates/kmp-mcp/src/cli.rs`, `startup.rs`, `stdio.rs` | `cargo test -p kmp-mcp`; `bash scripts/ci/kmp-plugin-smoke.sh`; `bash scripts/ci/embedded-binary-gates.sh` |
| 10 | SRP / DDD | Separate visual projection request/output contracts from projection mechanics and keep typed relation classes through metric policy. | `crates/kmp-application/src/memory/visual_projection.rs`, new `crates/kmp-application/src/memory/visual_projection/types.rs`, `metrics.rs`, `project.rs` | `cargo test -p kmp-application visual_projection`; `cargo test -p kmp-viewer -p kmp-mcp` |
| 11 | ChronoLoom SRP | Move view-model transitions and agent intent/long-poll interpretation into DOM-free native modules. Keep `loom-core.js` as the pure base. | `crates/kmp-viewer/ui/loom.js`, `crates/kmp-viewer/ui/loom-core.js`, new `view-model.js`, `agent-sync.js`, `crates/kmp-viewer/ui/index.html`, `crates/kmp-viewer/src/mcp_app.rs` | `node --test crates/kmp-viewer/ui/*.test.js`; `cargo test -p kmp-viewer mcp_app`; `cargo test -p kmp-mcp` |
| 12 | ChronoLoom SRP | Move rendering, gestures, detail and trace presentation into modules with explicit inputs; leave `loom.js` as composition only. | `crates/kmp-viewer/ui/loom.js`, new `renderer.js`, `gestures.js`, `detail.js`, `trace.js`, `crates/kmp-viewer/ui/index.html`, `crates/kmp-viewer/src/mcp_app.rs` | `node --test crates/kmp-viewer/ui/*.test.js`; `cargo test -p kmp-viewer mcp_app`; `cargo test -p kmp-mcp`; `bash scripts/demo/record-chronoloom-gifs.sh` |
| 13 | One file / test scope | Apply the established module shape to reusable dataset models and evaluator/provider DTOs in `kmp-testkit`; leave test-local fixtures beside tests. | `crates/kmp-testkit/src/memoryarena.rs`, `crates/kmp-testkit/src/memoryagentbench.rs`, `crates/kmp-testkit/src/llm_evaluator.rs`, `crates/kmp-testkit/src/graph_batch_llm.rs`, new focused modules | `cargo test -p kmp-testkit --all-targets` |

Each PR preserves the temporal answers fixed by #268, #274, #276, #280 and
#284. PR 4 exists because the older single-coordinate fixture cannot reproduce
a page boundary inside one multi-dimensional entry; PR 5 must not start until
that regression shield passes on both graph adapters. Viewer PRs also preserve
the loopback, capability and read-only boundary
recorded in `docs/embedded/README.md:93-98` and
`archive/docs/adr/ADR-017-embedded-memory-viewer.md:30-59`.
