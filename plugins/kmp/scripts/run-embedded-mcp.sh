#!/usr/bin/env bash
set -euo pipefail

PLUGIN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${PLUGIN_ROOT}/bin/kmp-mcp"

if [[ ! -x "${BINARY}" && -x "${PLUGIN_ROOT}/bin/kmp-mcp.exe" ]]; then
  BINARY="${PLUGIN_ROOT}/bin/kmp-mcp.exe"
fi

if [[ ! -x "${BINARY}" ]]; then
  echo "KMP plugin: missing executable ${BINARY}" >&2
  echo "KMP plugin: build the local plugin bundle before installing it" >&2
  exit 127
fi

export KMP_MCP_BACKEND=embedded

# The data directory is deliberately NOT set here: the embedded kernel
# resolves it itself — KMP_MCP_DATA_DIR when the operator says so, the
# enclosing project root when there is one, the per-user data home
# otherwise. A plugin that picked a location would override that doctrine.
exec "${BINARY}"
