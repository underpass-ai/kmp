#!/usr/bin/env bash

set -euo pipefail

GIT_URL="${KMP_MCP_GIT_URL:-https://github.com/underpass-ai/kmp}"
BRANCH="${KMP_MCP_BRANCH:-}"
TAG="${KMP_MCP_TAG:-}"
REV="${KMP_MCP_REV:-}"

selected_refs=0
[[ -n "${BRANCH}" ]] && selected_refs=$((selected_refs + 1))
[[ -n "${TAG}" ]] && selected_refs=$((selected_refs + 1))
[[ -n "${REV}" ]] && selected_refs=$((selected_refs + 1))

if [[ "${selected_refs}" -gt 1 ]]; then
  echo "set only one of KMP_MCP_BRANCH, KMP_MCP_TAG, or KMP_MCP_REV" >&2
  exit 2
fi

cmd=(cargo install --git "${GIT_URL}" kmp-mcp --locked --force)

if [[ -n "${BRANCH}" ]]; then
  cmd+=(--branch "${BRANCH}")
elif [[ -n "${TAG}" ]]; then
  cmd+=(--tag "${TAG}")
elif [[ -n "${REV}" ]]; then
  cmd+=(--rev "${REV}")
fi

if [[ -n "${CARGO_INSTALL_ROOT:-}" ]]; then
  cmd+=(--root "${CARGO_INSTALL_ROOT}")
fi

"${cmd[@]}"
