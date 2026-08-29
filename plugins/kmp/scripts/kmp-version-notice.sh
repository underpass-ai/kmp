#!/bin/sh
set -u

plugin_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary=${KMP_MCP_BIN:-"$plugin_root/bin/kmp-mcp"}
if [ ! -x "$binary" ]; then
  binary=$(command -v kmp-mcp 2>/dev/null || true)
fi
if [ -z "$binary" ] || [ ! -x "$binary" ]; then
  echo 'KMP: the plugin is installed but its Rust engine is missing. Run /kmp:setup.'
  exit 0
fi
"$binary" plugin notice --plugin-root "$plugin_root" 2>/dev/null ||
  echo 'KMP: the plugin and engine cannot prove alignment. Run /kmp:setup.'
