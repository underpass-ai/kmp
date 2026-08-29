#!/usr/bin/env bash

# ADR-013 "small surface" gates for the installable MCP binary:
# 1. the dependency graph must never grow infrastructure clients;
# 2. the stripped release binary must stay within the size budget.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

FORBIDDEN='neo4rs|async-nats|kmp-adapter-neo4j|kmp-adapter-valkey|kmp-adapter-nats|kmp-server|kmp-transport-grpc'
echo "embedded-gates: checking forbidden dependencies in kmp-mcp graph"
if cargo tree -p kmp-mcp --edges normal --prefix none --locked \
  | grep -E "^(${FORBIDDEN}) v"; then
  echo "embedded-gates: FORBIDDEN dependency linked into the MCP binary" >&2
  exit 1
fi

EMBEDDED_FORBIDDEN='opentelemetry|opentelemetry-otlp|prost|reqwest|tonic'
echo "embedded-gates: checking the in-process kernel observability boundary"
if cargo tree -p kmp-embedded --edges normal --prefix none --locked \
  | grep -E "^(${EMBEDDED_FORBIDDEN}) v"; then
  echo "embedded-gates: remote observability linked into the in-process kernel" >&2
  exit 1
fi

# Every installable MCP binary carries SQLite. Feature selection may remove
# optional product surfaces, but it cannot change the embedded storage engine.
for feature_args in "" "--no-default-features"; do
  label="default"
  if [ -n "${feature_args}" ]; then
    label="--no-default-features"
  fi
  echo "embedded-gates: checking ${label} kmp-mcp carries SQLite"
  for dep in rusqlite libsqlite3-sys; do
    if ! cargo tree -p kmp-mcp ${feature_args} --edges normal --prefix none --locked \
      | grep -E "^${dep} v" >/dev/null; then
      echo "embedded-gates: ${label} kmp-mcp is missing ${dep}" >&2
      exit 1
    fi
  done
done

echo "embedded-gates: building release binary"
cargo build --release -p kmp-mcp --locked
strip -o target/release/kmp-mcp.gates-stripped target/release/kmp-mcp
SIZE="$(stat -c%s target/release/kmp-mcp.gates-stripped)"
# SQLite, cl100k response accounting and the self-contained ChronoLoom MCP App
# are intentional shipped product dependencies. Keep a recorded ceiling above
# their CI baseline while the forbidden infrastructure graph stays gated above.
BUDGET=$((20 * 1024 * 1024))
echo "embedded-gates: stripped binary ${SIZE} bytes (budget ${BUDGET})"
if [ "${SIZE}" -gt "${BUDGET}" ]; then
  echo "embedded-gates: binary exceeds the recorded size budget" >&2
  exit 1
fi
