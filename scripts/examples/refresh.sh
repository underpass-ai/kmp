#!/usr/bin/env bash
# Regenerate every shipped memory bundle from its authored packets with the
# workspace binary: the examples under examples/ and this repository's own
# memory in .kmp/memory.jsonl. Run after editing a requests file or after a
# release that changes the bundle format.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"
binary="${KMP_MCP_BIN:-target/debug/kmp-mcp}"
if [[ ! -x "${binary}" ]]; then
  echo "refresh: build the engine first: cargo build --locked -p kmp-mcp" >&2
  exit 1
fi

bundle() {
  python3 -I scripts/examples/bundle.py --binary "${binary}" --requests "$1" --out "$2"
}

# What `kmp-mcp demo` writes, as a bundle a reader can import.
bundle crates/kmp-mcp/fixtures/demo/kmp-demo.requests.json examples/retry-budget/memory.jsonl
# KMP's own memory: the decisions behind this repository.
bundle examples/kmp-project/requests.json .kmp/memory.jsonl
