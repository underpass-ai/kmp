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
install_dir=${KMP_INSTALL_DIR:-"${HOME:?HOME is required}/.local/bin"}

engine_version() {
  "$1" --version 2>/dev/null | sed -n '1s/^kmp-mcp \([^ ]*\).*/\1/p'
}

# Whether release $1 is newer than release $2, on major.minor.patch.
newer_than() {
  awk -v a="${1%%[-+]*}" -v b="${2%%[-+]*}" 'BEGIN {
    split(a, x, "."); split(b, y, ".")
    for (i = 1; i <= 3; i++) {
      if ((x[i] + 0) > (y[i] + 0)) exit 0
      if ((x[i] + 0) < (y[i] + 0)) exit 1
    }
    exit 1
  }'
}

binary=${KMP_MCP_BIN:-"$plugin_root/bin/kmp-mcp"}
if [ ! -x "$binary" ]; then
  binary=$(command -v kmp-mcp 2>/dev/null || true)
fi
if [ -n "$binary" ] && [ -x "$binary" ]; then
  actual=$(engine_version "$binary")
  if [ "$actual" = "$version" ]; then
    # The manifest selects the bootstrap engine, not an upgrade
    # destination. Setup targets this release; update lets Rust resolve
    # latest unless the caller explicitly supplies --version.
    if [ "$action" = setup ]; then set -- --version "$version" "$@"; fi
    exec "$binary" "$action" "$@"
  fi
fi

# A stale plugin never downgrades the shared engine. When the engine already
# installed is newer than this plugin's release, it runs the lifecycle for
# its own release, and nothing is downloaded or replaced.
for installed in "$binary" "$install_dir/kmp-mcp"; do
  [ -n "$installed" ] && [ -x "$installed" ] || continue
  actual=$(engine_version "$installed")
  if [ -n "$actual" ] && newer_than "$actual" "$version"; then
    echo "KMP setup: kmp-mcp $actual is already installed, newer than this plugin's $version; using it instead of installing an older engine" >&2
    exec "$installed" "$action" "$@"
  fi
done

if [ "$action" = setup ]; then
  set -- --version "$version" "$@"
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
exec "$install_dir/kmp-mcp" "$action" --engine-dir "$install_dir" "$@"
