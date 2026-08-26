#!/usr/bin/env bash
set -euo pipefail

PLUGIN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# An explicit binary is an operator override and wins without a version gate.
# Every automatically selected binary below must match the plugin manifest.
BINARY="${KMP_MCP_BIN:-}"
if [[ -n "${KMP_MCP_BIN:-}" && ! -x "${KMP_MCP_BIN}" ]]; then
  echo "KMP plugin: KMP_MCP_BIN is set to '${KMP_MCP_BIN}', which is not executable." >&2
  exit 127
fi

binary_version() {
  local output
  if ! output="$("$1" --version 2>/dev/null)"; then
    return 1
  fi
  if [[ "${output}" =~ ^kmp-mcp[[:space:]]+([^[:space:]]+) ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return 0
  fi
  return 1
}

if [[ -z "${BINARY}" ]]; then
  PLUGIN_VERSION=""
  for manifest in \
    "${PLUGIN_ROOT}/.codex-plugin/plugin.json" \
    "${PLUGIN_ROOT}/.claude-plugin/plugin.json"; do
    if [[ -f "${manifest}" ]]; then
      PLUGIN_VERSION="$(sed -nE 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' "${manifest}")"
      [[ -n "${PLUGIN_VERSION}" ]] && break
    fi
  done
  if [[ -z "${PLUGIN_VERSION}" ]]; then
    echo "KMP plugin: cannot read its version from either plugin manifest." >&2
    exit 127
  fi
  EXPECTED_ENGINE_VERSION="${PLUGIN_VERSION%%+*}"

  BUNDLED_BINARY="${PLUGIN_ROOT}/bin/kmp-mcp"
  if [[ ! -x "${BUNDLED_BINARY}" && -x "${PLUGIN_ROOT}/bin/kmp-mcp.exe" ]]; then
    BUNDLED_BINARY="${PLUGIN_ROOT}/bin/kmp-mcp.exe"
  fi
  PATH_BINARY="$(command -v kmp-mcp 2>/dev/null || true)"

  BUNDLED_VERSION=""
  if [[ -x "${BUNDLED_BINARY}" ]]; then
    BUNDLED_VERSION="$(binary_version "${BUNDLED_BINARY}" || printf 'unknown\n')"
    if [[ "${BUNDLED_VERSION}" == "${EXPECTED_ENGINE_VERSION}" ]]; then
      BINARY="${BUNDLED_BINARY}"
    fi
  fi

  if [[ -z "${BINARY}" && -n "${PATH_BINARY}" && -x "${PATH_BINARY}" ]]; then
    PATH_VERSION="$(binary_version "${PATH_BINARY}" || true)"
    if [[ "${PATH_VERSION}" == "${EXPECTED_ENGINE_VERSION}" ]]; then
      BINARY="${PATH_BINARY}"
      if [[ -n "${BUNDLED_VERSION}" && "${BUNDLED_VERSION}" != "${EXPECTED_ENGINE_VERSION}" ]]; then
        echo "KMP plugin: cache engine ${BUNDLED_VERSION} does not match plugin ${PLUGIN_VERSION}; using matching PATH engine." >&2
        echo "KMP plugin: run kmp setup to repair the plugin-owned engine." >&2
      fi
    fi
  fi

  if [[ -z "${BINARY}" && -n "${BUNDLED_VERSION}" ]]; then
    echo "KMP plugin: cache engine ${BUNDLED_VERSION} does not match plugin ${PLUGIN_VERSION}." >&2
    echo "KMP plugin: no ${EXPECTED_ENGINE_VERSION} engine was found on PATH; run kmp setup." >&2
    exit 127
  fi
  if [[ -z "${BINARY}" && -n "${PATH_BINARY}" ]]; then
    echo "KMP plugin: the PATH engine does not match plugin ${PLUGIN_VERSION}." >&2
    echo "KMP plugin: run kmp setup to install engine ${EXPECTED_ENGINE_VERSION}." >&2
    exit 127
  fi
fi

if [[ ! -x "${BINARY}" ]]; then
  echo "KMP plugin: no kmp-mcp executable found." >&2
  echo "KMP plugin: looked for ${PLUGIN_ROOT}/bin/kmp-mcp (release bundle) and kmp-mcp on PATH." >&2
  echo "KMP plugin: or name one directly with KMP_MCP_BIN." >&2
  echo "KMP plugin: install one with 'cargo install kmp-mcp', or install the plugin from a release package." >&2
  exit 127
fi

# Belt and braces since 0.1.14, not a requirement: an unconfigured kmp-mcp
# now serves the embedded kernel on its own. It stays because this launcher
# ships with the plugin and can meet an older binary, which would otherwise
# default to gRPC and exit asking for an endpoint nobody set.
export KMP_MCP_BACKEND=embedded

# The data directory is deliberately NOT set here: the embedded kernel
# resolves it itself — KMP_MCP_DATA_DIR when the operator says so, the
# enclosing project root when there is one, the per-user data home
# otherwise. A plugin that picked a location would override that doctrine.
exec "${BINARY}"
