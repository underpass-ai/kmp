#!/usr/bin/env bash
set -euo pipefail

PLUGIN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${PLUGIN_ROOT}/bin/kmp-mcp"

if [[ ! -x "${BINARY}" && -x "${PLUGIN_ROOT}/bin/kmp-mcp.exe" ]]; then
  BINARY="${PLUGIN_ROOT}/bin/kmp-mcp.exe"
fi

# The release bundle ships bin/kmp-mcp and keeps priority: it pins the binary
# this plugin version was tested against. A marketplace install has no bin/ —
# that path is gitignored — so fall back to an installed kmp-mcp on PATH
# rather than leaving the host with a server that cannot start.
if [[ ! -x "${BINARY}" ]]; then
  if PATH_BINARY="$(command -v kmp-mcp 2>/dev/null)"; then
    BINARY="${PATH_BINARY}"
  fi
fi

if [[ ! -x "${BINARY}" ]]; then
  echo "KMP plugin: no kmp-mcp executable found." >&2
  echo "KMP plugin: looked for ${PLUGIN_ROOT}/bin/kmp-mcp (release bundle) and kmp-mcp on PATH." >&2
  echo "KMP plugin: install one with 'cargo install kmp-mcp', or install the plugin from a release package." >&2
  exit 127
fi

export KMP_MCP_BACKEND=embedded

# The data directory is deliberately NOT set here: the embedded kernel
# resolves it itself — KMP_MCP_DATA_DIR when the operator says so, the
# enclosing project root when there is one, the per-user data home
# otherwise. A plugin that picked a location would override that doctrine.
exec "${BINARY}"
