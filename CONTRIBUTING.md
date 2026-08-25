# Contributing

Thanks for helping KMP remember better.

## Before You Start

KMP is local-first agent memory, but the kernel underneath it stays generic.
Start with the [architecture guide](docs/architecture/README.md) before moving
boundaries.

Non-negotiable rules:

- DDD first
- hexagonal boundaries
- no god objects
- no god files
- one main concept per file
- one use case per file
- no product-specific nouns in the kernel boundary

The kernel public language stays node-centric:

- root node
- neighbor nodes
- relationships
- node detail

If a change needs `story`, `task`, `project`, `planning.*`,
`orchestration.*`, or similar product language, it probably belongs in an
integrating product, not here.

## Toolchain

- Rust `1.97.1`
- Docker or Podman for container-backed integration tests

## Local checks

For documentation-only changes, run the focused documentation contracts:

```bash
bash scripts/ci/documentation-spine.sh
python3 scripts/ci/kmp-capability-contract.py
python3 scripts/ci/kmp-agent-routing-contract.py
```

For Rust changes, start with:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo test --workspace --locked
```

The full local gate is `bash scripts/ci/quality-gate.sh`. It includes public
contract checks and is deliberately more expensive. The
[testing guide](docs/development/testing.md) maps embedded, container-backed
and live E2E changes to their focused gates.

## Contract Changes

If you change a public MCP, gRPC, HTTP or async contract:

- update the canonical examples under `api/examples`
- update the owning current document and executable capability contract
- keep contract tests green
- preserve generic naming
- do not introduce integrating-product nouns

Store-format and migration changes need an explicit compatibility story. A
binary must never guess which engine owns an existing store.

## Pull Requests

Good PRs here are small, explicit, and technically narrow.

Please include:

- what changed
- why it belongs in the kernel
- validation performed
- any contract or migration impact

## Documentation

Update current docs when your change affects:

- public contracts
- plugin, skill or MCP ownership
- setup, storage or privacy behavior
- enterprise deployment
- operational behavior

Historical material under `archive/` is evidence. Do not rewrite it to make a
new implementation look older than it is.

## Reporting Problems

Use GitHub issues for bugs and feature requests.

For security-sensitive reports, follow [`SECURITY.md`](./SECURITY.md).
