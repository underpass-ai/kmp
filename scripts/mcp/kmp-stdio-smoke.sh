#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"

MCP_BIN="${KMP_MCP_BIN:-kmp-mcp}"

if [[ -n "${KMP_KERNEL_GRPC_ENDPOINT:-}" ]]; then
  REF="${KMP_MCP_SMOKE_REF:-node:mission:engine-core-failure}"
  REQUEST=$(printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kernel_inspect","arguments":{"ref":"%s"}}}' "${REF}")
  EXPECTED='"isError":false'
else
  if [[ "${KMP_MCP_BACKEND:-}" != "fixture" ]]; then
    echo "MCP smoke requires KMP_KERNEL_GRPC_ENDPOINT for live mode or KMP_MCP_BACKEND=fixture for fixture mode" >&2
    exit 2
  fi
  REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kernel_ask","arguments":{"about":"question:830ce83f","question":"Where did Rachel move after her recent relocation?","answer_policy":"evidence_or_unknown"}}}'
  EXPECTED='"answer":"Austin"'
fi

RESPONSE="$(printf '%s\n' "${REQUEST}" | "${MCP_BIN}")"

printf '%s\n' "${RESPONSE}"

if ! grep -q '"jsonrpc":"2.0"' <<<"${RESPONSE}"; then
  echo "MCP smoke failed: missing JSON-RPC response" >&2
  exit 1
fi

if grep -q '"isError":true' <<<"${RESPONSE}"; then
  echo "MCP smoke failed: tool returned isError=true" >&2
  exit 1
fi

if ! grep -q "${EXPECTED}" <<<"${RESPONSE}"; then
  echo "MCP smoke failed: expected marker ${EXPECTED}" >&2
  exit 1
fi

echo "MCP smoke passed" >&2
