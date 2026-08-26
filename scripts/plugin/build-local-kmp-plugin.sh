#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PLUGIN_DIR="${ROOT_DIR}/plugins/kmp"
BINARY="${ROOT_DIR}/target/release/kmp-mcp"
PREBUILT="${KMP_PLUGIN_PREBUILT_BINARY:-}"

cd "${ROOT_DIR}"
mkdir -p "${PLUGIN_DIR}/bin"
if [[ -n "${PREBUILT}" ]]; then
  [[ -f "${PREBUILT}" ]] || {
    echo "KMP plugin bundle: prebuilt binary does not exist: ${PREBUILT}" >&2
    exit 1
  }
  case "${PREBUILT}" in
    *.exe) cp "${PREBUILT}" "${PLUGIN_DIR}/bin/kmp-mcp.exe" ;;
    *)     cp "${PREBUILT}" "${PLUGIN_DIR}/bin/kmp-mcp"
           chmod +x "${PLUGIN_DIR}/bin/kmp-mcp" ;;
  esac
else
  cargo build --release --locked -p kmp-mcp --features sqlite
  if [[ -f "${BINARY}" ]]; then
    cp "${BINARY}" "${PLUGIN_DIR}/bin/kmp-mcp"
    chmod +x "${PLUGIN_DIR}/bin/kmp-mcp"
  fi
  if [[ -f "${BINARY}.exe" ]]; then
    cp "${BINARY}.exe" "${PLUGIN_DIR}/bin/kmp-mcp.exe"
  fi
fi

echo "KMP plugin bundle ready at ${PLUGIN_DIR}"
