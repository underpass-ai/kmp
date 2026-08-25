#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BIN="${KMP_MCP_BIN:-${ROOT}/target/debug/kmp-mcp}"

if [ ! -x "$BIN" ]; then
  echo "fresh-session: build kmp-mcp first, or set KMP_MCP_BIN" >&2
  exit 1
fi

mkdir -p "${ROOT}/tmp"
WORK="$(mktemp -d "${ROOT}/tmp/pitch-fresh-session.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

printf '$ session 1 writes; sessions 2 and 3 start from zero process state\n\n'
KMP_MCP_BIN="$BIN" KMP_VIEWER_ADDR=off \
  bash "${ROOT}/scripts/demo/embedded_two_sessions.sh" "${WORK}/memory" \
  | sed "s#${WORK}/memory#<shared-data-dir>#g"
