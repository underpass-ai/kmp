#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
exec cargo run --locked --quiet --manifest-path "${root}/Cargo.toml" \
  -p kmp-release -- workflow "$@"
