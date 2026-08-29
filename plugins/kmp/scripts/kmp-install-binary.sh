#!/bin/sh
set -eu

plugin_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
action=setup
case "${1:-}" in
  setup|update) action=$1; shift ;;
esac

version=$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
  "$plugin_root/.codex-plugin/plugin.json" | head -n 1)
version=${version%%+*}
[ -n "$version" ] || { echo "KMP setup: plugin manifest has no version" >&2; exit 127; }
binary=${KMP_MCP_BIN:-"$plugin_root/bin/kmp-mcp"}
if [ ! -x "$binary" ]; then
  binary=$(command -v kmp-mcp 2>/dev/null || true)
fi
if [ -n "$binary" ] && [ -x "$binary" ]; then
  actual=$("$binary" --version 2>/dev/null | sed -n '1s/^kmp-mcp \([^ ]*\).*/\1/p')
  if [ "$actual" = "$version" ]; then
    exec "$binary" "$action" --version "$version" "$@"
  fi
fi

# A source marketplace cannot execute Rust before the first Rust binary
# exists. This is the sole bootstrap boundary: fetch the immutable release,
# verify its published digest, then hand every lifecycle decision to Rust.
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64) target=aarch64-unknown-linux-gnu ;;
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  *) echo "KMP setup: no bootstrap engine for this platform; use cargo install kmp-mcp" >&2; exit 127 ;;
esac

install_dir=${KMP_INSTALL_DIR:-"${HOME:?HOME is required}/.local/bin"}
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
asset="kmp-mcp-v${version}-${target}"
base="https://github.com/underpass-ai/kmp/releases/download/v${version}/${asset}"
curl --proto '=https' --tlsv1.2 -fsSL "$base" -o "$scratch/kmp-mcp"
curl --proto '=https' --tlsv1.2 -fsSL "$base.sha256" -o "$scratch/kmp-mcp.sha256"
published=$(awk 'NR == 1 { print $1 }' "$scratch/kmp-mcp.sha256")
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$scratch/kmp-mcp" | awk '{ print $1 }')
else
  actual=$(shasum -a 256 "$scratch/kmp-mcp" | awk '{ print $1 }')
fi
[ -n "$published" ] && [ "$published" = "$actual" ] || {
  echo "KMP setup: checksum mismatch for $asset" >&2
  exit 1
}
mkdir -p "$install_dir"
install -m 755 "$scratch/kmp-mcp" "$install_dir/kmp-mcp"
exec "$install_dir/kmp-mcp" "$action" --version "$version" --engine-dir "$install_dir" "$@"
