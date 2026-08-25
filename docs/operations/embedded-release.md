# Embedded Edition Releases (E5)

## How a release happens

Push a tag `vX.Y.Z` → the `release` workflow builds, strips and checksums
`kmp-mcp` for linux x86_64/aarch64, macOS arm64/x86_64 and Windows
x86_64, and attaches every artifact (+ `.sha256`) to the GitHub release.
`workflow_dispatch` runs the same matrix without publishing (pipeline
verification). The embedded edition releases from the same tags as the
kernel — one version for the whole product.

## Install paths

- From crates.io: `cargo install kmp-mcp` — the whole chain beneath it is
  published with every release (see [../release.md](../release.md)).
- One command: `scripts/install/install.sh` (checksum-verified download,
  prints per-host registration snippets).
- From source: `cargo install --path crates/kmp-mcp --locked`.

## Binary ↔ store format compatibility

| Binary version | `FORMAT_VERSION` read/written | On older format | On newer format |
| --- | --- | --- | --- |
| 0.1.0 – 0.1.3 | 1 | refuses to open, names `kmp-mcp migrate` | refuses to open, "upgrade the binary" |
| 0.1.4+ | 1 (redb); 2 (SQLite; fresh-store default in current shipped builds) | refuses to open, names `kmp-mcp migrate` | refuses to open, "upgrade the binary" |

`FORMAT_VERSION` names the *layout* — which engine wrote `store/` — not the
logical event format, which is 1 on both and is what bundles carry
(ADR-018). A binary built with `--no-default-features` recognises `2`
and refuses it by name, saying which feature to enable. No binary of any
version opens an empty store beside a real one.

Rules (ADR-012): the store stamps `FORMAT_VERSION` at creation; the binary
fails fast on any mismatch — never silent empty memory. A format bump
requires shipping its translation step in the same release and updating this
matrix in the same PR.

### Migrating a store the binary will not open

```bash
kmp-mcp migrate ~/.local/state/kmp/old ~/.local/state/kmp/new
{"source_format":0,"source_sha256":"1a5a…","destination_format":1,
 "events_migrated":1,"mutations_applied":11,"kernel_version":"0.1.1"}
```

### Changing engines: letting two agent hosts share one memory

The same command with `--engine sqlite` converts a redb store into a SQLite
one (ADR-018). Point both hosts' `KMP_MCP_DATA_DIR` at the new directory and
they share it — readers never block the writer, a second writer waits for the
commit lock instead of being refused. Current shipped builds carry both
engines.

```bash
kmp-mcp migrate ~/.local/share/kmp/default ~/.local/share/kmp/shared --engine sqlite
{"source_format":1,"source_sha256":"…","destination_format":2,
 "events_migrated":2,"mutations_applied":22,"kernel_version":"0.1.4"}
```

The source is left exactly as it was — the receipt's `source_format: 1,
destination_format: 2` is the audit trail — and a bundle exported from either
side is byte-identical, because the event log is the source of truth and knows
no engine. Migrating in the other direction, from a SQLite source, is refused
for now with the reason: WAL keeps commits in a sidecar until checkpointed and
a naive file copy would drop the newest ones.

The migration replays the source's event log into a new store and rebuilds
projections from it, rather than copying materialized tables — projections
are derived state, and their shape is the thing a format bump is most likely
to change.

What it guarantees, and what the tests assert:

- **The source is never opened for writing.** It is hashed, copied, and the
  copy is what gets opened, so even redb's recovery after an unclean shutdown
  cannot touch the operator's evidence. The hash is checked again at the end,
  and a mismatch fails the migration rather than reporting success.
- **The destination cannot already hold a store.** Re-running a finished
  migration says so — naming the event count and source hash — instead of
  reading as a scary conflict.
- **The result carries a receipt**, persisted in the destination and readable
  afterwards: source format, source sha256, events migrated, mutations
  applied, kernel version.

Today one store format exists, so a migration is a faithful replay. When a
format bump lands, its translation step belongs in
`crates/kmp-adapter-embedded/src/adapter/migration.rs`, and this matrix moves
in the same pull request. The scaffolding is deliberately in place and tested
before it is needed, including against a store stamped with an older format.

`kmp-neo4j-migrate` is a different tool for a different store: it migrates
the Neo4j schema of the deployed edition, not the embedded redb file.

## Verification status

- Release pipeline: validated via `workflow_dispatch` build matrix.
- Fresh-machine, install-to-first-recovered-context: verified on Linux
  (this machine); macOS and Windows pending real-hardware runs.

## Backup and portability (E6)

```bash
kmp-mcp export memory-backup.jsonl   # data dir per ADR-012 resolution
kmp-mcp import memory-backup.jsonl   # into an EMPTY store only
kmp-mcp snapshot create pre-release  # .kmp/snapshots/pre-release.jsonl
kmp-mcp snapshot verify pre-release  # identity, range, abouts and SHA-256
kmp-mcp migrate <old-dir> <new-dir>  # when the format moved
kmp-mcp --version                    # binary + store format
```

The bundle is the append-only event log (JSON Lines). Format 2's header carries
the portable event format and count plus a snapshot id, creation time,
inclusive event range, sorted about coverage and a SHA-256 digest of the event
lines. Import verifies that identity before replay, reproduces exact revisions
and rebuilds projections — temporal reads and relation proof survive the round
trip. Format-1 bundles remain readable.

For a project-scoped MCP session `.kmp/memory.jsonl` is atomically maintained
after every successful write. A pending marker exists before the store can
change and is cleared only after export, so `doctor` can distinguish a current
committed copy from a crash between the two. Named snapshots are immutable
recovery points; `snapshot read` attaches one through an isolated temporary
store without touching live memory. Recovery from a pending marker is
deliberately two-step: after stopping other sessions, export and inspect first,
then `kmp-mcp export --repair-pending`; a normal export never erases another
process's in-flight marker.

Export/import is an event-format path. `event_format` is engine-agnostic: redb
layout 1 and SQLite layout 2 both carry event format 1. Crossing an on-disk
layout boundary is what `migrate` is for.

Merging into a non-empty store remains deliberately unsupported. Snapshot
merge accepts only identical streams or one exact prefix of the other; two
branches that append at the same position are refused because causal order is
a semantic decision, not a JSONL edit.
