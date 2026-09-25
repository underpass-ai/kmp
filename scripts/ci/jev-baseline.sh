#!/usr/bin/env bash
# Scores TypeSafe Jev with KMP on the judged corpus and holds it to the
# recorded baseline. It replays the cassette beside the corpus by default:
# no key, no network, and a judgement that was never recorded fails the run
# instead of being guessed.
#
# Measure a new model, prompt or corpus change by recording with a real key:
#   set -a; . ~/.config/typesafe.env; set +a
#   KMP_TYPESAFE_CASSETTE_MODE=record JEV_BASELINE=write bash scripts/ci/jev-baseline.sh
# The floors may rise freely. Lowering one, including by re-recording, is a
# reviewed change that says why.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

export KMP_TYPESAFE_CASSETTE="${KMP_TYPESAFE_CASSETTE:-${ROOT_DIR}/crates/kmp-testkit/judged/jev.cassette.json}"
export KMP_TYPESAFE_CASSETTE_MODE="${KMP_TYPESAFE_CASSETTE_MODE:-replay}"
export KMP_LEXICAL_BRIDGE="${KMP_LEXICAL_BRIDGE:-${ROOT_DIR}/crates/kmp-testkit/judged/lexical-bridge.kmpb}"

cargo run --locked --quiet -p kmp-testkit --bin jev_kmp_scorecard -- \
  "${1:-crates/kmp-testkit/judged/jev_cases.json}" \
  "${2:-docs/development/jev-baseline.tsv}"
