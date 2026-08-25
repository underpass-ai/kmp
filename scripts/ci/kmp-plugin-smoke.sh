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

# The demo bundle ships inside the plugin, so a packaging change that drops it
# would leave /kmp:demo broken on every new install. That it *loads* is a Rust
# test (tests/demo_bundle.rs); that it is *there* belongs here, where the
# bundle's presence in the package is what is being proved.
if [ ! -f "${PLUGIN_DIR}/demo/checkout-latency.jsonl" ]; then
  echo "KMP plugin smoke: the demo bundle is missing from the plugin" >&2
  exit 1
fi
python3 - "${PLUGIN_DIR}/demo/checkout-latency.jsonl" <<'PY'
import json, sys

path = sys.argv[1]
with open(path, encoding="utf-8") as handle:
    lines = [line for line in handle if line.strip()]
header = json.loads(lines[0])
events = len(lines) - 1
if header["event_count"] != events:
    sys.exit(
        f"demo bundle header declares {header['event_count']} events, file has {events}"
    )
for line in lines[1:]:
    json.loads(line)
PY

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

for tool in kmp_wake kmp_ask kmp_write_memory kmp_trace; do
  if ! response_contains "\"name\":\"${tool}\"" <<<"${responses}"; then
    echo "KMP plugin smoke did not advertise ${tool}" >&2
    exit 1
  fi
done

# Claude Code names a plugin MCP server `plugin:<plugin>:<server>`. Doctor
# must accept that native registration, not prescribe a redundant direct MCP
# entry after setup has already succeeded.
DOCTOR_BIN="${PLUGIN_DIR}/bin/kmp-mcp"
if [[ ! -x "${DOCTOR_BIN}" ]]; then
  DOCTOR_BIN="${PLUGIN_DIR}/bin/kmp-mcp.exe"
fi
doctor_output="$(
  NO_COLOR=1 \
  KMP_MCP_BIN="${DOCTOR_BIN}" \
  KMP_MCP_BACKEND=embedded \
  KMP_VIEWER_ADDR=off \
  KMP_DOCTOR_CLAUDE_MCP_LIST='plugin:kmp:kmp: bundled launcher - connected' \
    bash "${PLUGIN_DIR}/scripts/kmp-doctor.sh"
)"
if ! grep -Fq '[✓] Hosts      Claude Code — kmp registered' <<<"${doctor_output}"; then
  echo "KMP plugin smoke: doctor rejected Claude Code's native plugin registration" >&2
  printf '%s\n' "${doctor_output}" >&2
  exit 1
fi

echo "KMP plugin smoke passed"
