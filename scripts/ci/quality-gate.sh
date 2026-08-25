#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"

bash scripts/ci/contract-gate.sh
bash scripts/ci/documentation-spine.sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p kmp-mcp --locked
bash scripts/ci/pitch-material.sh
