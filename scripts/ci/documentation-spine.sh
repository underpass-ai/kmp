#!/usr/bin/env bash
set -euo pipefail

# Check the README synchronization operation; editorial style and organization
# are reviewed as documentation rather than enforced by scans of prose.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
cargo test --locked -p kmp-release --test public_readme_contract
