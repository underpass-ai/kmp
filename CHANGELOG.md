# Changelog

Notable changes to KMP by Underpass. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[semantic versioning](https://semver.org/spec/v2.0.0.html).

The `v1beta1` contract has its own maturity story, tracked in
[docs/beta-status.md](docs/beta-status.md): stable for the fields that are
implemented, with deprecated fields removed in `v1`.

## [Unreleased]

### Added

- **`kmp-mcp uninstall` and `/kmp:uninstall`** — the inverse `/kmp:setup` never
  had. There was no supported way to remove an installation, or one store
  inside it, and the last instruction left to a user was `rm -rf` against paths
  they had to work out themselves: two engine copies, two stores on two
  formats, a committed bundle, a plugin cache and a prompt directory.
  It **saves the memory before it takes it** — every store is exported into the
  working directory, the file is named out loud with its event count, and the
  report says how to bring it back (`kmp-mcp import`, into an empty store,
  because import restores rather than merges). A save that fails keeps the
  store; `--purge` is how someone says they want it gone without a copy.
  The dry run is the default, `--apply` is how you say go, and a non-zero exit
  means something was left behind — so "uninstalled" is checkable rather than
  hoped for. It removes only what it printed: a binary outside your home may be
  a package manager's, a committed bundle belongs to the repository, and a host
  registration lives inside a file that is not ours. Those are named with the
  command that removes them, and left alone.

- `/kmp:doctor` counts the Codex commands instead of stopping at the
  registration. Four prompts shipped in every release and landed nowhere,
  because the installer copied a hardcoded three — and the doctor said `ok`
  the whole time, which is how it stayed hidden. It now names what is missing
  and the command that installs it.

- **`kmp-mcp document <about> [--out FILE]`** — one about, as a Markdown
  document. The kernel already held everything such a page needs and there was
  no way to get it out: a recall projection is budgeted in bytes for an
  agent's context window, and the bundle buries entry text in a `payload_json`
  string inside `changes[]`, so anyone who wanted a document wrote a throwaway
  script to pull the entries out — and wrote it again next time.
  Entries in temporal order grouped by kind, each with its own evidence beside
  it and its ref kept visible for `kernel_inspect`; relations rendered as
  prose carrying the `why`, which is the connective tissue a bare node list
  loses; supersession and contradiction in separate closing sections, because
  one is history and the other is a live disagreement.
  **Nothing in it is generated.** Ordering and grouping are rendering
  decisions; wording is not.

### Changed

- The mark lines up. Its top row lost a column to a `\` line continuation,
  which in Rust eats the newline *and* the leading whitespace of the next
  line — so the logo shipped one character out of true on the two surfaces a
  user actually meets it. A test now fails if any row starts in a different
  column.
- The tool schemas are built once instead of on every call. The strictness
  check rebuilt the whole ten-tool document — relation vocabulary included —
  to read one field of it, per call.

- **The mark reaches a user.** `/kmp:info` and `/kmp:doctor` now show the first
  block of the output verbatim before saying anything in their own words. That
  is the only place a KMP user meets the product's own face: the startup banner
  goes to stderr where the host swallows it, and nobody runs `--help` on a
  server a plugin launched. A test fails if either surface stops opening with
  the mark, and the plugin check fails if a command stops relaying it.
- The compact four-line mark is gone. It was written, documented as heading the
  sections of `info` and `doctor`, covered by its own unit test — and never
  called from anywhere outside its own file. The one-line `▌KMP▐ Backend ────`
  lockup is what actually heads those sections. Branded code that never renders
  is worse than none: it reads as done in review and is absent in use.
- A temporal `budget.max_bytes` that cannot be honoured now comes back as
  `invalid_argument` rather than `backend_error`. It is the caller's number.

- **The error `code` is produced where the failure happens.** It used to be
  reconstructed by substring-matching the English message, so anything
  containing `must` or `invalid` became `invalid_argument` — *"the store must
  be migrated before it can be opened"* arrived as the agent's fault, and the
  agent did what the skill told it not to: retried with different arguments
  against something no argument can fix. Rewording a message could silently
  change its code, and a message in another language degraded to
  `backend_error`. The kernel's own typed errors now travel out: the embedded
  path maps `ApplicationError` and `PortError`, the gRPC path maps the status
  code the server already sent. Nothing reads the message.
- **`conflict` exists.** The documentation tells agents that a conflicting
  retry means the write already landed and is success — and no code could say
  so, because a conflict arrived as `backend_error`, the same word as a
  corrupt store. The advice could not be followed.
- The six codes are enumerated in `tools/list` under `_meta."kmp/errorCodes"`,
  each with what to do about it. They existed only in the source, while the
  skill told agents to read them — advice with nothing behind it in any host
  that does not ship the skill.
- **`additionalProperties: false` is enforced.** All ten tools declared it and
  nothing applied it, which made the surface a silent-failure generator: a
  misspelled `dimensions`, a `budget` key one level too deep, a `from` sent to
  `kernel_goto` where the cursor is `at` — each accepted, dropped, and
  answered with a well-formed success built from defaults, so the agent read
  the result as proof its arguments were understood. Unknown keys are now
  refused, named, and given their path. The hand-written `kernel_ask.prefer`
  rejection is gone with it: that branch was only reachable because the
  declared strictness was not applied.

- **A memory cannot be written from the future.** `observed_at` was supplied by
  the writer and checked against nothing, and on this project's own store the
  frontier ended up three hours and twenty minutes ahead of the wall clock:
  agents wrote local time with a `Z`, which RFC3339 permits, so nothing was out
  of spec and nothing complained. The read path is ordered by that field, so
  `kernel_forward` from a correct present returned nothing while unread entries
  sat above it — an empty delta that looks exactly like a quiet week. A stamp
  more than five minutes ahead of the kernel's clock is now refused at write
  time, saying how far out it is and naming the cause. Earlier times are
  untouched: a backfill is legitimate. Nothing already written is edited — the
  record says what it said.
- **The temporal verbs apply the budget they advertise.** `kernel_goto`,
  `kernel_near`, `kernel_rewind` and `kernel_forward` declared the full
  `budget` object — `max_bytes` among it, described as normative — and
  enforced none of it: a `max_bytes: 9000` request came back at 17.3 KB, past
  the caller's ceiling and past the tool's own published
  `anthropic/maxResultSizeChars` of 10,000. Two limits, both advertised,
  neither applied. Every one of these verbs exists to be called by a model
  with a finite context, and an oversized result does not degrade a turn, it
  ends it — so declaring a limit and not keeping it is worse than declaring
  none, because the agent plans around the number and takes no precautions of
  its own. Entries are now dropped from the far end until the response fits,
  and `page.returned`, `page.total` and `page.has_more` say so. Both the
  embedded and the gRPC paths, which had the same gap.

- **`kernel_ask` stops claiming support it has not established.** The `answer`
  field opened "Memory answer supported by cited evidence" — a claim the
  kernel cannot make: it retrieves by term overlap, and whether those items
  answer the question is a judgement it does not perform. It now says what it
  did and points at `proof.evidence[].text`, where the reading belongs to the
  caller. For a kernel whose whole claim is that it does not generate,
  asserting unestablished support is the one thing it must never do.
- `UNKNOWN` now says which kind it is. `summary` distinguishes *nothing was
  retrieved* from *things were retrieved and none of them bear on this*, and
  `proof.missing` follows — two situations that lead to different next moves,
  and only one of them means the memory has not been written yet. An UNKNOWN
  answer no longer ships five citations beside it.
- **The tool surface explains itself.** `kernel_ask` says what
  `proof.confidence` measures (lexical term overlap with the best-matching
  evidence item — not a judgement that the evidence answers, and not the
  `confidence` on a relation, which is writer certainty). `kernel_write_memory`
  says it is the writer to reach for and `kernel_ingest` the low-level form.
  The four temporal verbs each name their own cursor parameter, since they are
  taught as one family and three of the four differ. `kernel_goto` stops
  advertising `dimensions` as if the other three lacked it.
- `observed_at` says **UTC**. RFC3339 alone permits an offset, so writers
  sending local wall-clock time with a `Z` were not out of spec — and put the
  memory's frontier hours into the future.
- The agent skill stops teaching a `kernel_goto` default of 50 entries. There
  is no default; the only `50` was in a unit test.

- **The default is the embedded kernel.** `kmp-mcp` with nothing set serves
  memory in-process; no variable, no endpoint, no flag. It used to default to
  gRPC and then exit 2 asking for an endpoint, blaming a variable the user
  never set and offering gRPC or fixture — never `embedded`, the mode the
  product actually is. Every host worked only because the plugin launcher
  pasted `KMP_MCP_BACKEND=embedded` over the default before exec.
  An endpoint already in the environment still chooses gRPC, which is how the
  cluster edition has always been selected, and `KMP_MCP_BACKEND` still
  settles it outright.
- `kmp-mcp info` no longer warns "no backend selected in this shell". With a
  default there is nothing unselected — it was a fossil of the
  Kubernetes-first days, and it was the second thing a stranger read.

### Added

- **`/kmp:info`** — what this install is and which memory this project opens.
  The binary has had `kmp-mcp info` since it had a CLI and no command mirrored
  it, so finding out which store you were on meant running a diagnostic. The
  `chosen by:` line is what it leads with: the store resolves from the working
  directory, so the same command run elsewhere opens another memory.
- **[`plugins/kmp/VOICE.md`](plugins/kmp/VOICE.md)** — the shape and the
  register KMP speaks in, in one place, with worked examples. Every command
  carries its block verbatim and `scripts/ci/kmp-plugin-voice.sh` fails the
  build when one drifts. A standard nobody enforces lasts until the next
  command is written in a hurry.
- Codex reaches parity. `kmp-setup` and `kmp-info` were missing, and the
  installer only ever copied three of the nine prompts, so `/kmp-save` and
  `/kmp-catchup` existed for Claude Code users and silently did not for Codex
  ones. Both hosts now offer the same nine, and the check fails if they
  diverge again.
- **The viewer comes up on its own.** Every embedded session serves it at
  `http://127.0.0.1:7317/` with nothing set, nothing installed and no flag.
  It had shipped inside every binary since 0.1.9, gated behind a variable that
  nothing set — not the plugin launcher, not the host recipes — so the best
  thing KMP has to show was reachable by nobody.
- The first memory a session writes hands back the link to it, once. That is
  the first moment there is anything to look at, and a link repeated on every
  write is a link nobody reads.
- `kmp-mcp info`, `kmp-mcp doctor` and `/kmp:doctor` all say where the viewer
  is. The plugin doctor asks the port rather than trusting the configuration,
  because configuration says what was intended and only the port says what is
  true.

- `scripts/kmp-install-binary.sh` in the plugin: installs the engine whose
  version matches the plugin's own, from the release that published it,
  verified against the checksum published beside it. No Rust toolchain needed.
  A marketplace install brings text and cannot bring a compiled binary, so the
  engine arrived separately and the two drifted; `/kmp:setup` now closes that.
- A session-start hook that says, once, when the engine and the plugin
  disagree, and names `/kmp:setup`. It offers and never installs: a hook that
  changes a machine while someone opens a terminal is a surprise. It stays
  silent when the versions agree, because a hook that speaks every session is
  one people turn off.

### Changed

- `KMP_VIEWER_ADDR` unset now means the documented default instead of silence.
  `off`, `none` or an empty value declines the viewer. An address you name is
  honoured or the session fails — a typo that quietly serves nothing costs an
  afternoon — while an address the binary offered steps aside with a warning
  if the port is busy, because a port is not worth a session. Amends
  [ADR-017](docs/adr/ADR-017-embedded-memory-viewer.md), which had it off by
  default.
- `/kmp:demo` points its viewer at port 7318, since 7317 is already serving
  the memory of the project running the demo.

### Fixed

- A start that failed for any reason other than backend selection used to end
  with "set `KMP_KERNEL_GRPC_ENDPOINT`…", because the follow-up advice was
  chosen by matching the front of the error message and everything unmatched
  fell through to it. A correctly configured session whose viewer port was
  taken was told to go and configure a backend. The failure now carries
  whether choosing a backend would fix it, and a viewer that cannot bind an
  address you named says what usually holds the port and which variable frees
  it.
- `kmp-doctor` reports the way `flutter doctor`, `brew doctor` and
  `gh auth status` do: one line per area, detail only where something is
  wrong, the fix attached to the problem, and a verdict in plain words with a
  single next command. Everything it used to print is still there under
  `--verbose`.

### Fixed

- The doctor's startup-history loop ran in a subshell, so a recorded startup
  failure was counted and then thrown away with the subshell. It reads the
  same lines through a here-string now, and a failed start reaches the verdict.

## [0.1.12] - 2026-08-23

The binary can now introduce itself. `info` says what it is and which memory it
would open here; `doctor` judges the same facts and ends in a verdict, moving a
diagnosis that lived in the plugin's shell script into the tool it diagnoses.
Store formats and public contracts are unchanged.

### Added

- `kmp-mcp info` — what this binary is and which memory it would open here:
  the backend in effect, the data directory with the rule that chose it, the
  format and engine on disk, the committed bundle when the store is
  project-scoped, the tool surface, and the last startups.
- `kmp-mcp doctor` — the same facts, judged, ending in a verdict. It reads the
  layout from the filesystem and never opens the store, so a diagnostic cannot
  create a memory as a side effect nor take the single-writer lock out from
  under a live session (ADR-011).
- A mark KMP shows when it announces itself: on `help`, atop `info` and
  `doctor`, and on startup. The startup mark goes to stderr, because stdout
  carries the protocol.

### Changed

- `kmp-adapter-embedded` exposes `read_stamped_version` and
  `store_file_path_for`, so a diagnostic can read the layout without
  reimplementing where the store lives.
- `--version` output is deliberately unchanged: it is parsed by the plugin's
  doctor, and a mark on it would break the tooling that reads it.

## [0.1.9] - 2026-08-17

A delivery-hardening patch: Rust setup now survives transient download failures,
and relation materialization edge cases are enforced through the real
NATS-to-storage-to-gRPC integration path. Public contracts, store formats and
runtime behavior are unchanged.

### Added

- **Out-of-order relation delivery has an executable convergence guarantee.**
  A relation may arrive before either endpoint; when the real nodes arrive,
  they replace the placeholders without losing the edge or its properties.
  (#76)

- **Relation replay is proved idempotent and state-preserving.** Replaying the
  same logical edge updates its rationale and sequence in place instead of
  duplicating it or silently retaining stale state. (#77)

- **Unmaterialized placeholders cannot leak into recalled context.** Context
  bundles omit placeholder nodes, their incident edges and their identifiers
  from rendered output until the endpoint is real. (#78)

### Changed

- **Relation materialization is now part of the mandatory conformance gate.**
  The container-backed integration workflow executes the baseline plus the
  out-of-order, replay-idempotency and placeholder-filtering scenarios on every
  relevant change. (#75)

### Fixed

- **Rust toolchain installation tolerates transient network failures.** CI,
  packaging, distribution and release workflows share one bounded-retry setup
  sourced from `rust-toolchain.toml`, including Bash 3.2-safe empty inputs on
  macOS. This closes the remaining infrastructure cause tracked in #30. (#74)

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

[Unreleased]: https://github.com/underpass-ai/kmp/compare/v0.1.9...HEAD
[0.1.9]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.9
[0.1.8]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.8
[0.1.7]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.7
[0.1.6]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.6
[0.1.5]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.5
[0.1.4]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.4
[0.1.3]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.3
[0.1.2]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.2
[0.1.1]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.1
[0.1.0]: https://github.com/underpass-ai/kmp/releases/tag/v0.1.0
