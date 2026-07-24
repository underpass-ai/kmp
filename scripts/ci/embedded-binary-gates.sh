#!/usr/bin/env bash

# ADR-013 "small surface" gates for the installable MCP binary:
# 1. the dependency graph must never grow infrastructure clients;
# 2. the stripped release binary must stay within the size budget.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

FORBIDDEN='neo4rs|async-nats|rehydration-adapter-neo4j|rehydration-adapter-valkey|rehydration-adapter-nats|rehydration-server|rehydration-transport-grpc'
echo "embedded-gates: checking forbidden dependencies in rehydration-mcp graph"
if cargo tree -p rehydration-mcp --edges normal --prefix none --locked \
  | grep -E "^(${FORBIDDEN}) v"; then
  echo "embedded-gates: FORBIDDEN dependency linked into the MCP binary" >&2
  exit 1
fi

EMBEDDED_FORBIDDEN='opentelemetry|opentelemetry-otlp|prost|reqwest|tonic'
echo "embedded-gates: checking the in-process kernel observability boundary"
if cargo tree -p rehydration-embedded --edges normal --prefix none --locked \
  | grep -E "^(${EMBEDDED_FORBIDDEN}) v"; then
  echo "embedded-gates: remote observability linked into the in-process kernel" >&2
  exit 1
fi

echo "embedded-gates: building release binary"
cargo build --release -p rehydration-mcp --locked
strip -o target/release/rehydration-mcp.gates-stripped target/release/rehydration-mcp
SIZE="$(stat -c%s target/release/rehydration-mcp.gates-stripped)"
BUDGET=$((16 * 1024 * 1024))
echo "embedded-gates: stripped binary ${SIZE} bytes (budget ${BUDGET})"
if [ "${SIZE}" -gt "${BUDGET}" ]; then
  echo "embedded-gates: binary exceeds the recorded size budget" >&2
  exit 1
fi
