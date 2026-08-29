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

## Pull-request routing

CI computes changed paths once with `scripts/ci/quality-gate-plan.py`. Rust
changes expand through the workspace's reverse dependency graph, so a crate
and its consumers are checked without retesting unrelated crates. Docs,
plugin, container, Helm and publication contracts have independent routes.

Rust tests run once with LLVM coverage instrumentation. Each unit or live
integration job uploads its LCOV fragment; the final `coverage` job only merges
those artifacts and enforces the line threshold. It does not compile code,
start containers or execute a second test suite.

`Cargo.toml`, `Cargo.lock`, the toolchain, the quality workflow and the router
itself deliberately select the full matrix. An unclassified path does the
same: optimization may skip proven-unrelated work, but uncertainty never does.
`workflow_dispatch` remains the explicit way to request a full gate.

Inspect a proposed route locally without running it:

```bash
python3 scripts/ci/quality-gate-plan.py --path crates/kmp-adapter-valkey/src/lib.rs
python3 scripts/ci/quality-gate-plan.py --path docs/architecture/index.md
python3 scripts/ci/quality-gate-plan.py --self-test
```

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
bash scripts/ci/integration-nats.sh
bash scripts/ci/integration-conformance.sh
bash scripts/ci/integration-agentic-context.sh
bash scripts/ci/integration-agentic-event-context.sh
bash scripts/ci/integration-kernel-full-journey.sh
bash scripts/ci/integration-kernel-full-journey-tls.sh
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
only deliberate, reviewable artifacts under `artifacts/`.
