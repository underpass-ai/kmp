#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PLUGIN_DIR="${ROOT_DIR}/plugins/kmp"
BINARY="${ROOT_DIR}/target/release/kmp-mcp"

cd "${ROOT_DIR}"
cargo build --release --locked -p kmp-mcp
mkdir -p "${PLUGIN_DIR}/bin"
if [[ -f "${BINARY}" ]]; then
  cp "${BINARY}" "${PLUGIN_DIR}/bin/kmp-mcp"
  chmod +x "${PLUGIN_DIR}/bin/kmp-mcp"
fi
if [[ -f "${BINARY}.exe" ]]; then
  cp "${BINARY}.exe" "${PLUGIN_DIR}/bin/kmp-mcp.exe"
fi

echo "KMP plugin bundle ready at ${PLUGIN_DIR}"
