# Testing

The pull-request gate is defined by
[`.github/workflows/quality-gate.yml`](../../.github/workflows/quality-gate.yml).
Do not infer current coverage from archived testing prose.

## Fast local checks

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
bash scripts/ci/documentation-spine.sh
python3 scripts/ci/kmp-capability-contract.py
python3 scripts/ci/kmp-agent-routing-contract.py
```

Run clippy before proposing Rust changes:

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Crates.io documentation is a public build, not a README preview:

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

The aggregate local gate is:

```bash
bash scripts/ci/quality-gate.sh
```

It is more expensive than the focused commands and should be used in
proportion to the change.

## Embedded gates

```bash
bash scripts/ci/embedded-binary-gates.sh
bash scripts/ci/embedded-sqlite-gates.sh
```

These enforce the installable binary boundary, store conformance, crash
recovery and concurrent SQLite use.

## Infrastructure integration

The dedicated scripts under `scripts/ci/` own each live dependency test:

```bash
bash scripts/ci/integration-valkey.sh
bash scripts/ci/integration-neo4j.sh
bash scripts/ci/integration-conformance.sh
bash scripts/ci/integration-mcp-real-kernel.sh
bash scripts/ci/container-image.sh
bash scripts/ci/helm-lint.sh
```

They require the tools and container runtime appropriate to the selected
test. Do not run every infrastructure or model-backed test for a documentation
change.

## Live E2E

Run `scripts/e2e/regen.sh` before an authorized live-cluster E2E. It is a
preflight, not permission to mutate a cluster. Cluster changes require the
specific deployment authority and credentials supplied for that environment.

## Scratch and evidence

Keep disposable test output under the repository's ignored `tmp/` directory
and remove it after the run. `target/` and `tmp/` are not evidence. Persist
only deliberate, reviewable artifacts under `artifacts/` or its archive.
