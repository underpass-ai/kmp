#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"

COVERAGE_MIN_LINES="${COVERAGE_MIN_LINES:-80}"
FRAGMENT_ROOT="${1:-dist/coverage}"

mapfile -d '' FRAGMENTS < <(
  find "${FRAGMENT_ROOT}" -type f -name '*.info' -print0 | sort -z
)

if (( ${#FRAGMENTS[@]} == 0 )); then
  echo "no coverage fragments found under ${FRAGMENT_ROOT}" >&2
  exit 1
fi

# Test jobs already ran once with LLVM instrumentation. This gate is only an
# artifact reducer: it never installs Rust, compiles code, starts containers or
# executes tests.
python3 scripts/ci/merge-coverage.py \
  --output target/llvm-cov/lcov.info \
  --fail-under-lines "${COVERAGE_MIN_LINES}" \
  "${FRAGMENTS[@]}"
