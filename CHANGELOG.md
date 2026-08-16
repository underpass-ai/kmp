# Changelog

Notable changes to KMP by Underpass. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[semantic versioning](https://semver.org/spec/v2.0.0.html).

The `v1beta1` contract has its own maturity story, tracked in
[docs/beta-status.md](docs/beta-status.md): stable for the fields that are
implemented, with deprecated fields removed in `v1`.

## [Unreleased]

### Fixed

- The plugin launchers can now run a binary an operator built themselves,
  named by `KMP_MCP_BIN`. They prefer the bundled `bin/kmp-mcp` over anything
  on `PATH` — a release bundle pins the binary that plugin version was tested
  against — and that bundle is built without the sqlite engine, so
  `cargo install kmp-mcp --features sqlite` was installed and never used: the
  shared store was refused by a binary that could not open it. The variable
  selects the executable and nothing else; the backend and the kernel's own
  data-directory resolution are unchanged. `kmp-doctor` already read the same
  variable, so a doctor that diagnosed one binary while the launcher ran
  another now agrees with itself. Gated by two hosts started through the real
  launcher against one shared store. (#35)

- The Windows launcher no longer forwards host arguments to the binary. It ran
  `"%BINARY%" %*`, and a leading argument is read as a maintenance command
  (`migrate`, `--version`), so a host that passed anything would get exit 2 and
  no tools — on Windows only. The POSIX launcher already dropped them.

## [0.1.4] - 2026-08-16

### Added

- **Two agent hosts can share one memory.** The embedded store now has a
  second engine behind a storage seam
  ([ADR-018](docs/adr/ADR-018-multi-process-embedded-store.md)): WAL-mode
  SQLite, opt-in through the `sqlite` cargo feature. redb remains the default
  and the default build is unchanged — pure Rust, one file, no C toolchain,
  and nobody's existing store is touched.

  The default engine takes one process at a time, so running Claude Code and
  Codex CLI on the same project meant whichever started first owned the memory
  and the other got nothing. The concurrency spike measured it: redb admits
  one of two processes and writes 300 of 600 events; SQLite admits both and
  writes all 600, and a reader alongside a live writer saw 31,843 consistent
  snapshots.

  ```bash
  cargo install kmp-mcp --features sqlite
  kmp-mcp migrate <old-dir> <new-dir> --engine sqlite   # existing memory
  KMP_MCP_ENGINE=sqlite ...                            # a fresh store
  ```

  Costs, stated: point reads about 5× slower, batched writes about 30%
  slower — both far above interactive rates — a C dependency in the opt-in
  build, and ~1.8MB of binary. It buys 2.5× smaller stores and 10× faster
  reopen.

- **`kmp-mcp migrate --engine`** converts a store between engines by replaying
  its event log into a fresh directory. The source is left byte-for-byte as it
  was and the receipt records both layouts. Migrating *from* a SQLite source
  is refused for now with the reason: WAL keeps commits in a sidecar until
  checkpointed, so a naive file copy would silently drop the newest events.

- **`KMP_MCP_ENGINE`** chooses the engine for a *fresh* data directory. An
  existing directory always opens with the engine it was created with, and
  asking for a different one is refused by name with the migrate command in
  the message — never quietly opened as the other.

- **Memory can live in the repository.** `kmp-mcp export` and `import` with no
  path now mean `.kmp/memory.jsonl` at the project root. The store
  (`.kernel/`) stays machine state and stays gitignored; the bundle is the
  event log in one text file, so a fresh clone arrives with the project's
  decisions instead of an empty memory. Because it is one JSON object per line
  in sequence order, adding a decision is a two-line diff, and each line
  carries who wrote it and the rationale of every relation — a pull request
  that also settled three questions shows them in review.

- **An example memory, and `/kmp:demo` to load it.** The plugin ships a bundle
  of a real-shaped incident and imports it into a data directory of its own,
  never the project's. The incident contains a wrong turn on purpose: the
  obvious cause is rolled back, the rollback does not help, and the real cause
  turns out to be elsewhere. That is what makes "what did we believe at 15:05"
  worth asking.

- **`/kmp:catchup`**, `/kmp:save`, `/kmp:restore` and `/kmp:revert`, with the
  matching Codex prompts. Catching up needed no new move — `kernel_rewind`
  for the frontier and `kernel_forward` for the delta already did it, with
  parameters nobody would guess — so the commands and a new skill section make
  the patterns reachable rather than adding an eleventh move.

### Fixed

- **`kernel_write_memory` now commits.** `options.dry_run` defaulted to true,
  so every call that did not know to pass `dry_run: false` compiled the ingest,
  returned it as a preview, and wrote nothing — with `isError: false`, so an
  agent reported success and a later `kernel_wake` failed with
  `node not found`. The schema stated no default, and both the skill and the
  write-protocol doc described committing as the normal path. A tool named
  `write_memory` commits; previewing is opt-in.

- **The plugin's MCP server starts.** `.mcp.json` declared `cwd: "."` with a
  relative command, and `cwd` does not resolve to the plugin directory, so the
  host spawned the launcher from wherever the session began and got `ENOENT`.
  The plugin installed, validated and loaded its skills; only the memory never
  came up. The command is now absolute via `${CLAUDE_PLUGIN_ROOT}`.

- **`cargo install kmp-mcp --features sqlite` works.** The feature named a
  dev-dependency, which resolves inside the workspace and fails for anyone
  installing from a registry.

- The plugin marketplace moved to
  [underpass-ai/plugins](https://github.com/underpass-ai/plugins), which
  carries both Underpass plugins. `/plugin marketplace add underpass-ai/kmp`
  no longer works; use `underpass-ai/plugins`.

### Changed

- `FORMAT_VERSION` now names the store *layout* — 1 is redb, 2 is SQLite —
  rather than the logical event format, which has its own constant and is
  unchanged. A binary older than a layout refuses it as "newer than this
  binary supports" instead of creating an empty store beside the real one; a
  binary without the sqlite feature recognises layout 2 and names the feature
  to enable.
- `kmp-mcp --version` lists the layouts the build can open.
- The startup line names the engine: `kernel in-process, sqlite engine`.
- `/kmp:doctor` reports which engine a store is on, and on redb ends the
  single-writer warning with the migrate command, data directory filled in.

### Internal

- A storage seam between the kernel ports and the engine, with the 16
  conformance scenarios as the proof it is faithful: same tests, same on-disk
  layout, a 100k-event store byte-identical to before.
- A new CI job runs the conformance suite, crash recovery and a
  two-processes-one-store scenario against the SQLite engine; the default
  binary gate fails if the C dependency ever reaches the default build.
- An install-shaped plugin gate that reproduces a marketplace install — no
  bundled binary, started through `.mcp.json` from an unrelated working
  directory — and checks all ten tools answer. It fails on three defects this
  release fixes, which is why it exists.

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
