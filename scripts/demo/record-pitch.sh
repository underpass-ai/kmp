#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

command -v vhs >/dev/null 2>&1 || {
  echo "record-pitch: VHS is required (https://github.com/charmbracelet/vhs)" >&2
  exit 1
}

BIN="${KMP_MCP_BIN:-${ROOT}/target/debug/kmp-mcp}"
if [ ! -x "$BIN" ]; then
  cargo build -p kmp-mcp --locked
fi
export KMP_MCP_BIN="$BIN"

mkdir -p docs/showcase/recordings
for tape in docs/showcase/tapes/*.tape; do
  echo "recording ${tape}"
  vhs --quiet "$tape"
done

echo "pitch recordings regenerated"
