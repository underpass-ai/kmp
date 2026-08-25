#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="${PAPER_OUTPUT_DIR:-${ROOT_DIR}/archive/artifacts/paper-use-cases}"
# Resolve to absolute path so the test binary finds it regardless of its cwd.
OUTPUT_DIR="$(cd "${ROOT_DIR}" && realpath -m "${OUTPUT_DIR}")"
METRICS_DIR="${OUTPUT_DIR}/cases"
SUMMARY_PATH="${OUTPUT_DIR}/summary.json"

mkdir -p "${METRICS_DIR}"

cd "${ROOT_DIR}"
. "${ROOT_DIR}/scripts/ci/testcontainers-runtime.sh"

export KMP_PAPER_METRICS_DIR="${METRICS_DIR}"
export KMP_PAPER_SUMMARY_PATH="${SUMMARY_PATH}"

cargo test \
  -p kmp-tests-paper \
  --features container-tests \
  --test relationship_use_case_integration \
  --locked \
  -- \
  --nocapture \
  --test-threads=1

cargo test \
  -p kmp-tests-paper \
  --features container-tests \
  --test relationship_use_case_ablation_integration \
  --locked \
  -- \
  --nocapture \
  --test-threads=1

bash "${ROOT_DIR}/scripts/ci/render-paper-use-cases-report.sh"
bash "${ROOT_DIR}/scripts/ci/render-paper-use-cases-figure.sh"

printf 'paper metrics written to %s\n' "${SUMMARY_PATH}"
