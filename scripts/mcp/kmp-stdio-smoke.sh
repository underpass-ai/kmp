#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"

MCP_BIN="${KMP_MCP_BIN:-kmp-mcp}"

if [[ -n "${KMP_KERNEL_GRPC_ENDPOINT:-}" ]]; then
  ABOUT="${KMP_MCP_SMOKE_ABOUT:-project:kmp}"
  REF="${KMP_MCP_SMOKE_REF:-node:mission:engine-core-failure}"
  REQUEST=$(printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kmp_inspect","arguments":{"about":"%s","ref":"%s"}}}' "${ABOUT}" "${REF}")
  EXPECTED='"isError":false'
elif [[ "${KMP_MCP_BACKEND:-}" == "fixture" ]]; then
  # The canned backend of a debug or eval-hooks build.
  REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kmp_ask","arguments":{"about":"question:830ce83f","question":"Where did Rachel move after her recent relocation?","answer_policy":"evidence_or_unknown"}}}'
  EXPECTED='"answer":"Austin"'
else
  # A release binary has no fixture: smoke the embedded kernel on a store of
  # its own, which an empty wake answers.
  SMOKE_STORE="$(mktemp -d)"
  trap 'rm -rf "${SMOKE_STORE}"' EXIT
  export KMP_MCP_BACKEND=embedded KMP_MCP_DATA_DIR="${SMOKE_STORE}" KMP_VIEWER_ADDR=off
  REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kmp_wake","arguments":{"about":"project:smoke"}}}'
  EXPECTED='"isError":false'
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
