# Changelog

Notable changes to KMP. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Detailed notes from the early release cycle are preserved in the
[pre-0.2.0 snapshot](archive/changelog/pre-0.2.0.md).

## [Unreleased]

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
  artifacts and the previous README now live under `archive/`; maintained
  Helm charts live under `distribution/`.
- The root project metadata, MCP Registry listing, MCPB manifest, published
  crate pages and rustdocs now describe the same product: local-first agent
  memory that preserves what happened, when and why.
- SQLite is documented as the fresh-store path. Existing redb stores remain
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
- Project memory gained explicit SQLite/redb engine selection and portable
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

- A storage seam, opt-in SQLite engine, redb-to-SQLite migration, project-local
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

[Unreleased]: https://github.com/underpass-ai/kmp/compare/v0.2.1...HEAD
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
