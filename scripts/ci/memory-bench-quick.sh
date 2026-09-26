#!/usr/bin/env bash
# Informative memory bench job (BENCH_SPEC section 12, BT20).
#
# Runs `memory_bench ci`: an A/A of this checkout's release binary on the public
# part of quick-a (the judged retrieval and Jev corpora in cassette replay, and
# synth-v1 10^3 mono). No network, no provider key, nothing private: B-real is a
# local release gate and never runs here, the key and the private root are
# removed from the environment, and every store is a disposable copy under the
# bench cache. Tokens are counted only when tiktoken 0.14.0 is importable; the
# report says so otherwise.
#
# The job is informative: its workflow step is continue-on-error and the gate
# job does not need it. It publishes report.md (the mode summary followed by
# each section's report) as an artifact. docs/development/bench-floors.tsv is
# the proposal of the figures a later gate would hold.
#
# Usage: bash scripts/ci/memory-bench-quick.sh [output dir, default dist/memory-bench]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"
out="${1:-dist/memory-bench}"
binary="target/release/kmp-mcp"
variant="scripts/performance/memory_bench/variants/baseline.toml"

unset TYPESAFE_API_KEY MEMORY_BENCH_PRIVATE_ROOT KMP_TYPESAFE_CASSETTE_MODE

if [[ ! -x "${binary}" ]]; then
  cargo build --release --locked -p kmp-mcp
fi

mkdir -p "${out}"
status=0
python3 -m scripts.performance.memory_bench ci --variant "${variant}" \
  >"${out}/result.json" 2>"${out}/bench.log" || status=$?

python3 - "${out}" "${status}" <<'PY'
"""Assemble report.md: the mode summary, then every section's report.md."""
import json
import pathlib
import sys

out, status = pathlib.Path(sys.argv[1]), int(sys.argv[2])
parts = []
try:
    result = json.loads((out / 'result.json').read_text(encoding='utf-8'))
except (OSError, ValueError):
    result = None
if result is None:
    log = (out / 'bench.log').read_text(encoding='utf-8', errors='replace')[-4000:]
    parts.append(f'# KMP memory bench: ci\n\nThe bench did not finish (exit {status}).\n\n```\n{log}\n```\n')
else:
    summary = pathlib.Path(result['markdown'])
    parts.append(summary.read_text(encoding='utf-8'))
    (out / 'summary.json').write_text(pathlib.Path(result['summary']).read_text(encoding='utf-8'),
                                      encoding='utf-8')
    reports = summary.parent.parent
    for name, section in result['sections'].items():
        key = section.get('report_key')
        page = reports / key / 'report.md' if key else None
        if page is not None and page.is_file():
            parts.append(f'\n---\n\n<!-- section {name} -->\n' + page.read_text(encoding='utf-8'))
(out / 'report.md').write_text('\n'.join(parts), encoding='utf-8')
print(f'memory bench ci: exit {status}; report {out / "report.md"}')
PY

exit "${status}"
