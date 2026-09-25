#!/usr/bin/env bash
# Jev is not deterministic: the same request can move a probability by a few
# hundredths, enough to cross a threshold. This records N independent samples
# of the judged corpus, each into its own cassette, and prints every metric's
# mean and range, so a decision can tell an effect from noise.
#   set -a; . ~/.config/typesafe.env; set +a
#   bash scripts/eval/jev-samples.sh 3 target/jev-samples
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"
SAMPLES="${1:-3}"
OUT="${2:-${ROOT_DIR}/target/jev-samples}"
mkdir -p "${OUT}"
for n in $(seq 1 "${SAMPLES}"); do
  KMP_TYPESAFE_CASSETTE="${OUT}/sample-${n}.cassette.json" \
  KMP_TYPESAFE_CASSETTE_MODE=record JEV_REPORT_ONLY=1 \
    bash scripts/ci/jev-baseline.sh > "${OUT}/sample-${n}.txt" 2>&1
done
python3 - "${OUT}" "${SAMPLES}" <<'PY'
import re, sys, statistics
out, n = sys.argv[1], int(sys.argv[2])
values = {}
for i in range(1, n + 1):
    for line in open(f"{out}/sample-{i}.txt"):
        m = re.match(r"\s+([a-z0-9_]+)\s+([0-9.]+)$", line)
        if m:
            values.setdefault(m.group(1), []).append(float(m.group(2)))
print(f"{'metric':32} {'mean':>7} {'min':>7} {'max':>7}   ({n} samples)")
for name, v in values.items():
    print(f"{name:32} {statistics.mean(v):7.3f} {min(v):7.3f} {max(v):7.3f}")
PY
