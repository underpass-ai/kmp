#!/usr/bin/env bash
#
# Matrix evaluation: models × prompt variants from YAML config.
#
# Runs the lightweight e2e test (4 UCs) for each cell in the matrix.
# Each cell prints a summary table + detailed agent/judge responses.
#
# Usage:
#   export ANTHROPIC_KEY="$(cat /path/to/anthropic-key.txt)"
#   export OPENAI_KEY="$(cat /path/to/openai-key.txt)"
#   bash scripts/ci/evaluate-prompt-variants.sh
#
# Override matrix file:
#   bash scripts/ci/evaluate-prompt-variants.sh path/to/custom-matrix.yaml
#
# Run subset:
#   FILTER_MODELS="qwen3-8b" FILTER_PROMPTS="default citation-agent" \
#     bash scripts/ci/evaluate-prompt-variants.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RESOURCES="${ROOT_DIR}/crates/kmp-testkit/resources"
MATRIX_FILE="${1:-${RESOURCES}/evaluation-matrix.yaml}"

cd "${ROOT_DIR}"
. "${ROOT_DIR}/scripts/ci/testcontainers-runtime.sh"

# Pre-compile once
cargo test \
  -p kmp-tests-paper \
  --features container-tests \
  --test llm_judge_prompt_evaluation \
  --no-run 2>&1 | tail -1

CELL=0

python3 -c "
import yaml, json, sys

with open('${MATRIX_FILE}') as f:
    matrix = yaml.safe_load(f)

filter_models = '${FILTER_MODELS:-}'.split() or list(matrix['models'].keys())
filter_prompts = '${FILTER_PROMPTS:-}'.split() or list(matrix['prompts'].keys())

cells = []
for model_name in filter_models:
    m = matrix['models'][model_name]
    for prompt_name in filter_prompts:
        prompt_path = matrix['prompts'][prompt_name]
        cells.append({
            'model_name': model_name,
            'prompt_name': prompt_name,
            'endpoint': m['endpoint'],
            'model': m['model'],
            'provider': m['provider'],
            'tls': m.get('tls', False),
            'api_key_env': m.get('api_key_env', ''),
            'judge_endpoint': m['judge_endpoint'],
            'judge_model': m['judge_model'],
            'judge_provider': m['judge_provider'],
            'judge_api_key_env': m.get('judge_api_key_env', 'ANTHROPIC_KEY'),
            'prompt_path': prompt_path or '',
        })

json.dump(cells, sys.stdout)
" | python3 -c "
import json, sys, subprocess, os

cells = json.load(sys.stdin)
resources = '${RESOURCES}'
total = len(cells)

for i, cell in enumerate(cells, 1):
    print(f\"\n\n{'█' * 68}\")
    print(f\"██  CELL {i}/{total}: model={cell['model_name']}  prompt={cell['prompt_name']}\")
    print(f\"{'█' * 68}\n\")

    env = os.environ.copy()
    env['LLM_ENDPOINT'] = cell['endpoint']
    env['LLM_MODEL'] = cell['model']
    env['LLM_PROVIDER'] = cell['provider']
    env['LLM_TEMPERATURE'] = '0.0'

    if cell['tls']:
        env.setdefault('LLM_TLS_CERT_PATH', '/tmp/vllm-client.crt')
        env.setdefault('LLM_TLS_KEY_PATH', '/tmp/vllm-client.key')
    else:
        env.pop('LLM_TLS_CERT_PATH', None)
        env.pop('LLM_TLS_KEY_PATH', None)

    if cell['api_key_env']:
        env['LLM_API_KEY'] = os.environ.get(cell['api_key_env'], '')

    env['LLM_JUDGE_ENDPOINT'] = cell['judge_endpoint']
    env['LLM_JUDGE_MODEL'] = cell['judge_model']
    env['LLM_JUDGE_PROVIDER'] = cell['judge_provider']
    env['LLM_JUDGE_API_KEY'] = os.environ.get(cell['judge_api_key_env'], '')

    if cell['prompt_path']:
        env['LLM_PROMPTS_PATH'] = os.path.join(resources, cell['prompt_path'])
    else:
        env.pop('LLM_PROMPTS_PATH', None)

    result = subprocess.run(
        ['cargo', 'test',
         '-p', 'kmp-tests-paper',
         '--features', 'container-tests',
         '--test', 'llm_judge_prompt_evaluation',
         '--', '--nocapture', '--test-threads=1'],
        env=env,
        cwd='${ROOT_DIR}',
    )
    if result.returncode != 0:
        print(f'  CELL {i} FAILED (exit {result.returncode})')

print(f\"\n\n{'█' * 68}\")
print(f\"██  ALL {total} CELLS COMPLETE\")
print(f\"{'█' * 68}\")
"
