#!/usr/bin/env bash
set -euo pipefail

VERSION="1.8.1"
DESTINATION="${1:-${PWD}/tmp/mcp-publisher/bin}"
case "$(uname -s):$(uname -m)" in
  Linux:x86_64|Linux:amd64)
    ASSET="mcp-publisher_linux_amd64.tar.gz"
    SHA256="a06c9096dcb9727c13555b6be26c7effa707b01f06a4c561ba7a3635443cf2cc"
    ;;
  Linux:aarch64|Linux:arm64)
    ASSET="mcp-publisher_linux_arm64.tar.gz"
    SHA256="8dd75a6cf6845688b5d4e46df58d3ca26d5c8d233bb0626606e1db82c5e883e4"
    ;;
  *)
    echo "install-mcp-publisher: unsupported host $(uname -s) $(uname -m)" >&2
    exit 1
    ;;
esac
URL="https://github.com/modelcontextprotocol/registry/releases/download/v${VERSION}/${ASSET}"

mkdir -p "${DESTINATION}"
work="$(mktemp -d "${DESTINATION}/.download.XXXXXX")"
trap 'rm -rf "${work}"' EXIT

curl --proto '=https' --tlsv1.2 -fsSL "${URL}" -o "${work}/${ASSET}"
printf '%s  %s\n' "${SHA256}" "${work}/${ASSET}" | sha256sum --check --status || {
  echo "install-mcp-publisher: checksum mismatch for ${URL}" >&2
  exit 1
}
tar -xzf "${work}/${ASSET}" -C "${DESTINATION}" mcp-publisher
"${DESTINATION}/mcp-publisher" --version
