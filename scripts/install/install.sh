#!/usr/bin/env bash

# KMP installer: drops the kmp-mcp binary into ~/.local/bin, then points at
# `kmp-mcp setup`, which installs the plugin into each host.
#
#   curl --proto '=https' --tlsv1.2 -sSfL https://raw.githubusercontent.com/underpass-ai/kmp/main/scripts/install/install.sh | bash
#
# Pin a version with:  KMP_VERSION=v<version> ./install.sh

set -euo pipefail

REPO="underpass-ai/kmp"
VERSION="${KMP_VERSION:-}"
INSTALL_DIR="${KMP_INSTALL_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch="$(uname -m)"
case "${os}-${arch}" in
  Linux-x86_64) target="x86_64-unknown-linux-gnu" ;;
  Linux-aarch64) target="aarch64-unknown-linux-gnu" ;;
  Darwin-arm64) target="aarch64-apple-darwin" ;;
  Darwin-x86_64) target="x86_64-apple-darwin" ;;
  *) echo "install: unsupported platform ${os}/${arch} — build from source: cargo install --git https://github.com/${REPO} kmp-mcp" >&2
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

ASSET="kmp-mcp-${VERSION}-${target}"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"

echo "install: fetching ${ASSET}"
tmp="$(mktemp -d)"
curl --proto '=https' --tlsv1.2 -sSfL -o "${tmp}/${ASSET}" "${URL}"
curl --proto '=https' --tlsv1.2 -sSfL -o "${tmp}/${ASSET}.sha256" "${URL}.sha256"
(cd "${tmp}" && if command -v sha256sum >/dev/null; then sha256sum -c "${ASSET}.sha256"; else shasum -a 256 -c "${ASSET}.sha256"; fi)

mkdir -p "${INSTALL_DIR}"
install -m 0755 "${tmp}/${ASSET}" "${INSTALL_DIR}/kmp-mcp"
rm -rf "${tmp}"

echo
echo "installed: ${INSTALL_DIR}/kmp-mcp (${VERSION})"
echo
echo "Connect it to your agent hosts. The KMP plugin owns the MCP registration;"
echo "do not add a second KMP server by hand."
echo
echo "    ${INSTALL_DIR}/kmp-mcp setup     # Claude Code, Codex, Hermes and Pi found on PATH"
echo "    ${INSTALL_DIR}/kmp-mcp doctor    # check the result"
echo
echo "Restart each host once afterwards. Plugin marketplace installs and per-host"
echo "options: https://github.com/${REPO}#install"
