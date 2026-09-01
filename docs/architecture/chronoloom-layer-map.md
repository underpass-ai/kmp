# ChronoLoom layer map — the target before fixing #463

This is the checked-in architecture map the ChronoLoom refactor requires
before any code moves, in the tradition of
[kmp-mcp-layer-map.md](kmp-mcp-layer-map.md) for
[#404](https://github.com/underpass-ai/kmp/issues/404). It covers both halves
of the shared loom implicated in
[#463](https://github.com/underpass-ai/kmp/issues/463): the **view aggregate**
in `crates/kmp-viewer/src/` and the **browser application** in
`crates/kmp-viewer/ui/`.

The refactor precedes the fix on purpose. #463's root cause is architectural:
the domain aggregate `ViewState` is also the wire format
(`skip_serializing_if` on domain fields), and the browser consumes a full
snapshot as if it were a patch inside one 2,300-line file. Separating domain
from DTO and use case from renderer makes the eventual fix a small, reviewable
diff in exactly one mapper and one use case — and makes the class of bug
(“a cleared field vanishes from the wire”) visible at the boundary that owns
it. **No behavior or wire change lands inside the refactor**: today's wire
bytes, bug included, are pinned until the fix that follows.

## The bar

The same bar as #404, held on every slice at once:

- **Hexagonal boundaries.** `domain/` and `ports/` at the center,
  `application/{dto,mappers,use_cases}` around it, `adapters/` at the edge,
  thin composition roots. Reference shapes: `crates/kmp-release/src/`,
  `crates/kmp-mcp/src/lifecycle/`.
- **DDD without primitive obsession.** A field the domain reasons about is a
  value object: a clock is a `Clock`, not a `String` checked against a
  constant array; a revision is a `ViewRevision`, not a `u64`.
- **One file = one primary type.** The 600-line monolith budget from the
  kmp-mcp gate applies as reviewed practice here.
- **DTOs only at boundaries**, with explicit mappers. `serde` derives live on
  DTOs, never on domain types; `serde_json::Value` never travels inland.
- **Coverage.** `kmp-viewer` carries no floor in
  [coverage-floors.tsv](../development/coverage-floors.tsv), so the 80% bar
  applies to the crate directly. The browser application gets the same bar
  over its pure and application layers via `node --test` coverage; renderer
  and DOM adapters are exercised by the HTTP smoke and MCP App substring
  contracts as today.
- **No behavior or wire change** inside a refactor slice. The HTTP smoke
  suite, the MCP view-tool tests and the loom-core tests stay green
  unblessed.

## Rust — the view bounded context in `kmp-viewer`

### Today

| File (lines) | What it is |
|:--|:--|
| `view_state.rs` (1200) | Twelve types in one file: value collections (`TimeRange`, `Focus`, `Projection`, `TraceSelection`, `Provenance`), the aggregate state (`ViewState`), the command (`ViewPatch`), errors, outcome, the entry record, the idempotency record, and `ViewRegistry` — storage, locking, TTL pruning, optimistic concurrency, idempotency replay, undo history and the change bell, all in the same type. Domain types carry `serde` and are the wire format. |
| `routes.rs` (657) | Memory read routes and the four view-control routes on one impl, plus query-parameter decoding free functions — the extraction the [2026-08-28 audit](conformance-audit-2026-08-28.md) row 8 already prescribed. |
| `views.rs` (580) | Already the wire DTO + mapper layer for the memory read routes. Stays; the view context gets the same treatment. |
| `lib.rs` (254) | Server composition + capability auth. Stays composition. |
| `http.rs` (343) | Hand-rolled HTTP transport. Stays a transport adapter. |
| `mcp_app.rs` (104) | MCP App HTML assembly. Stays an adapter. |

### Target

```
crates/kmp-viewer/src/view/
  domain/          one value object or aggregate concept per file:
                   clock, semantic_zoom, relation_class, view_id,
                   view_revision, actor, memory_ref, timestamp, focus_window,
                   focus, projection_settings, trace_selection, provenance,
                   view_state, view_patch, intent_digest, idempotency_key,
                   idempotency_claim, idempotency_record, view_session (the
                   aggregate root: state + history + idempotency + every
                   invariant), session_intent, session_outcome, view_error
  ports/           what the application needs, not what tokio offers:
                   view_session_store, change_bell, wall_clock,
                   overlay_catalog
  application/
    applied        the apply result: state, applied, and the unhonored
                   notes in the boundary's words — application, not domain
    commands/      what a boundary assembles to drive a use case:
                   open_view_command, apply_intent_command (never serde)
    dto/           the wire truth, serde lives here: view_state_dto,
                   focus_dto, time_range_dto, projection_dto,
                   trace_selection_dto, provenance_dto, view_intent_dto
    mappers/       explicit both ways: view_state_mapper (domain → DTO,
                   pins today's skip-on-none bytes until the #463 fix),
                   view_intent_mapper (intent DTO → domain patch, owning
                   the vocabulary refusals), intent_digest (stable identity
                   of what the caller asked)
    use_cases/     one operation per file: open_view, get_view_state,
                   apply_view_intent, undo_view_move, await_view_change
  adapters/        in_memory_sessions (Mutex<HashMap> + TTL prune),
                   tokio_change_bell, system_wall_clock,
                   static_overlay_catalog, http view routes + status mapping
  registry.rs      thin composition: the process-wide shared() wiring both
                   faces (HTTP browser, MCP tools) to one loom
```

`crates/kmp-mcp/src/serving/view_tools.rs` keeps its #404 shape and becomes
honest about its role: it maps tool-call `Value`s to the view context's
intent DTO and serializes the state DTO — it no longer serializes a domain
aggregate.

## JavaScript — the ChronoLoom application in `ui/`

### Today

| File (lines) | What it is |
|:--|:--|
| `loom-core.js` (591) | The pure domain kit (`KMP_LOOM`): clocks, lanes, extent, zoom ladder, lens, axis, diff, prism, query. DOM-free and tested in `node`. Within budget; stays. |
| `loom.js` (2305) | Everything else in one file: shared mutable state, the backend port disguised as `api()`, five use-case flows (load about/projection, agent-state application, trace, selection, view report/poll), the Pixi scene, the DOM panels, the gestures, and the composition. The god file where #463's browser half hides. |
| `mcp-app-bridge.js` (224) | The MCP Apps transport adapter behind the same `api()` port. Already an adapter; stays. |

### Target

Script-tag modules (no bundler, no npm — the CSP and vendoring rules stand):
each file one IIFE, one concern, registered on one namespace, loaded in
order by `index.html` and inlined in the same order by `mcp_app.rs`.

```
crates/kmp-viewer/ui/
  loom-core.js       pure domain kit (unchanged surface, KMP_LOOM)
  loom-state.js      KMP_APP.state — the model/view/sync state, one place,
                     with the pure window arithmetic (clampWindow) and the
                     emphasis rule (entryAlpha) beside the data they read
  loom-api.js        KMP_APP.api — the backend port: fetch adapter, with the
                     MCP bridge delegation seam it already honors
  loom-panels.js     KMP_APP.panels — DOM panel adapters (and KMP_APP.dom,
                     the shared DOM kit, which is why panels load before the
                     use cases): rail, detail, prism, diff, provenance,
                     stats, search results, the search box as an operation,
                     control wiring
  loom-viewport.js   KMP_APP.viewport — the time-window use cases: clock,
                     window, zoom rung, temporal lens, centering
  loom-data.js       KMP_APP.data — projection use cases: probe + load
                     about, load projection, observability
  loom-selection.js  KMP_APP.selection — select, reveal at Moment, trace
  loom-sync.js       KMP_APP.sync — the view-aggregate contract: open,
                     long-poll, applyAgentState, frameRefs, report, undo.
                     The #463 browser site, isolated and node-testable
  loom-scene.js      KMP_APP.scene — Pixi renderer adapter: theme and
                     palette, stage, lanes, atlas, weave, arcs, labels,
                     navigator, tooltip
  loom-gestures.js   KMP_APP.gestures — pointer, wheel, keyboard, navigator
                     drags
  loom.js            composition root: wires the namespaces and runs init()
  mcp-app-bridge.js  MCP Apps transport adapter (unchanged)
```

Cross-calls resolve late through the shared `KMP_APP` namespace, never at
load time, so the load order stays a composition decision rather than a
dependency knot. `loom-state.js`, `loom-sync.js` and the mapping halves of
`loom-api.js` run under `node --test` next to `loom-core.test.js`; the scene
and panel adapters remain pinned by the MCP App substring contracts and the
HTTP smoke suite.

## Migration order — one slice at a time

| # | Slice | Moves | Proof |
|:--|:--|:--|:--|
| 1 | This map | — | The agreed target before any code moves |
| 2 | Rust view domain | `view_state.rs` types → `view/domain/*`, invariants into `ViewSession` | `cargo test -p kmp-viewer`; every existing view test moves with its concept |
| 3 | Rust ports + adapters + application | registry becomes composition over `in_memory_sessions` + bell + clock; use cases and DTO/mappers land; `routes.rs` view handlers become the HTTP adapter; query decoding extracted | HTTP smoke unchanged; wire-parity DTO tests pin today's bytes |
| 4 | kmp-mcp view boundary | `view_tools.rs` speaks intent DTO + state DTO | `cargo test -p kmp-mcp`; tool wire unchanged |
| 5 | JS state + api + sync | state object, backend port and view-sync use cases leave `loom.js` | `node --test` over the new files; MCP App substring contracts updated with the moved lines |
| 6 | JS data + scene + panels + gestures | the remaining halves leave `loom.js`; `loom.js` becomes composition | HTTP smoke; MCP App contracts; a browser pass over the demo store |
| 7 | Coverage close | Rust crate ≥ 80% lines; JS pure + application files ≥ 80% | `cargo llvm-cov -p kmp-viewer`; `node --test --experimental-test-coverage` |

Only after slice 7 is green does the #463 fix land — as a change to
`view_state_mapper` (explicit nulls for cleared facets), `loom-sync.js`
(a snapshot reconciles, it does not patch) and their tests.
