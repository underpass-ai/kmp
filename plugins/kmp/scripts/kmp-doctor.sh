#!/bin/sh
set -eu

plugin_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary=${KMP_MCP_BIN:-"$plugin_root/bin/kmp-mcp"}
if [ ! -x "$binary" ]; then
  binary=$(command -v kmp-mcp 2>/dev/null || true)
fi
if [ -z "$binary" ] || [ ! -x "$binary" ]; then
  echo "KMP doctor needs kmp-mcp; install it with 'cargo install kmp-mcp'." >&2
  exit 127
fi
exec "$binary" doctor "$@"
