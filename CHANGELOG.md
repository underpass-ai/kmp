# Changelog

Notable changes to KMP. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Detailed notes from the early release cycle remain available in the
[`v0.5.0` Git tree](https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/changelog/pre-0.2.0.md).

## [Unreleased]

## [0.6.0] - 2026-08-30

### Fixed

- `kmp_ingest` no longer wraps an already-namespaced `memory.dimensions[].id` a
  second time. Reads hand out the namespaced form and the agent contract says to
  copy identifiers back byte-for-byte, so doing exactly that used to open a
  parallel dimension lane with no error and no warning. The read path already
  resolved this correctly; the decision now lives once, in the identity value
  object, and a dimension owned by another about is refused instead of
  reinterpreted.
- A temporal page trimmed to fit `budget.max_bytes` no longer announces itself as
  complete. `summary` and `next_action` were computed before the trim, so a
  shortened page kept the untrimmed count and flipped `has_more` without leaving
  any cursor. The trim now restates the count, names the entry it stopped at and
  says which verb continues from there. `kmp_near` is no longer silenced by a
  guard that demanded a cursor it is designed not to have, and a budget too small
  for a single entry names the number to raise instead of returning an unwalkable
  page.

### Changed

- The MCP adapter's largest files are being split into explicit hexagonal slices.
  This release moves JSON-RPC framing, tool-result envelopes, JSON-Schema
  primitives, shared request and response shapes and the relation vocabulary out
  of `protocol.rs`; proto-to-JSON rendering and the inspect byte budget out of
  `kmp.rs`; and relation-quality policy out of `write.rs`. No advertised contract
  and no answer changed — checked-in fixtures pin every tool definition and every
  tool's answer, and an architecture gate now ratchets the remaining debt so it
  can be paid down but not grown.

## [0.5.2] - 2026-08-29

### Changed

- Setup, update and host diagnosis now run through one Rust lifecycle boundary
  with explicit domain objects, use cases, ports, adapters, mappers and
  machine-readable receipts. Shell entrypoints only locate and execute the
  binary.
- The plugin API separates domain value objects, application DTOs and plugin
  ports, while the HTTP gateway separates authentication domain objects,
  verifier ports, OIDC adapters, claim mapping and authorization use cases.
- Campaign production files no longer live in the KMP product repository.

### Fixed

- Native Claude Code and Codex installs now converge every enabled KMP
  consumer, prove the actual engine each host launches, require exact plugin
  tree parity and preserve the previous shared engine until all earlier gates
  pass.
- Project writes now lock and compare the live event stream with the committed
  bundle before SQLite changes. Doctor audits exact history parity and reports
  pending or divergent revisions as blocking failures.
- External lifecycle commands and MCP surface proofs are time-bounded without
  allowing full output pipes to deadlock the child process.

## [0.5.1] - 2026-08-29

### Changed

- Embedded KMP now presents SQLite WAL only. Runtime diagnostics, store
  inventory, plugin guidance and active documentation no longer carry the
  retired engine identity, and the nonfunctional store-migration command has
  been removed. Unsupported formats remain fail-closed and untouched behind a
  generic external export/import recovery contract.

### Fixed

- `kmp-mcp uninstall --store <absolute-path>` now scopes preview and apply to
  exactly one memory, exports it before removal, and refuses while an owning
  MCP host is live without stopping that host or disturbing other stores,
  engines, plugins or wiring.
- Release tags now bind the exact green marketplace review and publish their
  checksummed assets before that catalog becomes public, so Claude never sees
  an unclonable tag and the updater never sees a version without engine assets.

## [0.5.0] - 2026-08-29

### Added

- Setup and update deterministically seed two versioned format-2 guides:
  `guide:kmp-agent` for exact agent operations and the shorter `guide:kmp` for
  people. `open:guide` opens the human guide directly in ChronoLoom without
  replacing the agent's routing instructions.
- KMP Embedded's first campaign ships a reproducible OBS harness for a real PTY
  beside real Chromium, three release-bound MP4 stories, one README GIF
  derivative, captions, fixed-seed procedural audio and evidence gates that
  keep generated media unpublished until human review passes.

### Changed

- The `kmp-memory` router now treats visual requests as a first-class lane:
  recover memory first, then open ChronoLoom and apply semantic view intents
  with revision-aware conflict rebasing. English and Spanish request forms are
  covered by the routing contract.
- Public plugin descriptions no longer embed a tool count that can drift from
  the live surface. The GitHub, marketplace and crates.io overviews are
  synchronized from one reviewed block and release preparation checks their
  content parity.
- Release preparation now regenerates both guide audiences from the exact
  bumped `kmp-mcp` binary, rejects stale guide envelopes or empty notes, and
  refuses to tag until the public marketplace carries the matching plugin.

### Fixed

- The first-open SQLite concurrency gate isolates simultaneous store creation
  from its separate lock-contention workload, so the central multi-process
  property no longer depends on scheduler timing.

## [0.4.2] - 2026-08-28

### Changed

- `kmp-mcp export --about` can be repeated to create a verifiable format-2
  bundle for selected abouts without publishing unrelated memory from a
  shared store.
- The CLI handles `-h` and `--help` before any mutation and rejects
  flag-shaped positional paths instead of silently creating those files.
- `kmp_inspect` now returns a successful partial response with continuation
  guidance when its byte budget cannot fit every requested expansion, while
  preserving the irreducible object core.

### Fixed

- Store selection reports an explicit durability outcome when a project store
  is unusable, and `doctor` detects a maintained bundle orphaned by fallback
  to another selected store.

## [0.4.1] - 2026-08-28

### Changed

- The updater detects Codex and Claude Code independently and keeps every
  installed host's plugin aligned with the engine it launches.
- ChronoLoom assigns a port per live session, allowing concurrent agents to
  open independent viewers without stealing one another's listener.
- The bundled checkout-latency journey is executable and guarded by CI.

### Fixed

- `kmp_wake` preserves supersession markers when byte budgets trim optional
  projection detail.
- `kmp_inspect` and `kmp_trace` reject refs outside the requested about before
  inspecting or traversing them.
- Plugin guidance no longer claims the embedded SQLite store is
  single-writer.

## [0.4.0] - 2026-08-28

### Changed

- Embedded KMP now contains SQLite alone: the retired dependency, canonical
  engine, conformance target and legacy quality-journal importer are removed.
  Format-1 stores are detected and rejected without opening or changing their
  bytes; KMP 0.3.2 remains the export bridge into a portable bundle.

### Fixed

- `kmp-mcp migrate` can no longer panic on a truncated format-1 file or report
  success for an empty one. Both cases fail before a destination or scratch
  file is created and leave the source untouched.
- Rewind pages now emit entries newest-to-oldest, matching the direction in
  which continuation cursors move. Concatenating a complete rewind sweep is
  globally descending and no longer changes its apparent latest entry with
  the page size. Empty explicit-clock reads also name the absent clock.

## [0.3.2] - 2026-08-28

### Fixed

- Embedded and direct ingest now confine every caller-supplied entry,
  relation, and evidence ref to the exact about, closing the remaining path
  that could overwrite another about's entries or root anchor.
- ChronoLoom now refuses an idempotency key reused for a different view
  intent instead of silently treating the collision as a successful replay.

## [0.3.1] - 2026-08-28

### Fixed

- Uninstall rescue files now include a stable full-path identity, so memories
  in stores with the same directory name cannot overwrite one another before
  both stores are removed.
- Concurrent memory writes now return a structured, retryable conflict that
  tells agents to rebase and replay the same logical write with the same
  idempotency key, while keeping reused-key content mismatches non-retryable.
- ChronoLoom chooses semantic zoom from actual marks per lane as well as time
  per pixel, opening dense memories in Atlas while keeping sparse long windows
  at a useful level of detail.
- Recall detail tiers now change the observable expansion whenever the byte
  budget permits; the legacy token hint no longer silently suppresses every
  tier behind the stable core.
- Ask fallback configuration now rejects Chinese, Japanese, and Thai language
  tags instead of accepting retries that word-based retrieval cannot serve.
- Ask now folds diacritics for matching in German, French, Spanish, and
  Portuguese, including German `ß` → `ss`, while returning stored evidence
  byte-for-byte.
- Generated writer refs now combine a readable summary slug with the logical
  write identity, so repeated observations and long summaries with a shared
  prefix remain distinct instead of silently overwriting one another.
- Caller-supplied current and semantic-delta refs are now confined to safe
  descendants of the write's own about, preventing either entry path from
  replacing another about's entry, internal node, or root anchor.
- HTTP ingest authorization now checks every caller-chosen entry, relation and
  evidence ref, so an about-scoped token cannot write into another tenant's
  graph through the low-level batch surface.
- Markdown documents now render stored fields as quoted literals and expose
  terminal and bidirectional controls visibly, preventing evidence text from
  inventing entry sections or changing what the terminal appears to show.

## [0.3.0] - 2026-08-28

### Added

- ChronoLoom is now a shared, agent-directed view of memory. Codex and Claude
  can open it, select a decision and frame its complete proof path through
  three typed view tools; every move is named, explained and undoable, while
  the person can click, filter, pan or take control at any time.
- The README ships one reproducible 26-second capture of a real agent request
  driving the live browser, with pinned Playwright and FFmpeg source.

### Changed

- Fresh embedded memory and its bounded quality telemetry now use shareable
  SQLite unconditionally. Retired engine selectors, obsolete migration
  destinations and `share-memory` are retired; format-1 memory remains readable and
  migratable through an isolated compatibility path for the 0.3 upgrade.
  The quality journal moves to `telemetry/quality.sqlite3`, preserves retention
  and imports an existing legacy journal exactly once while leaving that source
  file intact.

### Fixed

- ChronoLoom protects every route with a process-lifetime capability, hands the
  usable link back through `kmp_view_open` and `kmp_view_get_state`, frames
  trace-only intents, and keeps semantic zoom to Atlas, Episode and Moment.
  Both doctors now tell the user to ask the agent to open the loom instead of
  advertising a bare URL that returns `401`.
- `kmp_inspect` returns the typed evidence entities that support the inspected
  ref, including their text, source, metadata and complete `supports` set.
- Zero-configuration embedded sessions write their diagnostic journal; startup
  rejects an unexpanded `~` data directory instead of creating it literally;
  and Doctor probes the plugin launcher that the host actually runs.
- Legacy-store diagnostics use the current SQLite-only migration command and
  never recommend the retired `--engine` option.

## [0.2.10] - 2026-08-28

### Changed

- Temporal reads accept an explicit occurred, observed, ingested or validity
  axis, and the writer accepts every canonical clock. The application now
  exposes a paginated level-of-detail visual projection for ChronoLoom instead
  of making the browser reconstruct whole temporal lines.
- Time-based validity reads now exclude intervals that ended at the cursor,
  treating `valid_until` as an exclusive end in every direction. Validity
  `goto` projects the interval that actually holds, while ref-cursor historical
  reads mark ended entries in `proof.expired`, independently of supersession.
- ChronoLoom adds elapsed/event-density focus+context lenses, persistent A/B
  projection diffs and time-aligned observability overlays. Hosts that
  negotiate the stable MCP Apps extension can open the self-contained loom as
  a `ui://` resource; its bulk projection stays in structured app data and out
  of model text context.
- Quality metrics now say what they measure: causal density counts only causal
  relations, while noise ratio counts nodes with no summary, detail or
  non-structural relation and never classifies identifiers by vocabulary.

- The memory viewer is redesigned end to end. The force layout survives real
  memory (degree-normalized springs, Barnes-Hut repulsion, a velocity cap,
  deterministic placement, fit-to-view) where 544 nodes used to fly off the
  canvas; big graphs open as a map of folded dimensions that expand in
  place; the whole timeline plays on a scrubbing strip that shows the graph
  as of any instant; traces render hop by hop with why, evidence and
  confidence over a gradient-inked path; search learns `kind:`/`dim:`/`id:`
  and a focus mode. The UI wears the product's visual identity — the
  gradient reserved for meaning: edge classes, the audit path, played time —
  and the pure algorithmic half lives in code-native UI assets. The loopback
  server remains GET-only and read-only; the same renderer can also be served
  as a negotiated MCP App.

- Pull-request quality gates now derive changed crates and their reverse
  dependency closure, routing documentation, adapters, containers, Helm and
  publication checks independently instead of retesting the whole workspace
  for every path. Rust tests also emit coverage during that single execution;
  the coverage gate only merges their LCOV artifacts and applies the threshold.
- Release candidates now contain the complete checksummed asset set and an
  input digest. Version tags promote those exact bytes without rebuilding;
  automatic packaging and distribution no longer run on `main`.

### Fixed

- Bounded temporal pages count whole entries instead of coordinates, and ref
  continuations preserve every entry tied at the same timestamp without
  repeats or gaps.
- Explicit-clock reads prove when a referenced entry lacks that clock rather
  than falling back to a different axis or presenting an empty result as
  unexplained absence.
- The kernel stamps absent ingest clocks and assigns the next free sequence per
  dimension scope. The writer rejects scope ids reused across dimensions before
  sending a malformed canonical ingest.
- Sequence ties use the recorded clock before a lexical ref, temporal proof
  carries supersession lifecycle, and a superseding write marks the replaced
  projection node `SUPERSEDED`.
- Ask indexes stored entry text as well as evidence and distinguishes the two
  with `proof.evidence[].metadata.proof_role`.
- Trace returns proof hops in walk order from `from`, retaining any additional
  non-chaining relation at the end instead of making every caller reconstruct
  the path.
- ChronoLoom marks fallback-only bundles as hollow and states on-canvas when no
  entry carries the selected clock. Its legends now describe the active
  projection, semantic selections survive view updates, and coarse kind totals
  count each entry once across multiple lanes.
- The release MCPB smoke validates the canonical ten memory and three view tool
  names, so adding an intentional tool cannot fail an obsolete count assertion.
- The CodeQL waiter follows the concrete `Analyze (*)` jobs and treats a
  skipped aggregate as terminal instead of timing out after successful scans.

## [0.2.9] - 2026-08-27

### Fixed

- Agent guidance now treats about ids as opaque routing identifiers, preserving
  user-supplied values byte-for-byte instead of stripping prefixes such as
  `project:`.
- Semantic Ask now permits one initial selection per language, distinguishes
  cursor pagination from retries, and treats bounded `UNKNOWN` as terminal
  instead of restarting with a larger budget or sweeping the graph.
- Manual dependency-review runs now resolve an explicit base/head range instead
  of hiding a missing-range failure annotation behind `continue-on-error`.

## [0.2.8] - 2026-08-26

### Fixed

- GitHub workflows now pin a complete reviewed inventory of external actions,
  including current Node.js 24 releases, and reject unknown actions or SHA
  drift before merge.
- Crate publication restores registry dependencies without saving or cleaning
  transient Cargo package targets, eliminating false failure annotations after
  a successful publish.
- Full-journey integration tests now wait for the complete asynchronous
  projection before their first structural assertion, removing a TLS coverage
  race without weakening the exact graph checks.

## [0.2.7] - 2026-08-26

### Fixed

- Agents now treat KMP refs as opaque identifiers and pass returned refs
  byte-for-byte instead of prefixing them with an about or reconstructing them.
- GitHub workflows now pin the Node.js 24 releases of artifact transfer and
  Rust cache actions, removing the Node.js 20 migration warnings from releases.

## [0.2.6] - 2026-08-26

### Changed

- The Claude Code plugin now ships its MCP server as `memory` instead of
  `kmp`, so the host composes `plugin:kmp:memory` rather than repeating the
  product name. The plugin segment already carries the identity; the server
  segment now says what the server is. Codex registers the server flat and
  keeps the `kmp` id, where nothing is composed and a bare `memory` would say
  less. Hand-registered Claude servers are unaffected.

### Fixed

- Doctor now counts the SQLite database, WAL and SHM files when reporting the
  physical store size, and derives `last written` from the database and WAL so
  WAL-resident commits are visible without mistaking SHM read activity for a
  memory write.

### Security

- SQLite connections now enable defensive page and schema controls, reject an
  unexpected KMP table schema before application writes, and verify store
  integrity on open. MCP initialization also states explicitly that stored
  memory is untrusted data and cannot authorize actions.

## [0.2.5] - 2026-08-26

### Fixed

- The KMP memory skill now routes from both the user's intent and each tool
  result. Current state, release history and other implicit temporal questions
  continue through temporal navigation after an unanswered semantic lookup;
  relevant pages finish before repository fallback, while consequential claims
  require inspection and claimed connections require a trace.
- The plugin routing contract now distinguishes a genuinely unanswered
  semantic question from an `UNKNOWN` that should change retrieval lanes, and
  exercises pagination, audit gates and cross-language evidence preservation.

## [0.2.4] - 2026-08-26

### Fixed

- Plugin launchers now reject automatically selected engines from another
  KMP version. A matching PATH engine safely bypasses a stale local cache;
  otherwise startup fails with the exact setup repair instead of silently
  mixing plugin and engine releases. Explicit `KMP_MCP_BIN` pins still win.
- Native Windows startup now resolves the private user store through
  `LOCALAPPDATA`, with `APPDATA` and `USERPROFILE` fallbacks, when Unix home
  variables are absent.

## [0.2.3] - 2026-08-26

### Fixed

- The Codex updater installs the matching engine into both the normal CLI
  location and the exact new plugin cache returned by `codex plugin add`.
  An engine already present in the previous cache can no longer redirect a
  later release back into that stale directory.

## [0.2.2] - 2026-08-26

### Fixed

- The published `kmp_write_memory` schema now permits the relation-free first
  write that creates a new about. Strict runtime validation still requires a
  relation for every later write, and `tools/list` explains that distinction.

## [0.2.1] - 2026-08-26

### Fixed

- The Codex updater refreshes its Git marketplace, verifies the plugin version
  returned by the host and refuses to update only the engine when the
  marketplace is stale.
- Release tagging now verifies that the public Codex marketplace already
  advertises the same KMP version, preventing stale skills and launchers from
  trailing a newly published engine.
- The plugin README links to the canonical embedded documentation from source,
  standalone release bundles and the separate Codex marketplace.

## [0.2.0] - 2026-08-26

### Added

- A new English, local-first documentation surface: quick installation, real
  interaction examples, plugin/skill/MCP ownership, FAQs, a technical
  architecture section and focused embedded and enterprise runbooks.
- A reproducible animated terminal demo and a stable vector hero built from
  KMP's own wordmark.
- A current research direction that treats papers as evolving engineering
  instruments and keeps the first unpublished paper available as historical
  evidence.
- Strict warnings-as-errors rustdoc validation for every published crate.

### Changed

- Historical documentation, Kubernetes experiments, product plans, research
  artifacts and the previous README remain available in Git history;
  maintained Helm charts live under `distribution/`.
- The root project metadata, MCP Registry listing, MCPB manifest, published
  crate pages and rustdocs now describe the same product: local-first agent
  memory that preserves what happened, when and why.
- SQLite is documented as the fresh-store path. Existing legacy stores remain
  readable through their explicitly stamped compatibility path.
- Enterprise deployment remains free and open source, but is deliberately a
  secondary path behind the local embedded experience.

### Fixed

- Zero-configuration embedded MCP sessions now mount the local viewer. The
  backend already defaulted to embedded, but viewer startup previously checked
  only for an explicit `KMP_MCP_BACKEND=embedded` value.
- The repository now carries the complete Apache-2.0 text. Container and MCPB
  distributions include `LICENSE`, `NOTICE` and third-party notices; the root
  `NOTICE` retains Tirso García Ibáñez's copyright and original-author
  attribution.
- Repository ignore rules and the Docker build context now exclude local
  stores, credentials, scratch data, archives and unrelated workspace files
  by default.
- The marketplace install smoke can validate minor and major releases whose
  patch number is zero instead of assuming every release is at least two
  patches into its current series.

## [0.1.18] - 2026-08-25

### Fixed

- The plugin updater stages its engine installer before replacing the plugin
  cache, so updating one half cannot delete the helper needed to update the
  other.

## [0.1.17] - 2026-08-25

### Fixed

- Agent memory writes commit through one validated call. Dry-run remains an
  explicit diagnostic option instead of doubling the normal write path.

## [0.1.16] - 2026-08-25

### Fixed

- Plugin, skill and MCP ownership is explicit across Codex and Claude Code.
- Temporal questions route through temporal navigation, interval starts are
  inclusive, and semantic Ask can retry in configured fallback languages
  without translating stored evidence.

## [0.1.15] - 2026-08-25

### Fixed

- The MCPB integrity pin matches the published artifact.
- Doctor recognizes Claude plugin-owned MCP registrations.
- Codex startup survives the migration from the former MCP server id.

## [0.1.14] - 2026-08-25

### Added

- The embedded backend became the zero-configuration default, with a local
  viewer, document export, uninstall, store discovery, commit-native recovery
  bundles and authenticated Streamable HTTP for cluster deployments.
- The public MCP surface was renamed to ten `kmp_*` moves and prepared for the
  official MCP Registry.

### Changed

- Recall projection became transport-neutral and pageable across embedded,
  gRPC, stdio and HTTP paths.
- Project memory gained explicit embedded-engine selection and portable
  recovery semantics.

### Fixed

- Ask evidence, temporal budgets, error codes, schema strictness, future
  timestamps, cursor stability, startup diagnostics and product branding were
  hardened with executable regressions.

## [0.1.13] - 2026-08-23

### Added

- Setup installs the engine expected by the plugin, and doctor reports binary,
  host and ownership problems with concrete repairs.

### Fixed

- Exempt structural relations no longer fail validated writes.
- Doctor reports the hosts it actually found.

## [0.1.12] - 2026-08-23

### Added

- `kmp-mcp info`, `kmp-mcp doctor` and the first rendered KMP wordmark.

## [0.1.11] - 2026-08-18

### Added

- Graph-aware evidence reranking and documentation that teaches why relation
  explanations are part of memory rather than decoration.

### Fixed

- Recall diversification preserves the graph context needed to justify an
  answer.

## [0.1.10] - 2026-08-18

### Added

- Monotone pageable recall projection with stable references and explicit
  truncation accounting.

### Fixed

- Strict answerability, paraphrase recall, retained evidence priority and TLS
  projection convergence.

## [0.1.9] - 2026-08-17

### Fixed

- Rust setup retries transient failures, and relation materialization is gated
  through out-of-order, replay and placeholder-filtering tests.

## [0.1.8] - 2026-08-17

### Fixed

- Recall payloads respect token budgets without truncating the response
  envelope into an unusable result.

## [0.1.7] - 2026-08-17

### Changed

- The memory, MCP and viewer contracts were hardened through issues #61-#68.

## [0.1.6] - 2026-08-17

### Added

- Supersession, wake cursors, startup traces and the `share-memory` migration
  path for multi-process use.

### Fixed

- Viewer timeline, replay, parameter validation and fresh-about writes.

## [0.1.5] - 2026-08-17

### Fixed

- Concurrent SQLite conversion waits for the winning migration, and operators
  can point the plugin launcher at an explicit binary.

## [0.1.4] - 2026-08-16

### Added

- A storage seam, opt-in SQLite engine, embedded-store migration, project-local
  memory, demo/catch-up/revert workflows and commit-by-default writes.

## [0.1.3] - 2026-08-16

### Changed

- Documentation introduced the embedded and cluster editions and the real
  agent installation path.

### Fixed

- Plugin launchers fall back to `kmp-mcp` on `PATH`, and release manifests are
  stamped with the release version.

## [0.1.2] - 2026-08-16

### Added

- Explicit migration for an older embedded store that the current binary
  refuses to open normally.

## [0.1.1] - 2026-08-15

### Fixed

- Published crates follow dependency order, allowing the complete install
  chain to resolve on crates.io.

## [0.1.0] - 2026-08-15

### Added

- First public KMP release: crates.io packages, prebuilt MCP binaries, plugin
  bundles, container image, Helm chart and release automation.

[Unreleased]: https://github.com/underpass-ai/kmp/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/underpass-ai/kmp/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/underpass-ai/kmp/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/underpass-ai/kmp/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/underpass-ai/kmp/compare/v0.4.2...v0.5.0
[0.4.2]: https://github.com/underpass-ai/kmp/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/underpass-ai/kmp/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/underpass-ai/kmp/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/underpass-ai/kmp/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/underpass-ai/kmp/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/underpass-ai/kmp/compare/v0.2.10...v0.3.0
[0.2.10]: https://github.com/underpass-ai/kmp/compare/v0.2.9...v0.2.10
[0.2.1]: https://github.com/underpass-ai/kmp/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/underpass-ai/kmp/compare/v0.1.18...v0.2.0
[0.1.18]: https://github.com/underpass-ai/kmp/compare/v0.1.17...v0.1.18
[0.1.17]: https://github.com/underpass-ai/kmp/compare/v0.1.16...v0.1.17
[0.1.16]: https://github.com/underpass-ai/kmp/compare/v0.1.15...v0.1.16
[0.1.15]: https://github.com/underpass-ai/kmp/compare/v0.1.14...v0.1.15
[0.1.14]: https://github.com/underpass-ai/kmp/compare/v0.1.13...v0.1.14
[0.1.13]: https://github.com/underpass-ai/kmp/compare/v0.1.12...v0.1.13
[0.1.12]: https://github.com/underpass-ai/kmp/compare/v0.1.11...v0.1.12
[0.1.11]: https://github.com/underpass-ai/kmp/compare/v0.1.10...v0.1.11
[0.1.10]: https://github.com/underpass-ai/kmp/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/underpass-ai/kmp/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/underpass-ai/kmp/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/underpass-ai/kmp/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/underpass-ai/kmp/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/underpass-ai/kmp/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/underpass-ai/kmp/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/underpass-ai/kmp/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/underpass-ai/kmp/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/underpass-ai/kmp/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.0
