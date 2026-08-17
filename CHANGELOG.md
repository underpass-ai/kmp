# Changelog

Notable changes to KMP by Underpass. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[semantic versioning](https://semver.org/spec/v2.0.0.html).

The `v1beta1` contract has its own maturity story, tracked in
[docs/beta-status.md](docs/beta-status.md): stable for the fields that are
implemented, with deprecated fields removed in `v1`.

## [Unreleased]

## [0.1.8] - 2026-08-17

One focused recall fix: token budgets now reduce detail without erasing the
answer or the proof that supports it.

### Fixed

- **Budgeted recall preserves its semantic payload.** Oversized `kernel_ask`
  responses retain a bounded answer, one cited reason and minimal proof instead
  of falling back to a misleading summary-only packet; `kernel_wake` likewise
  retains its wake shape and resume cursor. Truncation summaries and metadata
  now report omitted items and shortened text explicitly. (#71)

## [0.1.7] - 2026-08-17

Eight operator-facing fixes from first contact through sustained use: the CLI
now explains itself, a fresh memory can actually be seeded, a broken session
is diagnosed as broken, two editor windows work on the shipped default, and
large or unrelated recalls fail safely instead of misleading the host.

### Added

- **`kmp-mcp --help` and `-h`** print the supported backends, maintenance
  commands and environment controls instead of being rejected as unknown
  commands. (#61)

### Changed

- **Fresh default embedded stores use SQLite.** Installable binaries, release
  artifacts and plugin bundles now carry the multi-process engine, so two
  editor hosts can share a new memory without rebuilding the product or
  setting engine variables. Existing redb stores remain redb by their format
  stamp and are never converted implicitly; `--no-default-features` keeps the
  pure-Rust fallback. (#64)

- **Compact MCP responses are bounded after final serialization.** Wake and
  ask apply entry limits, remove duplicate prose and structural evidence, and
  count the actual cl100k payload before returning it, so a compact packet
  reaches the model instead of being discarded by the host. (#68)

### Fixed

- **The first strict write may establish an about root.** Once that root
  exists, later strict writes still require a justified relation to known
  memory. (#62)

- **`kmp-doctor` no longer calls a tool-less session usable.** A most-recent
  startup failure or an active redb writer lock is reported as unusable, with
  the resolved data directory, logs and self-ignore state included. (#63)

- **Every data-directory path installs the same non-destructive skeleton.**
  Fresh startup, explicit directories, migration and `share-memory` preserve
  operator-owned files while ensuring logs and a self-ignoring `.gitignore`,
  so a migrated store does not appear in the enclosing repository. (#65)

- **`kernel_ask` returns `UNKNOWN` for unrelated questions.** Evidence must
  now clear a relevance floor; weak or partial support is reflected in
  `missing` and confidence rather than presenting the nearest graph node as
  an answer. (#66)

- **A transient redb startup lock no longer kills memory for the whole host
  session.** MCP initialization and tool discovery stay available while the
  embedded backend retries lazily, then recover when the competing writer
  releases the store. (#67)

## [0.1.6] - 2026-08-17

Eleven fixes found by driving the product as a user rather than as its
author: the web viewer, the memory-writing surface, and the two paths an
operator actually walks — sharing one memory between hosts, and updating.

### Added

- **`kmp-mcp share-memory`** turns the seven manual steps of moving a store to
  the shared sqlite engine into one command, with the three non-obvious ones
  handled: the live store is locked by the session asking for the migration,
  so it snapshots first; both stores must report the same event count and last
  sequence *before* the swap; and the original is kept as
  `<dir>-redb-before-share` rather than deleted. Refuses rather than guesses —
  a binary without the engine, a leftover working directory, a store already
  shareable, a verification that does not match. (#43)

- **`kernel_wake` returns `resume_cursor`**, the newest coordinate the packet
  covers. Catching up used to take three calls, the middle one a rewind whose
  only purpose was to recover a timestamp. The kernel still does not track its
  readers: the cursor is the caller's to carry. (#25)

- **`proof.superseded`** marks entries that a later one replaced, naming what
  replaced them and why. Deliberately separate from `conflicts`:
  `contradicts` says two entries disagree and both may be live, while
  `supersedes` is a lifecycle — folding them together would make every revert
  read as an unresolved disagreement. (#28)

- **`kmp-doctor` reports startup history and version drift.** It reads the
  last five starts from the log, loudly when the most recent failed, and warns
  when the plugin files and the binary are different versions. (#44, #45)

### Fixed

- **A memory server that died at startup left no trace.** The file log
  existed; what bypassed it was the startup outcome itself, which went through
  `eprintln!` and `process::exit` and never through tracing. A failed start
  left the session with no tools, the host swallowed the reason, and the
  doctor had nothing to read. Both outcomes are recorded now. (#45)

- **The first write to a fresh about was impossible.** Strict
  `kernel_write_memory` demands a relation, a relation target must exist, and
  a fresh about holds nothing — including, it turned out, its own anchor,
  which the projection materialises but the ingest never counted as a known
  ref. (#14)

- **The viewer's Timeline landed blank and Replay claimed there was nothing to
  replay** on memory holding twelve entries. `goto` and `rewind` walk by
  temporal position, `sequence` is optional at ingest, and memory written
  without one answers `0/0` — which the viewer's own test corpus never
  reproduces, because it writes a sequence on every coordinate. (#39)

- **The viewer printed the store's sort key where a date belongs** —
  `unix:101786903200:000000000` in the timeline's time column — and showed
  `SNAPSHOT pending` forever, a placeholder the embedded edition never
  replaces. (#41)

- **The viewer's budget control moved the numbers but never the picture.** It
  bounds the rendered context, not the graph; at 256 tokens the status bar
  claimed ×142.9 compression beside a graph showing every node. The control
  and its figures are named for what they bound. (#40)

- **`depth=abc` answered 200 as though it read the default** while `scope`
  and `dims` beside it refused by name, and `HEAD` answered 405 though
  RFC 9110 makes it GET without a body. (#42)

- **A test asserted an exact projection size the readiness probe never waited
  for**, so it raced: 15 of 17, everything else correct, passing on re-run.
  It now waits for the query it is about to assert. (#30, partly — the
  conformance half stays open)

- **The viewer's Replay ended with two thirds of the graph dark.** It walked
  the timeline, and a timeline holds only entries that carry a coordinate: 24
  of 68 nodes here. The other 44 are dimensions and evidence, whose time is
  the entry's — a dimension is the scope an entry was written into, evidence
  exists at the moment the entry it supports does. A step now reveals the
  entry and whatever hangs off it. (#57)

### Changed

- **Merging to main no longer re-runs the gates on a tree already proved
  green.** The rule is "skip when this tree was proved", never "trust the
  pull request": an out-of-date merge, a conflict resolved in the UI, a direct
  push, or any doubt at all still runs everything. (#31)

- **Dimension scoping is documented as what it is for**, not only as what it
  accepts. Abouts are deliberately not joined by relations — an edge would
  bake the link into the graph and unbound the frontier an about exists to
  bound — so the join lives with the reader, at read time. That reasoning did
  not exist on any surface, and its absence cost a wrongly filed issue. (#33)

## [0.1.5] - 2026-08-17

### Fixed

- Two agent hosts starting at the same instant against a store that does not
  exist yet could still lose one of them. Switching a new store into WAL takes
  a brief exclusive lock, and when the loser's connection holds a write lock
  the switch fails *immediately* — `busy_timeout`, armed before it exactly as
  [ADR-018](docs/adr/ADR-018-multi-process-embedded-store.md)'s spike
  prescribed, is never consulted for that one. The switch is now retried under
  the same bounded deadline. The spike's conclusion that "the fix is ordering,
  not retry logic" is corrected in place with the measurements. (#34)

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

[Unreleased]: https://github.com/underpass-ai/kmp/compare/v0.1.8...HEAD
[0.1.8]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.8
[0.1.7]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.7
[0.1.6]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.6
[0.1.5]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.5
[0.1.4]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.4
[0.1.3]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.3
[0.1.2]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.2
[0.1.1]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.1
[0.1.0]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.0
