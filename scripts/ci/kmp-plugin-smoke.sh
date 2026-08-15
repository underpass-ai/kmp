#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PLUGIN_DIR="${ROOT_DIR}/plugins/kmp"
FIXTURE="${ROOT_DIR}/tests/plugin/kmp-smoke.jsonl"

# One data directory per run, outside any project or the operator's real
# data home: a smoke that inherited either would read memory a previous
# run — or a developer's own agent session — left behind.
SMOKE_DATA_DIR="$(mktemp -d)"
trap 'rm -rf "${SMOKE_DATA_DIR}"' EXIT
if command -v cygpath >/dev/null 2>&1; then
  # Native Windows binary: it cannot open an MSYS path.
  export KMP_MCP_DATA_DIR="$(cygpath -w "${SMOKE_DATA_DIR}")"
else
  export KMP_MCP_DATA_DIR="${SMOKE_DATA_DIR}"
fi

cd "${ROOT_DIR}"
python3 -m json.tool "${PLUGIN_DIR}/.codex-plugin/plugin.json" >/dev/null
python3 -m json.tool "${PLUGIN_DIR}/.claude-plugin/plugin.json" >/dev/null
python3 -m json.tool "${PLUGIN_DIR}/.mcp.json" >/dev/null

# Both host manifests must carry the same version: a bundle that tells
# Codex one version and Claude Code another is a packaging defect.
python3 - <<'EOF'
import json
import pathlib
import sys

plugin_dir = pathlib.Path("plugins/kmp")
codex = json.loads((plugin_dir / ".codex-plugin/plugin.json").read_text())["version"]
claude = json.loads((plugin_dir / ".claude-plugin/plugin.json").read_text())["version"]
if codex != claude:
    sys.exit(f"KMP plugin smoke: manifest versions diverge ({codex} != {claude})")
EOF

bash scripts/plugin/build-local-kmp-plugin.sh

responses="$("${PLUGIN_DIR}/scripts/run-embedded-mcp.sh" <"${FIXTURE}")"

response_contains() {
  local needle="$1"
  if command -v rg >/dev/null 2>&1; then
    rg -Fq -- "${needle}"
  else
    grep -Fq -- "${needle}"
  fi
}

if [[ "$(printf '%s\n' "${responses}" | wc -l)" -ne 2 ]]; then
  echo "KMP plugin smoke expected two MCP responses" >&2
  exit 1
fi

for tool in kernel_wake kernel_ask kernel_write_memory kernel_trace; do
  if ! response_contains "\"name\":\"${tool}\"" <<<"${responses}"; then
    echo "KMP plugin smoke did not advertise ${tool}" >&2
    exit 1
  fi
done

echo "KMP plugin smoke passed"
