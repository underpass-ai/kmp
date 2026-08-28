#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"

bash scripts/ci/contract-gate.sh
python3 scripts/ci/github-actions-contract.py
python3 scripts/ci/quality-gate-plan.py --self-test
python3 scripts/ci/merge-coverage.py --self-test
python3 scripts/ci/quality-workflow-contract.py
bash scripts/ci/documentation-spine.sh
bash scripts/ci/mcp-registry.sh
node --test crates/kmp-viewer/ui/loom-core.test.js
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo test --workspace --locked
cargo build -p kmp-mcp --locked
KMP_MCP_BIN=target/debug/kmp-mcp bash scripts/demo/embedded_two_sessions.sh
