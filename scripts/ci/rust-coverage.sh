#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"

COVERAGE_MIN_LINES="${COVERAGE_MIN_LINES:-80}"
COVERAGE_FLOORS="${COVERAGE_FLOORS:-docs/development/coverage-floors.tsv}"
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
#
# The bar is per crate. The router narrows the test plan to the crates a change
# can reach, so a single aggregate percentage is measured over whatever it
# selected — and a plan narrowed to one crate held that crate to a bar
# calibrated on the whole workspace. Crates already under the bar carry a
# recorded floor that may rise freely; refresh it deliberately, never to make a
# red build green:
#
#   COVERAGE_FLOOR_BASELINE=write bash scripts/ci/rust-coverage.sh dist/coverage
WRITE_FLOORS=()
if [[ "${COVERAGE_FLOOR_BASELINE:-}" == "write" ]]; then
  WRITE_FLOORS=(--write-floors)
fi

# Enforcement follows the plan. A crate the router did not select is measured
# but not judged: its lines were covered only incidentally, by tests that
# never claimed to prove it. A blank list judges every measured crate.
ENFORCE_ONLY=()
if [[ -n "${COVERAGE_ENFORCED_CRATES:-}" ]]; then
  ENFORCE_ONLY=(--enforce-only "${COVERAGE_ENFORCED_CRATES}")
fi

python3 scripts/ci/merge-coverage.py \
  --output target/llvm-cov/lcov.info \
  --fail-under-lines "${COVERAGE_MIN_LINES}" \
  --floors "${COVERAGE_FLOORS}" \
  "${WRITE_FLOORS[@]}" \
  "${ENFORCE_ONLY[@]}" \
  "${FRAGMENTS[@]}"
