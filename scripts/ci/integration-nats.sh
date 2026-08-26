#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"
. "${ROOT_DIR}/scripts/ci/testcontainers-runtime.sh"
. "${ROOT_DIR}/scripts/ci/coverage-test.sh"

run_cargo_test \
  -p kmp-adapter-nats \
  --features container-tests \
  --test runtime_integration \
  --locked \
  -- \
  --nocapture \
  --test-threads=1
