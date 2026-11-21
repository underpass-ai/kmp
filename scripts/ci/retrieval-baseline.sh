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
#
# With no arguments it then scores the judged collection and the shipped-bridge
# stress cases again with the table `kmp-mcp setup` installs, each against its
# own baseline (retrieval-baseline-shipped-bridge.tsv,
# retrieval-bridge-stress-baseline.tsv); RETRIEVAL_BASELINE=write records all three.

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

# The table `kmp-mcp setup` installs for every user is not the judged fixture:
# it is a multilingual embedding that also pairs neighbours inside one
# language. The promise that `kmp_ask` never answers what is not stored has to
# hold with it too, so the judged collection and the shipped-bridge stress
# cases (synthetic negatives and cross-language questions,
# crates/kmp-mcp/tests/fixtures/lexical_bridge_stress/) are scored with it,
# each against its own recorded rows. Only when no collection was named.
if [[ $# -eq 0 ]]; then
  shipped="${ROOT_DIR}/distribution/lexical-bridge/kmp-lexical-bridge.kmpb"
  echo
  echo "== the judged collection with the shipped lexical-bridge table"
  KMP_LEXICAL_BRIDGE="${shipped}" cargo run --locked --quiet -p kmp-testkit --bin retrieval_kmp_scorecard -- \
    crates/kmp-testkit/judged/retrieval_cases.json \
    docs/development/retrieval-baseline-shipped-bridge.tsv
  echo
  echo "== the shipped-bridge stress cases with the shipped lexical-bridge table"
  KMP_LEXICAL_BRIDGE="${shipped}" cargo run --locked --quiet -p kmp-testkit --bin retrieval_kmp_scorecard -- \
    crates/kmp-mcp/tests/fixtures/lexical_bridge_stress/cases.json \
    docs/development/retrieval-bridge-stress-baseline.tsv
fi
