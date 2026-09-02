#!/usr/bin/env bash

# Scores `kmp_ask` against the judged collection and holds it to the recorded
# baseline. Offline, no model, no network: the metrics are a comparison against
# labels, which is why this can gate a change while the task benchmarks cannot.
#
# The floors may rise freely. Refresh them deliberately, never to make a red
# build green:
#
#   RETRIEVAL_BASELINE=write bash scripts/ci/retrieval-baseline.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

cargo run --locked --quiet -p kmp-testkit --bin retrieval_kmp_scorecard -- \
  "${1:-crates/kmp-testkit/judged/retrieval_cases.json}" \
  "${2:-docs/development/retrieval-baseline.tsv}"
