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
| 0.1.x | 1 | explicit "run the migration tool" error | explicit "upgrade the binary" error |

Rules (ADR-012): the store stamps `FORMAT_VERSION` at creation; the binary
fails fast on any mismatch — never silent empty memory. A format bump
requires shipping the migration tool in the same release and updating this
matrix in the same PR.

## Verification status

- Release pipeline: validated via `workflow_dispatch` build matrix.
- Fresh-machine, install-to-first-recovered-context: verified on Linux
  (this machine); macOS and Windows pending real-hardware runs.

## Backup and portability (E6)

```bash
kmp-mcp export memory-backup.jsonl   # data dir per ADR-012 resolution
kmp-mcp import memory-backup.jsonl   # into an EMPTY store only
kmp-mcp --version                    # binary + store format
```

The bundle is the append-only event log (JSON Lines, header with format
versions and event count). Import replays it reproducing exact revisions and
rebuilding projections — temporal reads and relation proof survive the round
trip. Merging into a non-empty store is deliberately unsupported.
