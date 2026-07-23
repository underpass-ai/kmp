#!/usr/bin/env bash

# KMP Embedded Edition installer: drops the rehydration-mcp binary into
# ~/.local/bin and prints the per-host registration snippets.
#
#   curl --proto '=https' --tlsv1.2 -sSfL https://raw.githubusercontent.com/underpass-ai/rehydration-kernel/main/scripts/install/install.sh | bash
#
# Pin a version with:  KMP_VERSION=v0.1.0 ./install.sh

set -euo pipefail

REPO="underpass-ai/rehydration-kernel"
VERSION="${KMP_VERSION:-}"
INSTALL_DIR="${KMP_INSTALL_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch="$(uname -m)"
case "${os}-${arch}" in
  Linux-x86_64) target="x86_64-unknown-linux-gnu" ;;
  Linux-aarch64) target="aarch64-unknown-linux-gnu" ;;
  Darwin-arm64) target="aarch64-apple-darwin" ;;
  Darwin-x86_64) target="x86_64-apple-darwin" ;;
  *) echo "install: unsupported platform ${os}/${arch} — build from source: cargo install --git https://github.com/${REPO} rehydration-mcp" >&2
     exit 1 ;;
esac

if [ -z "${VERSION}" ]; then
  VERSION="$(curl --proto '=https' --tlsv1.2 -sSfL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep -o '"tag_name": *"[^"]*"' | head -1 | cut -d'"' -f4)"
fi
if [ -z "${VERSION}" ]; then
  echo "install: could not resolve the latest release tag" >&2
  exit 1
fi

ASSET="rehydration-mcp-${VERSION}-${target}"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"

echo "install: fetching ${ASSET}"
tmp="$(mktemp -d)"
curl --proto '=https' --tlsv1.2 -sSfL -o "${tmp}/${ASSET}" "${URL}"
curl --proto '=https' --tlsv1.2 -sSfL -o "${tmp}/${ASSET}.sha256" "${URL}.sha256"
(cd "${tmp}" && if command -v sha256sum >/dev/null; then sha256sum -c "${ASSET}.sha256"; else shasum -a 256 -c "${ASSET}.sha256"; fi)

mkdir -p "${INSTALL_DIR}"
install -m 0755 "${tmp}/${ASSET}" "${INSTALL_DIR}/rehydration-mcp"
rm -rf "${tmp}"

echo
echo "installed: ${INSTALL_DIR}/rehydration-mcp (${VERSION})"
echo
echo "Register it in your agent host (memory is per-project by default):"
echo
echo "  Claude Code:"
echo "    claude mcp add kernel-memory --scope user \\"
echo "      --env REHYDRATION_MCP_BACKEND=embedded \\"
echo "      -- ${INSTALL_DIR}/rehydration-mcp"
echo
echo "  Codex CLI (~/.codex/config.toml):"
echo "    [mcp_servers.kernel-memory]"
echo "    command = \"${INSTALL_DIR}/rehydration-mcp\""
echo "    env = { REHYDRATION_MCP_BACKEND = \"embedded\" }"
echo
echo "Then, inside a session: kernel_wake {\"about\":\"project:<name>\"}"
