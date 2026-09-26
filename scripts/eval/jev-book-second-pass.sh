#!/usr/bin/env bash
# The verdict book's promise on the judged corpus (DESIGN L4 4a, P5): a
# second run over the same stores and book sends no request and reads the
# same answers, byte for byte.
#
# Pass 1 replays the recorded cassette with fresh books. Pass 2 keeps every
# arm's book, seeds the rest afresh and puts an empty cassette behind it:
# a verdict the book does not hold fails the run instead of reaching the
# network. The two runs' tool answers, without Jev cost accounting, must be
# identical, and pass 2 must spend nothing.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

CASES="${1:-crates/kmp-testkit/judged/jev_cases.json}"
CASSETTE="${KMP_TYPESAFE_CASSETTE:-${ROOT_DIR}/crates/kmp-testkit/judged/jev.cassette.json}"
WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

printf '{"schema":"kmp.typesafe.cassette.v1","model":"jev-1.13.0","entries":{}}\n' >"${WORK}/empty.cassette.json"

run() {
  cargo run --locked --quiet -p kmp-testkit --bin jev_kmp_scorecard -- "${CASES}"
}

echo "pass 1: recorded cassette, fresh books"
KMP_TYPESAFE_CASSETTE="${CASSETTE}" KMP_TYPESAFE_CASSETTE_MODE=replay \
  KMP_LEXICAL_BRIDGE="${KMP_LEXICAL_BRIDGE:-${ROOT_DIR}/crates/kmp-testkit/judged/lexical-bridge.kmpb}" \
  JEV_REPORT_ONLY=1 JEV_SCORECARD_ANSWERS="${WORK}/first.answers" run >"${WORK}/first.log"
echo "pass 2: the same books, an empty cassette behind them"
KMP_TYPESAFE_CASSETTE="${WORK}/empty.cassette.json" KMP_TYPESAFE_CASSETTE_MODE=replay \
  KMP_LEXICAL_BRIDGE="${KMP_LEXICAL_BRIDGE:-${ROOT_DIR}/crates/kmp-testkit/judged/lexical-bridge.kmpb}" \
  JEV_SCORECARD_KEEP_BOOK=1 JEV_REPORT_ONLY=1 JEV_SCORECARD_ANSWERS="${WORK}/second.answers" \
  run >"${WORK}/second.log"

for pass in first second; do
  echo "${pass}: $(grep -E 'curate Jev requests' "${WORK}/${pass}.log" | sed 's/^ *(cost, not gated) *//')"
  echo "${pass}: $(grep -E 'tool answers, sha256' "${WORK}/${pass}.log" | sed 's/^ *(replay, not gated) *//')"
done

status=0
if ! grep -qE ' 0 curate Jev requests, 0 input tokens' "${WORK}/second.log"; then
  echo "FAIL: the second pass spent Jev requests" >&2
  status=1
fi
if ! cmp -s "${WORK}/first.answers" "${WORK}/second.answers"; then
  echo "FAIL: the second pass read different answers" >&2
  diff "${WORK}/first.answers" "${WORK}/second.answers" | cut -c1-400 | head -20 >&2 || true
  status=1
fi
metrics() { grep -E '^  [a-z_0-9]+ +[0-9.]+$' "$1"; }
if ! diff <(metrics "${WORK}/first.log") <(metrics "${WORK}/second.log") >/dev/null; then
  echo "FAIL: the second pass scored differently" >&2
  status=1
fi
# The retrieval cases' rerank arms, answered from their own cassette: each
# case's book is carried to the second run; the ask answers must match but
# for the wall-clock ingest times of the re-seeded stores.
retrieval() {
  RETRIEVAL_RERANK="$1" KMP_TYPESAFE_CASSETTE="$2" KMP_TYPESAFE_CASSETTE_MODE=replay \
    KMP_LEXICAL_BRIDGE="${KMP_LEXICAL_BRIDGE:-${ROOT_DIR}/crates/kmp-testkit/judged/lexical-bridge.kmpb}" \
    RETRIEVAL_BOOKS="${WORK}/books-$1" RETRIEVAL_ANSWERS="$3" \
    cargo run --locked --quiet -p kmp-testkit --bin retrieval_kmp_scorecard >"$3.log"
}
without_clocks() {
  python3 -c '
import json, sys
def strip(value):
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items() if k not in ("ingested_at", "observed_at")}
    if isinstance(value, list):
        return [strip(v) for v in value]
    return value
for line in open(sys.argv[1]):
    case, answer = line.split(" ", 1)
    print(case, json.dumps(strip(json.loads(answer)), sort_keys=True))
' "$1"
}
for arm in narrow wide; do
  retrieval "${arm}" "${ROOT_DIR}/crates/kmp-testkit/judged/retrieval.jev.cassette.json" "${WORK}/retrieval-${arm}-1"
  retrieval "${arm}" "${WORK}/empty.cassette.json" "${WORK}/retrieval-${arm}-2"
  if diff <(without_clocks "${WORK}/retrieval-${arm}-1") <(without_clocks "${WORK}/retrieval-${arm}-2") >/dev/null; then
    echo "retrieval ${arm}: $(wc -l <"${WORK}/retrieval-${arm}-1") asks answered again from the book alone, identical"
  else
    echo "FAIL: retrieval ${arm} answered differently from the book" >&2
    status=1
  fi
done

[ "${status}" -eq 0 ] && echo "verdict book holds: second pass sent no request and read identical answers"
exit "${status}"
