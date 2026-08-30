# kmp-mcp layer map — the target for #404

This is the checked-in architecture map [#404](https://github.com/underpass-ai/kmp/issues/404)
requires before migration starts: every tracked file below `crates/kmp-mcp/src/**`,
what it is today, and the bounded context and layer it must land in. The
[2026-08-28 conformance audit](conformance-audit-2026-08-28.md) reviewed the
whole workspace; this map executes its `kmp-mcp` findings at file granularity.

A directory with an architectural name proves nothing. The three contexts that
already wear the target shape — `lifecycle/`, `plugin_notice/`, `guide/` — are
audited against the same rules as everything else.

## The bar

Every slice is held to all of these at once. A green gate proves budgets, not
architecture; the reviewer holds the rest.

- **Hexagonal boundaries.** `domain/` and `ports/` at the center,
  `application/{dto,mappers,use_cases}` around it, `adapters/` at the edge,
  thin composition roots. The reference shapes are `crates/kmp-release/src/`
  and, inside this crate, `lifecycle/`.
- **DDD without primitive obsession.** A field the domain reasons about is a
  value object, not a `String`. A request struct of 23 strings and booleans is
  the named anti-example this campaign replaces.
- **SOLID.** One use case per operation. Ports are born from application and
  domain needs, never from the shape of an external library.
- **One file = one primary type**, enforced by
  `scripts/ci/kmp-mcp-architecture-gate.sh` with reviewed exceptions only for
  trivial private helpers.
- **DTOs only at boundaries**, with explicit mappers between DTO and domain.
  `serde_json::Value` is a boundary format; it does not travel inland.
- **Coverage.** `kmp-mcp` carries no floor in
  [coverage-floors.tsv](../development/coverage-floors.tsv), so the 80% bar
  applies to the crate directly: new code arrives with its tests, tests move
  with the code they prove, and the crate never dips below 80%.
- **No behavior or wire change** inside a refactor slice.
  `tool_surface_parity`, `stdio_binary` and the write/lifecycle test suites
  stay green unblessed; mutation probes run after the final bless of a slice,
  never before.

## Bounded contexts

| Context | Owns | Today |
|:--|:--|:--|
| `contract` | What the server advertises and validates: the thirteen tool definitions, schema families, a minimal registry, argument validation | `protocol.rs` + `protocol/`, `args.rs` |
| `projection` | Kernel response → tool output: byte budgets, pagination truth, per-family mappers | `kmp.rs` + `kmp/` |
| `write` | Write intent → canonical ingest: intent domain, read-context mapper, validation and relation-quality policies, preflight and commit | `write.rs`, `write/relation_quality.rs`, `ingest.rs` |
| `lifecycle` | Setup, update, doctor/info, inventory, memory rescue, removal — one context, narrow ports | `lifecycle/` today; absorbs `uninstall.rs`, `diagnostics.rs`, `memories.rs` |
| `serving` | The MCP boundary: stdio JSON-RPC transport, backend port and its adapters, tool errors, telemetry, view tools, agent policy | `server.rs`, `backend.rs`, `embedded.rs`, `grpc.rs` + `grpc/`, `fixture.rs`, `tool_error.rs`, `observability.rs`, `view_tools.rs`, `agent_policy.rs`, `clock.rs` |
| `reader` | Person-facing read operations: document rendering, snapshots | `document.rs`, `snapshot.rs` |
| `cli` | Parsing, presentation and thin composition roots | `main.rs`, `banner.rs`, `style.rs`, `pulse.rs`, `viewer.rs` |
| `plugin_notice`, `guide` | Already-shaped contexts | audited in place |

## File-by-file map

Directories already in target shape are audited in place and listed as globs;
an audit finding demotes a file back into this table.

| File (lines) | Today | Target |
|:--|:--|:--|
| `protocol.rs` (1847) | Free functions returning `Value`; every tool definition in one file | `contract/`: one definition file per tool (thirteen), schema families, registry, validator adapter |
| `protocol/{json_rpc,relation_vocabulary,request_shape,response_shape,result,schema}.rs` | #409 slices, functions over `Value` | `contract/` schema family modules; JSON-RPC framing moves to `serving` transport |
| `args.rs` (94) | Required-argument check over `Value` | `contract/` validator adapter |
| `kmp.rs` (1782, ~1301 test) | Response projection, budgets, ~15 fns + one huge test module | `projection/`: budget value objects, one mapper per response family; tests move with their mapper; shared fixtures into `test_support` |
| `kmp/inspect_budget.rs` (346), `kmp/rendering.rs` (399) | Budgeting and rendering split out flat | `projection/` domain and adapter respectively |
| `write.rs` (1601) | Intent parsing, validation, preflight, commit in one file | `write/`: intent value objects (`about`, `observed_at`, `actor`, `current_kind`… each a type), read-context mapper, validation policies, preflight and commit use cases |
| `write/relation_quality.rs` (225) | Relation-quality policy | `write/` domain policy, kept |
| `ingest.rs` (664) | `KmpIngestPlan` of raw `String`s + arg validation | `write/` ingest-plan domain + mapper; argument shape checks join `contract` |
| `uninstall.rs` (1147) | 5 types + fs traversal inline | `lifecycle/`: removal and memory-rescue use cases over a survey port; `Piece`/`PieceKind`/`Roots` become domain files |
| `diagnostics.rs` (1220) | info/doctor: findings + styled reports | `lifecycle/`: diagnose use cases; report presentation as adapter (styled/plain parity pinned) |
| `memories.rs` (472) | Machine-wide store inventory | `lifecycle/`: inventory use case over a store-layout port; `Reach`, `Memory` become domain files |
| `server.rs` (773) | stdio JSON-RPC loop + dispatch + view orchestration | `serving/`: transport adapter; dispatch against the backend port; view orchestration its own use case |
| `backend.rs` (230, 5 types) | Port trait + enum dispatch + TLS config in one file | `serving/ports/` for the trait; selection enum and TLS config one file each |
| `embedded.rs` (658), `grpc.rs` + `grpc/**` (~1801), `fixture.rs` (115) | Backend implementations | `serving/adapters/`, one type per file; `grpc/requests/*` are mappers and say so |
| `tool_error.rs` (199, 2 types) | Error code + envelope | `serving/` domain, one type per file |
| `observability.rs` (542, 3 types) | OTel wiring + error classification | `serving/adapters/` telemetry; classification joins `tool_error` |
| `view_tools.rs` (420, 2 types) | Three ChronoLoom view tools | `serving/` use case per tool over the view port |
| `agent_policy.rs` (452) | Persistent agent guidance, storage inline | `serving/`: policy domain + store adapter (audit decides final seam) |
| `clock.rs` (168) | Stamp plausibility | `serving/` domain, likely compliant — audit |
| `document.rs` (687) | One command: about → readable document | `reader/`: use case over budgeted projection port + renderer adapter |
| `snapshot.rs` (187) | Recovery points over bundles | `reader/` use case — audit |
| `main.rs` (1561) | Parsing + composition + inline `std::fs`/`Command`/`println!` | `cli/`: thin composition root + parser; everything effectful behind the contexts above; pinned by a revived `cli_surface_parity` |
| `banner.rs` (193), `style.rs` (158), `pulse.rs` (165) | Presentation | `cli/` presentation adapters, likely compliant — audit |
| `viewer.rs` (110) | Viewer address decided once | `serving/` (binary and diagnostics both read it) — audit |
| `lib.rs` (99) | Module wiring + a test mod | Stays the crate root; test mod moves out |
| `lifecycle/**` (61 files) | Target shape | Audit in place; absorbs the three files above |
| `plugin_notice/**` (20 files), `guide/**` (17 files) | Target shape | Audit in place |

## Migration order — one slice, one PR

Small PRs, iterated as drafts on the dev loop, each landing through the full
gate. A slice is one bounded context step, never one big file split for its
own sake. Before moving any file, check who reads it by name:
`documentation-spine.sh`, `mcp-registry.sh` and `kmp-agent-routing-contract.py`
read `protocol.rs` as text today and switch to
`fixtures/contract/tools_list.json` in slice 5.

| # | Slice | Absorbs | Why this order |
|:--|:--|:--|:--|
| 1 | This map | — | The agreed target before any code moves |
| 2 | `lifecycle`: inventory | `memories.rs` | Smallest absorption into an existing shape; proves the pattern |
| 3 | `lifecycle`: removal | `uninstall.rs` | Same pattern, real ports; `survey()` leaves the use case |
| 4 | `lifecycle`: doctor/info | `diagnostics.rs` | Completes the context #404 names |
| 5 | `contract` registry | `protocol.rs`, `protocol/`, `args.rs` | Static definitions pinned byte-for-byte by parity; includes the gate path fix |
| 6 | `serving` spine | `server.rs`, `backend.rs`, adapters, `tool_error.rs`, `observability.rs`, `view_tools.rs` | Transport and dispatch behind ports before the domains that flow through them |
| 7 | `projection` | `kmp.rs`, `kmp/` | Budget value objects + per-family mappers; the test module is the real work |
| 8 | `write` | `write.rs`, `ingest.rs`, `relation_quality.rs` | The deepest domain, cut last with every pattern and net in place |
| 9 | `cli` composition | `main.rs`, presentation files | Thin root over finished contexts; revives `cli_surface_parity` |
| 10 | Closing audit | `reader`, `agent_policy`, `clock`, the shaped contexts | Every remaining file against the bar; conformance ratchet reaches zero or carries reviewed exceptions |

The debt ratchet in [kmp-mcp-conformance.tsv](kmp-mcp-conformance.tsv) may
only shrink while this map executes; refresh with
`KMP_ARCHITECTURE_BASELINE=write` only when debt has genuinely been paid.

## After — the migration's landing (2026-08-30)

Ten slices later the debt baseline in
[kmp-mcp-conformance.tsv](kmp-mcp-conformance.tsv) is **empty**: no tracked
source exceeds one primary type or 600 lines. The contexts landed as:

- **`contract/`** — handshake, one definition per tool, schema families,
  registry, validator; audited by `surface_audit` and `writer_audit`.
- **`serving/`** — the port and its future, TLS and environment, the server
  and its dispatch by concern, the embedded/gRPC/fixture adapters, telemetry
  that counts without reading, the view tools, tool errors.
- **`projection/`** — one mapper per response family, budgets beside the
  family they trim, shared `test_support`, the recall budget audit.
- **`write/`** — plan, read context, relations and their quality policy,
  writer coordinates and clock, generated-ref identity, arguments, results,
  the planner; the ingest side mirrored; audited by `planner_audit` and
  `writer_identity_audit`.
- **`lifecycle/`** — setup, update, inventory, removal, doctor and info in
  one context, one vocabulary, over narrow ports.
- **`document/`** — the reader's renderer: model, markdown discipline,
  composition, `render` as its only door.
- **`cli/` under the binary** — a fifteen-line composition root, the verb
  modules, and the stdio serve path; pinned by `cli_surface_parity`.

Audit outcomes for the crate-root files that stayed put: `banner`, `style`
and `pulse` are presentation kit used by lifecycle reports and the CLI both,
so neither context may own them; `clock`, `viewer`, `snapshot` and
`agent_policy` are single-concern, single-type files within budget whose
placement can follow the first change that needs them moved. `guide/`,
`plugin_notice/` and the pre-existing `lifecycle/` slices were spot-audited
against the same rules while absorbing their neighbors.

The nets that made this safe remain the contract for what comes next:
`tool_surface_parity`, `cli_surface_parity`, `stdio_binary`, the context
audits — and the rule that a mutation probe runs after the final bless,
because three of them found real holes on the way and each hole became a
test before the probe was re-run red.
