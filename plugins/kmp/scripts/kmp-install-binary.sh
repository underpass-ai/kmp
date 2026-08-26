#!/usr/bin/env bash
#
# kmp-install-binary — put the engine this plugin was tested against on this
# machine.
#
# Installing the plugin from a marketplace brings text: commands, skills, the
# launcher. It cannot bring a compiled binary, because a marketplace
# distributes a git repository and a binary is per platform. So the engine
# arrives separately, and the two drift.
#
# This closes that: it installs the binary whose version matches the plugin's
# own, from the release that published it, verified against the checksum
# published beside it. No Rust toolchain required. `cargo install` remains the
# fallback for a platform with no published asset.
#
# Usage: kmp-install-binary.sh [--dir <install-dir>] [--also-dir <install-dir>] [--version <X.Y.Z>]

set -euo pipefail

REPO="underpass-ai/kmp"
INSTALL_DIR="${KMP_INSTALL_DIR:-$HOME/.local/bin}"
ALSO_DIRS=()
WANT=""

while [ $# -gt 0 ]; do
  case "$1" in
    --dir) INSTALL_DIR="${2:?--dir needs a path}"; shift 2 ;;
    --also-dir) ALSO_DIRS+=("${2:?--also-dir needs a path}"); shift 2 ;;
    --version) WANT="${2:?--version needs X.Y.Z}"; shift 2 ;;
    -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
    *) echo "kmp-install-binary: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

say()  { printf '  %s\n' "$1"; }
die()  { printf 'kmp-install-binary: %s\n' "$1" >&2; exit 1; }

# The version to install is the plugin's own, so the engine and the launcher
# that starts it cannot disagree.
if [ -z "$WANT" ]; then
  for candidate in \
    "$(dirname "${BASH_SOURCE[0]}")/../.claude-plugin/plugin.json" \
    "${CLAUDE_PLUGIN_ROOT:-}/.claude-plugin/plugin.json"; do
    if [ -f "$candidate" ]; then
      WANT="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version"])' \
        "$candidate" 2>/dev/null || true)"
      [ -n "$WANT" ] && break
    fi
  done
fi
[ -n "$WANT" ] || die "cannot tell which version to install; pass --version X.Y.Z"

os="$(uname -s)"
arch="$(uname -m)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
case "${os}-${arch}" in
  Linux-x86_64)   target="x86_64-unknown-linux-gnu" ;;
  Linux-aarch64)  target="aarch64-unknown-linux-gnu" ;;
  Darwin-arm64)   target="aarch64-apple-darwin" ;;
  Darwin-x86_64)  target="x86_64-apple-darwin" ;;
  *)
    say "no published binary for ${os}-${arch}; building it instead"
    command -v cargo >/dev/null 2>&1 \
      || die "no published binary for ${os}-${arch} and no cargo to build one"
    cargo install kmp-mcp --version "$WANT" --force --root "${work}/cargo-root"
    cp "${work}/cargo-root/bin/kmp-mcp" "${work}/kmp-mcp"
    ;;
esac

if [ -n "${target:-}" ]; then
  asset="kmp-mcp-v${WANT}-${target}"
  base="https://github.com/${REPO}/releases/download/v${WANT}"

  say "downloading ${asset}"
  # https only, and the checksum is fetched from the same release: a binary
  # that does not match what was published is not installed.
  curl --proto '=https' --tlsv1.2 -sSfL "${base}/${asset}" -o "${work}/kmp-mcp" \
    || die "could not download ${base}/${asset}"
  curl --proto '=https' --tlsv1.2 -sSfL "${base}/${asset}.sha256" -o "${work}/kmp-mcp.sha256" \
    || die "could not download the checksum for ${asset}"

  published="$(awk '{print $1}' "${work}/kmp-mcp.sha256")"
  downloaded="$(sha256sum "${work}/kmp-mcp" | awk '{print $1}')"
  if [ "$published" != "$downloaded" ]; then
    die "checksum mismatch for ${asset}: published ${published}, downloaded ${downloaded}"
  fi
  say "checksum verified"
fi

mkdir -p "$INSTALL_DIR"
install -m 755 "${work}/kmp-mcp" "${INSTALL_DIR}/kmp-mcp"
say "installed ${INSTALL_DIR}/kmp-mcp"

# A native Codex plugin can carry its own engine while the same release also
# remains available to the CLI and other hosts on PATH. Install both from the
# one checksum-verified download so neither half can drift during an update.
for also_dir in "${ALSO_DIRS[@]}"; do
  if [ "$also_dir" != "$INSTALL_DIR" ]; then
    mkdir -p "$also_dir"
    install -m 755 "${work}/kmp-mcp" "${also_dir}/kmp-mcp"
    say "installed ${also_dir}/kmp-mcp"
  fi
done

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *) say "note: ${INSTALL_DIR} is not on PATH — add it with:"
     say "  export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
esac

"${INSTALL_DIR}/kmp-mcp" --version
