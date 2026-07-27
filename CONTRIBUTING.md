# Contributing

Thanks for contributing.

## Before You Start

This repo is intentionally strict about architecture.

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

## Local Checks

Run these before opening a PR:

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
bash scripts/ci/contract-gate.sh
bash scripts/ci/quality-gate.sh
```

If your change touches container-backed flows, also run the relevant script from
`scripts/ci/`.

## Contract Changes

If you change public gRPC or async contracts:

- update the canonical examples under `api/examples`
- keep contract tests green
- preserve generic naming
- do not introduce integrating-product nouns

## Pull Requests

Good PRs here are small, explicit, and technically narrow.

Please include:

- what changed
- why it belongs in the kernel
- validation performed
- any contract or migration impact

## Documentation

Update docs when your change affects:

- public contracts
- runtime integration
- migration handoff
- operational behavior

## Reporting Problems

Use GitHub issues for bugs and feature requests.

For security-sensitive reports, follow [`SECURITY.md`](./SECURITY.md).
