#!/usr/bin/env bash
# Scores `kmp_relate` against the judged relate collection and holds it to
# the recorded baseline. Offline, no model, no network: a relate reading is
# a set — facts, declared, coordinate, tensions — and the score is precision
# and recall per set against what a reader judged, which is why this can
# gate a change while the task benchmarks cannot.
#
# The floors may rise freely. Refresh them deliberately, never to make a red
# build green:
#   RELATE_BASELINE=write bash scripts/ci/relate-baseline.sh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

cargo run --locked --quiet -p kmp-testkit --bin relate_kmp_scorecard -- \
  "${1:-crates/kmp-testkit/judged/relate_cases.json}" \
  "${2:-docs/development/relate-baseline.tsv}"
