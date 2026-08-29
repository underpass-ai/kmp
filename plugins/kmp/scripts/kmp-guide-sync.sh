#!/usr/bin/env bash
set -euo pipefail

plugin_root="${KMP_GUIDE_PLUGIN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
[ "${1:-}" = "sync" ] && shift

binary="${KMP_MCP_BIN:-}"
if [ "${1:-}" = "--binary" ]; then
  binary="${2:?--binary needs a path}"
  shift 2
fi
if [ -z "$binary" ] && [ -x "$plugin_root/bin/kmp-mcp" ]; then
  binary="$plugin_root/bin/kmp-mcp"
fi
if [ -z "$binary" ]; then
  binary="$(command -v kmp-mcp || true)"
fi
[ -n "$binary" ] || {
  echo 'kmp-guide-sync: no kmp-mcp binary found; run kmp-setup first' >&2
  exit 1
}

exec "$binary" guide sync --plugin-root "$plugin_root" "$@"
