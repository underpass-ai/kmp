#!/usr/bin/env bash

# E4 acceptance demo: session 1 records an architectural decision in the
# embedded kernel and dies; session 2 (a fresh process) recovers it with
# proof. No infrastructure, no network — one binary, one data dir.

set -euo pipefail

BIN="${KMP_MCP_BIN:-target/release/kmp-mcp}"
if [[ $# -gt 0 ]]; then
  DATA_DIR="$1"
else
  DATA_DIR="$(mktemp -d)"
  trap 'rm -rf "${DATA_DIR}"' EXIT
fi
export KMP_MCP_BACKEND=embedded
export KMP_MCP_DATA_DIR="${DATA_DIR}"

call() { # $1 = json-rpc line; runs ONE fresh binary session per call
  printf '%s\n' "$1" | "${BIN}" 2>/dev/null
}

echo "== session 1: record the decision (then the process dies) =="
INGEST='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kmp_ingest","arguments":{
  "about":"project:demo","idempotency_key":"ingest:demo-decision-1",
  "memory":{"dimensions":[{"id":"project:demo:decisions:arch","kind":"decision"}],
    "entries":[{"id":"project:demo:decision:sqlite","kind":"decision",
      "text":"We chose SQLite for the embedded store; safe concurrent sessions are the load-bearing criterion.",
      "coordinates":[{"dimension":"decision","scope_id":"project:demo:decisions:arch",
        "occurred_at":"2026-07-22T12:00:00Z","sequence":1}]}],
    "relations":[],
    "evidence":[{"id":"evidence:project:demo:sqlite-concurrency","supports":["project:demo:decision:sqlite"],
      "text":"Regression: two independent processes can share one SQLite store without losing events.",
      "source":"crates/kmp-adapter-embedded/tests/two_writers_one_store.rs"}]}}}}'
call "$(echo "$INGEST" | tr -d '\n')" | grep -o '"read_after_write_ready":[a-z]*'

echo "== session 2: a fresh process recovers the memory =="
WAKE='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"kmp_wake","arguments":{"about":"project:demo"}}}'
call "${WAKE}" | grep -o 'project:demo:decision:sqlite' | head -1

echo "== session 3: audit the decision with its proof =="
INSPECT='{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"kmp_inspect","arguments":{"about":"project:demo","ref":"project:demo:decision:sqlite","include":{"incoming":true,"details":true}}}}'
call "${INSPECT}" | grep -o 'evidence:project:demo:sqlite-concurrency' | head -1

echo "DEMO OK — memory survived three independent processes at ${DATA_DIR}"
