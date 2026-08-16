# Changelog

Notable changes to KMP by Underpass. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[semantic versioning](https://semver.org/spec/v2.0.0.html).

The `v1beta1` contract has its own maturity story, tracked in
[docs/beta-status.md](docs/beta-status.md): stable for the fields that are
implemented, with deprecated fields removed in `v1`.

## [Unreleased]

## [0.1.3] - 2026-08-16

### Fixed

- The plugin launcher no longer dies on a marketplace install. It execs
  `bin/kmp-mcp` inside the plugin directory, and that path is gitignored, so
  it only exists in a release package — a marketplace install produced a
  plugin whose MCP server exited 127 telling the user to "build the local
  plugin bundle", which is not something they can do. Both launchers still
  prefer the bundled binary, since a release package pins the one that plugin
  version was tested against, and now fall back to `kmp-mcp` on `PATH`. When
  neither exists the error names both places it looked and how to get one.
- `serverInfo.name` in the MCP `initialize` response was `kmp-kmp`, an
  artifact of a blanket rename. It is now `underpass-kmp-mcp`, matching the
  sibling `underpass-made-mcp`.

### Changed

- The README opens with the two editions and the install for each host
  instead of a contributor quickstart, and leads with the plugin for Claude
  Code. New `docs/editions.md` is the canonical embedded-vs-cluster
  comparison; the operations index is grouped by edition.
- The Choreographer integration guide is now `docs/integrations/made-kmp.md`
  after the MADE rename.

## [0.1.2] - 2026-08-16

### Added

- `kmp-mcp migrate <source-dir> <destination-dir>`: the way out of the
  fail-fast rule. A store whose `FORMAT_VERSION` this binary refuses to open
  can be replayed into a new one — history first, projections rebuilt from
  it, since projections are derived state and their shape is what a format
  bump would change. The source is hashed, copied and never opened for
  writing, so redb's own crash recovery cannot touch the operator's evidence;
  the hash is verified again at the end. The destination cannot already hold
  a store, and a re-run of a finished migration says so instead of reading as
  a conflict. The result carries a receipt — source format, source sha256,
  events migrated, mutations applied, kernel version — persisted in the
  destination and readable afterwards.

  Today one store format exists, so a migration is a faithful replay; the
  translation step for a future format lands in the same module, and the
  compatibility matrix moves in the same pull request. The scaffolding ships
  tested rather than promised, including against a store stamped with an
  older format.

### Fixed

- The refusal to open an older store no longer points at a "migration tool"
  that did not exist. It names the command that does.

## [0.1.1] - 2026-08-15

### Fixed

- The crate chain publishes in an order cargo can actually resolve.
  `kmp-adapter-embedded` dev-depends on `kmp-application`, and because the
  internal pins are shared with the normal dependencies that edge carries a
  version — so cargo insisted on resolving it, and 0.1.0 stopped there with
  five crates published. `kmp-application` now goes first, and
  `check-publish-chain.sh` simulates the publish (walking the chain while
  carrying the set of crates already on the registry) instead of assuming
  dev-dependencies never matter, which is the assumption that let this
  through.

Crates published at 0.1.0 — `kmp-plugin-api`, `kmp-domain`, `kmp-ports`,
`kmp-observability`, `kmp-memory-api` — stay published at that version;
registry versions are immutable. 0.1.1 is the first version where the whole
chain, `kmp-mcp` included, is on crates.io.

## [0.1.0] - 2026-08-15

First release. The kernel and everything around it existed before this tag;
what this version adds is a way to get it.

### Distribution

- **crates.io.** `cargo install kmp-mcp` installs the MCP adapter, with the
  twelve crates behind it published in dependency order (0.1.0 published the
  first five and failed on the sixth; see 0.1.1):
  `kmp-plugin-api`, `kmp-domain`, `kmp-ports`, `kmp-observability`,
  `kmp-memory-api`, `kmp-adapter-embedded`, `kmp-application`,
  `kmp-embedded`, `kmp-proto`, `kmp-proto-mapping`, `kmp-viewer`, `kmp-mcp`.
  The server, its transport and adapters, and the test crates are marked
  `publish = false`: they are distributed as an image, not as libraries.
- **Container image and Helm chart.** `ghcr.io/underpass-ai/kmp` and
  `oci://ghcr.io/underpass-ai/charts/kmp`, both stamped with the release
  version. Pushes to `main` publish a development chart version
  (`0.1.0-main.<run>`), so a release's chart is never overwritten by an
  intermediate commit.
- **Plugin bundles.** The Codex / Claude Code plugin is packaged for Linux
  (x86_64, arm64), macOS (arm64) and Windows (x86_64) and attached to the
  GitHub release with checksums.
- **Prebuilt binaries.** `kmp-mcp` for five host targets, stripped and
  checksummed, on the release page.

### Added

- Publishing metadata and a README for every published crate.
- `scripts/release.sh` — `version` bumps the workspace, the internal
  dependency pins and the chart together; `release` tags only what already
  agrees.
- `scripts/ci/publish-crates.sh` — the crate chain, resumable and patient
  with the registry's new-crate rate limit.
- `scripts/ci/check-publish-chain.sh` — a pull-request gate that keeps the
  chain describing the workspace.
- `scripts/ci/check-vendored-contract.sh` — a pull-request gate that keeps
  the vendored proto and MCP fixtures identical to the contract they were
  copied from.

### Changed

- `kmp-proto` compiles the kernel contract from a vendored copy inside the
  crate, and `kmp-mcp` embeds its fixture responses from a vendored copy of
  the reference examples. A published crate can only ship what lives inside
  it; both copies are diffed against `api/` on every CI run.

[Unreleased]: https://github.com/underpass-ai/kmp/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.2
[0.1.1]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.1
[0.1.0]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.0
