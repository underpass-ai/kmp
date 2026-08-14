#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOG_PATH="${1:-${TMPDIR:-/tmp}/agentic-context-debug.log}"

mkdir -p "$(dirname "${LOG_PATH}")"
: > "${LOG_PATH}"

cd "${ROOT_DIR}"
. "${ROOT_DIR}/scripts/ci/testcontainers-runtime.sh"

export AGENTIC_DEBUG=1

{
  echo "[agentic-debug] log_path=${LOG_PATH}"
  echo "[agentic-debug] cwd=${ROOT_DIR}"
  echo "[agentic-debug] started_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  cargo test \
    -p kmp-tests-kernel \
    --features container-tests \
    --test agentic_integration \
    --locked \
    -- \
    --nocapture \
    --test-threads=1

  echo "[agentic-debug] finished_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} 2>&1 | tee -a "${LOG_PATH}"
