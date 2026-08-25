#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BIN="${KMP_MCP_BIN:-${ROOT}/target/debug/kmp-mcp}"
BUNDLE="${ROOT}/plugins/kmp/demo/checkout-latency.jsonl"

if [ ! -x "$BIN" ]; then
  echo "wrong-turn: build kmp-mcp first, or set KMP_MCP_BIN" >&2
  exit 1
fi

mkdir -p "${ROOT}/tmp"
WORK="$(mktemp -d "${ROOT}/tmp/pitch-wrong-turn.XXXXXX")"
VIEWER_PID=""
cleanup() {
  if [ -n "$VIEWER_PID" ]; then
    kill "$VIEWER_PID" 2>/dev/null || true
    wait "$VIEWER_PID" 2>/dev/null || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

export KMP_MCP_BACKEND=embedded
export KMP_MCP_DATA_DIR="${WORK}/memory"
export KMP_VIEWER_ADDR=off
export XDG_DATA_HOME="${WORK}/xdg"

printf '$ kmp-mcp import checkout-latency.jsonl\n'
"$BIN" import "$BUNDLE"

printf '\n$ kmp-mcp document incident:checkout-latency\n'
DOCUMENT="$($BIN document incident:checkout-latency)"
printf '%s\n' "$DOCUMENT" | awk '
  /^# incident:/ { print; next }
  /^8 entries,/ { print; next }
  /^## What did not/ { show = 1 }
  /^## What still disagrees/ { show = 1 }
  show { print }
'

PORT="${KMP_PITCH_VIEWER_PORT:-17319}"
KMP_VIEWER_ADDR=off "$BIN" viewer "127.0.0.1:${PORT}" >"${WORK}/viewer.log" 2>&1 &
VIEWER_PID=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do
  if ABOUTS="$(curl -fsS "http://127.0.0.1:${PORT}/api/abouts" 2>/dev/null)"; then
    break
  fi
  sleep 0.2
done
: "${ABOUTS:?viewer did not answer on ${PORT}}"

printf '\n$ curl http://127.0.0.1:%s/api/abouts\n' "$PORT"
printf '%s\n' "$ABOUTS"
printf 'The same incident is now visible in the local graph viewer.\n'
