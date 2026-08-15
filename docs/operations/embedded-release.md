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
| 0.1.x | 1 | refuses to open, names `kmp-mcp migrate` | refuses to open, "upgrade the binary" |

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
kmp-mcp migrate <old-dir> <new-dir>  # when the format moved
kmp-mcp --version                    # binary + store format
```

The bundle is the append-only event log (JSON Lines, header with format
versions and event count). Import replays it reproducing exact revisions and
rebuilding projections — temporal reads and relation proof survive the round
trip. Merging into a non-empty store is deliberately unsupported.

Export/import is a same-format path: `import` refuses a bundle whose
`store_format` differs from the binary's, on purpose. Crossing a format
boundary is what `migrate` is for.
