#!/usr/bin/env bash

# Scores `kmp_ask` against the judged collection and holds it to the recorded
# baseline. Offline, no model, no network: the metrics are a comparison against
# labels, which is why this can gate a change while the task benchmarks cannot.
#
# The original 35 cases keep their floors, reported on their own. The anchored
# and negative cases around them add floors for every judged answer and
# ceilings for false UNKNOWN and false answers (overall, with `high`
# confidence, and per case kind). Floors may rise and ceilings fall freely.
# Refresh them deliberately, never to make a red build green:
#
#   RETRIEVAL_BASELINE=write bash scripts/ci/retrieval-baseline.sh
#
# The floors are read with the anchored ask gate, the default. Measure without
# it (the rule of v0.23.0; reported, never gated) with:
#
#   RETRIEVAL_ASK_GATE=off bash scripts/ci/retrieval-baseline.sh

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
