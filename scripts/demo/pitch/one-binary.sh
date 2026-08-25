#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BIN="${KMP_MCP_BIN:-${ROOT}/target/debug/kmp-mcp}"

if [ ! -x "$BIN" ]; then
  echo "one-binary: build kmp-mcp first, or set KMP_MCP_BIN" >&2
  exit 1
fi

mkdir -p "${ROOT}/tmp"
WORK="$(mktemp -d "${ROOT}/tmp/pitch-one-binary.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

export KMP_MCP_BACKEND=embedded
export KMP_MCP_DATA_DIR="${WORK}/memory"
export KMP_VIEWER_ADDR=off
export XDG_DATA_HOME="${WORK}/xdg"

printf '$ kmp-mcp --version\n'
"$BIN" --version

printf '\n$ kmp-mcp info\n'
INFO="$($BIN info)"
printf '%s\n' "$INFO" | awk '
  /▌KMP▐ Backend/ { show = 1 }
  /▌KMP▐ Memory/  { show = 0 }
  /▌KMP▐ Tools/   { show = 1 }
  /▌KMP▐ Viewer/  { show = 0 }
  show { print }
'

printf '\nOne process. Local store. No service, account or API key.\n'
