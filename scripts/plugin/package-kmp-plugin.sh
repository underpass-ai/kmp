#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="${KMP_PLUGIN_PREBUILT_BINARY:-}"
if [ -z "$binary" ]; then
  cargo build --release --locked -p kmp-mcp --features sqlite --manifest-path "$root/Cargo.toml"
  binary="$root/target/release/kmp-mcp"
  [ -f "$binary" ] || binary="${binary}.exe"
fi

arguments=(plugin package --root "$root" --binary "$binary")
if [ "${KMP_PLUGIN_PACKAGE_RELEASE:-0}" = "1" ] || [[ "${GITHUB_REF_NAME:-}" == v* ]]; then
  arguments+=(--release)
fi

exec cargo run --locked --quiet --manifest-path "$root/Cargo.toml" -p kmp-release -- "${arguments[@]}"
