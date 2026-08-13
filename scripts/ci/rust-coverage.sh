#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"

mkdir -p target/llvm-cov

COVERAGE_MIN_LINES="${COVERAGE_MIN_LINES:-80}"
COVERAGE_IGNORE_FILENAME_REGEX="${COVERAGE_IGNORE_FILENAME_REGEX:-kmp-conformance/.*|kmp-testkit/.*|kmp-tests-shared/.*|kmp-tests-kernel/.*|kmp-tests-paper/.*|kmp-transport-grpc/src/agentic_reference/.*}"

if [[ -z "${LLVM_COV:-}" ]] && command -v llvm-cov >/dev/null 2>&1; then
  export LLVM_COV
  LLVM_COV="$(command -v llvm-cov)"
fi

if [[ -z "${LLVM_PROFDATA:-}" ]] && command -v llvm-profdata >/dev/null 2>&1; then
  export LLVM_PROFDATA
  LLVM_PROFDATA="$(command -v llvm-profdata)"
fi

# Merge coverage from the fast workspace suite plus container-backed adapter
# tests so Sonar sees the real exercised read/write paths.
cargo llvm-cov clean --workspace

cargo llvm-cov --workspace --locked --no-report

. "${ROOT_DIR}/scripts/ci/testcontainers-runtime.sh"

run_container_coverage_test() {
  local package="$1"
  local test_target="$2"

  # Coverage instrumentation makes the container-backed suites slower and more
  # sensitive to parallel startup/load spikes, so keep them single-threaded.
  RUST_TEST_THREADS=1 cargo llvm-cov \
    -p "${package}" \
    --features container-tests \
    --test "${test_target}" \
    --locked \
    --no-report \
    -- \
    --test-threads=1

  return 0
}

run_container_coverage_test kmp-adapter-valkey valkey_integration
run_container_coverage_test kmp-adapter-neo4j neo4j_integration
run_container_coverage_test kmp-adapter-nats runtime_integration
run_container_coverage_test kmp-tests-kernel agentic_integration
run_container_coverage_test kmp-tests-kernel agentic_event_integration
run_container_coverage_test kmp-tests-kernel kernel_full_journey_integration
run_container_coverage_test kmp-tests-kernel kernel_full_journey_tls_integration

cargo llvm-cov report \
  --locked \
  --ignore-filename-regex "${COVERAGE_IGNORE_FILENAME_REGEX}" \
  --lcov \
  --output-path target/llvm-cov/lcov.info
cargo llvm-cov report \
  --locked \
  --ignore-filename-regex "${COVERAGE_IGNORE_FILENAME_REGEX}" \
  --summary-only \
  --fail-under-lines "${COVERAGE_MIN_LINES}"
