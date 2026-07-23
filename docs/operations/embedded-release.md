# Embedded Edition Releases (E5)

## How a release happens

Push a tag `vX.Y.Z` → the `release` workflow builds, strips and checksums
`rehydration-mcp` for linux x86_64/aarch64, macOS arm64/x86_64 and Windows
x86_64, and attaches every artifact (+ `.sha256`) to the GitHub release.
`workflow_dispatch` runs the same matrix without publishing (pipeline
verification). The embedded edition releases from the same tags as the
kernel — one version for the whole product.

## Install paths

- One command: `scripts/install/install.sh` (checksum-verified download,
  prints per-host registration snippets).
- From source: `cargo install --path crates/rehydration-mcp --locked`
  (kept working; crates.io publication deferred until the name/branding
  decision of ADR-013 is revisited).

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
