#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
: "${KMP_MCP_HTTP_URL:?set KMP_MCP_HTTP_URL, for example https://kmp.underpassai.com/mcp}"
: "${KMP_MCP_HTTP_TOKEN:?set a short-lived bearer token}"
: "${KMP_MCP_SMOKE_ABOUT:?set an isolated about granted by the token}"
: "${KMP_KERNEL_GRPC_ENDPOINT:?set the live KernelMemoryService endpoint}"
: "${KMP_KERNEL_GRPC_TLS_CA_PATH:?set the gRPC CA file}"
: "${KMP_KERNEL_GRPC_TLS_CERT_PATH:?set the gRPC client certificate}"
: "${KMP_KERNEL_GRPC_TLS_KEY_PATH:?set the gRPC client key}"

KMP_MCP_BIN="${KMP_MCP_BIN:-${ROOT_DIR}/target/debug/kmp-mcp}"
if [[ ! -x "${KMP_MCP_BIN}" ]]; then
  cargo build --locked -p kmp-mcp --bin kmp-mcp
fi

mkdir -p "${ROOT_DIR}/tmp"
SCRATCH_DIR="$(mktemp -d "${ROOT_DIR}/tmp/mcp-http-live-smoke.XXXXXX")"
trap 'rm -rf "${SCRATCH_DIR}"' EXIT

METADATA_URL="${KMP_MCP_HTTP_URL%/mcp}/.well-known/oauth-protected-resource"
curl --fail --silent --show-error "${METADATA_URL}" >"${SCRATCH_DIR}/metadata.json"
jq -e --arg resource "${KMP_MCP_HTTP_URL}" '.resource == $resource' \
  "${SCRATCH_DIR}/metadata.json" >/dev/null

unauthorized_status="$(curl --silent --output "${SCRATCH_DIR}/unauthorized.json" \
  --write-out '%{http_code}' \
  --header 'Content-Type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  "${KMP_MCP_HTTP_URL}")"
[[ "${unauthorized_status}" == "401" ]]

curl --fail --silent --show-error \
  --header 'Content-Type: application/json' \
  --header "Authorization: Bearer ${KMP_MCP_HTTP_TOKEN}" \
  --data '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  "${KMP_MCP_HTTP_URL}" >"${SCRATCH_DIR}/tools.json"
jq -e '.result.tools | length == 13' "${SCRATCH_DIR}/tools.json" >/dev/null

jq -cn --arg about "${KMP_MCP_SMOKE_ABOUT}" \
  '{jsonrpc:"2.0",id:3,method:"tools/call",params:{name:"kmp_wake",arguments:{about:$about,budget:{detail:"balanced",max_bytes:10000}}}}' \
  >"${SCRATCH_DIR}/wake.request.json"
curl --fail --silent --show-error \
  --header 'Content-Type: application/json' \
  --header "Authorization: Bearer ${KMP_MCP_HTTP_TOKEN}" \
  --data-binary "@${SCRATCH_DIR}/wake.request.json" \
  "${KMP_MCP_HTTP_URL}" >"${SCRATCH_DIR}/wake.http.json"

KMP_MCP_BACKEND=grpc \
KMP_KERNEL_GRPC_TLS_MODE=mutual \
"${KMP_MCP_BIN}" <"${SCRATCH_DIR}/wake.request.json" >"${SCRATCH_DIR}/wake.grpc.json"

jq -S '.result' "${SCRATCH_DIR}/wake.http.json" >"${SCRATCH_DIR}/wake.http.normalized.json"
jq -S '.result' "${SCRATCH_DIR}/wake.grpc.json" >"${SCRATCH_DIR}/wake.grpc.normalized.json"
cmp "${SCRATCH_DIR}/wake.http.normalized.json" "${SCRATCH_DIR}/wake.grpc.normalized.json"

echo "mcp-http live smoke: 10 tools, auth boundary, and live gRPC/HTTP Wake parity passed"
