#!/usr/bin/env bash
set -euo pipefail

PLUGIN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# An explicit binary wins over both of the choices below. The release bundle
# is built without the sqlite engine — that is what keeps a default install
# free of a C toolchain — and it otherwise takes priority over PATH, so
# without this an operator who ran `cargo install kmp-mcp --features sqlite`
# could not reach the engine through the plugin at all. It selects the
# executable and nothing else: the backend below and the kernel's own data
# directory resolution still apply.
BINARY="${KMP_MCP_BIN:-${PLUGIN_ROOT}/bin/kmp-mcp}"
if [[ -n "${KMP_MCP_BIN:-}" && ! -x "${KMP_MCP_BIN}" ]]; then
  echo "KMP plugin: KMP_MCP_BIN is set to '${KMP_MCP_BIN}', which is not executable." >&2
  exit 127
fi

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
  echo "KMP plugin: or name one directly with KMP_MCP_BIN." >&2
  echo "KMP plugin: install one with 'cargo install kmp-mcp', or install the plugin from a release package." >&2
  exit 127
fi

export KMP_MCP_BACKEND=embedded

# The data directory is deliberately NOT set here: the embedded kernel
# resolves it itself — KMP_MCP_DATA_DIR when the operator says so, the
# enclosing project root when there is one, the per-user data home
# otherwise. A plugin that picked a location would override that doctrine.
exec "${BINARY}"
