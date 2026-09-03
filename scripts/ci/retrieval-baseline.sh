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

# Every case runs against the same lexical-bridge table: the judged fixture,
# built by scripts/lexical-bridge/build.py from the words the cases use, so
# the cross-language cases measure the mechanism and not whichever table an
# operator happens to have installed.
export KMP_LEXICAL_BRIDGE="${KMP_LEXICAL_BRIDGE:-${ROOT_DIR}/crates/kmp-testkit/judged/lexical-bridge.kmpb}"

cargo run --locked --quiet -p kmp-testkit --bin retrieval_kmp_scorecard -- \
  "${1:-crates/kmp-testkit/judged/retrieval_cases.json}" \
  "${2:-docs/development/retrieval-baseline.tsv}"
