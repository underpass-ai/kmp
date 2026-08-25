#!/usr/bin/env bash
#
# P0 ground truth diagnostic — runs the minimum subset to demonstrate
# that the ground truth penalizes precise causal reasoning.
#
# Runs: 1 variant (micro-ops-explanatory-clean) x 3 agents x 1 judge x 1 prompt = 3 evals
# Time: ~3 minutes (vs 65 min full matrix)
# Cost: ~$0.10 (vs ~$15 full matrix)
#
# What to look for in the output:
#   - Qwen3-8B says failure_point="incident root" → Task=OK (trivial match)
#   - GPT-5.4 / Opus 4.6 say failure_point="chain-N" → Task=FAIL (precise but penalized)
#
# If frontier models score LOWER than Qwen, the ground truth is the problem.
#
# Usage:
#   export ANTHROPIC_KEY="$(cat /path/to/anthropic-key.txt)"
#   export OPENAI_KEY="$(cat /path/to/openai-key.txt)"
#   bash scripts/ci/e2e-ground-truth-diagnostic.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

SEP="$(printf '═%.0s' {1..62})"

echo "${SEP}"
echo "  P0 GROUND TRUTH DIAGNOSTIC"
echo "  micro scale × clean noise × default prompt"
echo "  3 agents × 2 judges (minus self) = 4 agent-judge pairs"
echo "  6 variants × 4 pairs = 24 evals (~5 min)"
echo "${SEP}"
echo

export FILTER_SCALES="${FILTER_SCALES:-micro}"
export FILTER_NOISE="${FILTER_NOISE:-clean}"
export FILTER_PROMPTS="${FILTER_PROMPTS:-default}"

cargo test -p kmp-tests-paper --features container-tests \
  --test llm_judge_prompt_evaluation -- --nocapture --test-threads=1

# Find the latest run directory
LATEST_RUN=$(ls -td "${ROOT_DIR}/archive/artifacts/e2e-runs/"*/ 2>/dev/null | head -1)

if [[ -z "${LATEST_RUN}" ]]; then
  echo "No run directory found"
  exit 1
fi

echo
echo "${SEP}"
echo "  DIAGNOSTIC ANALYSIS"
echo "${SEP}"
echo
echo "Run: ${LATEST_RUN}"
echo

# Extract and compare failure_point across models for explanatory variant
python3 -c "
import json, glob, os

run_dir = '${LATEST_RUN}'
results_dir = os.path.join(run_dir, 'results')
if not os.path.isdir(results_dir):
    print('No results directory found')
    exit(1)

files = sorted(glob.glob(os.path.join(results_dir, '*.json')))
if not files:
    print('No result files found')
    exit(1)

print('Agent Response Analysis (explanatory variants only):')
print('=' * 80)
print()

for f in files:
    d = json.load(open(f))

    # Parse agent response
    agent_resp = d.get('agent_response', '')
    try:
        parsed = json.loads(agent_resp.strip().strip('\`').removeprefix('json').strip())
        fp = parsed.get('failure_point', '?')
        rn = parsed.get('restart_node', '?')
    except Exception:
        fp = '(parse error)'
        rn = '(parse error)'

    model = d.get('model', '?')
    variant = d.get('variant', '?')
    task = d.get('task')
    if task is True:
        task_str = 'OK'
    elif task is False:
        task_str = 'FAIL'
    else:
        task_str = 'ERR'

    print(f'  {model:<20} {variant}')
    print(f'    failure_point: {fp}')
    print(f'    restart_node:  {rn}')
    print(f'    Task verdict:  {task_str}')
    print()

# Summary
print('Verdict:')
print('-' * 40)
results = [json.load(open(f)) for f in files]
by_model = {}
for r in results:
    m = r['model'].split('→')[0]  # agent name
    by_model.setdefault(m, []).append(r.get('task'))

for model, tasks in sorted(by_model.items()):
    ok = sum(1 for t in tasks if t is True)
    total = len(tasks)
    print(f'  {model:<16}: Task {ok}/{total}')

frontier_ok = sum(1 for m, ts in by_model.items() if m != 'qwen3-8b' for t in ts if t is True)
frontier_total = sum(len(ts) for m, ts in by_model.items() if m != 'qwen3-8b')
qwen_ok = sum(1 for t in by_model.get('qwen3-8b', []) if t is True)
qwen_total = len(by_model.get('qwen3-8b', []))

print()
if qwen_total > 0 and frontier_total > 0:
    if qwen_ok / qwen_total > frontier_ok / frontier_total:
        print('⚠  GROUND TRUTH PROBLEM CONFIRMED: small model > frontier models')
        print('   The ground truth rewards trivial root-matching over causal precision.')
    elif qwen_ok / qwen_total == frontier_ok / frontier_total:
        print('→  Models score equally — ground truth may be adequate for this subset')
    else:
        print('✔  Frontier models score higher — ground truth appears correct')
"
