#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"
. "${ROOT_DIR}/scripts/ci/testcontainers-runtime.sh"

cargo test \
  -p kmp-adapter-neo4j \
  --features container-tests \
  --test neo4j_integration \
  --locked \
  -- \
  --nocapture \
  --test-threads=1
