#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${KMP_DEMO_BIN:-${ROOT_DIR}/target/debug/kmp-mcp}"
DATA_DIR="${ROOT_DIR}/tmp/readme-gif"
BUNDLE="${ROOT_DIR}/plugins/kmp/demo/checkout-latency.jsonl"

if [[ ! -x "${BIN}" ]]; then
  cargo build --quiet --locked -p kmp-mcp --manifest-path "${ROOT_DIR}/Cargo.toml"
fi

rm -rf "${DATA_DIR}"
mkdir -p "${DATA_DIR}"
export KMP_MCP_DATA_DIR="${DATA_DIR}"
export KMP_VIEWER_ADDR=off

"${BIN}" import "${BUNDLE}" >/dev/null 2>&1

cyan=$'\033[38;5;81m'
green=$'\033[38;5;84m'
purple=$'\033[38;5;141m'
dim=$'\033[2m'
bold=$'\033[1m'
reset=$'\033[0m'

printf '%sKMP%s  local memory, actual evidence, zero cloud detours\n\n' "${bold}${purple}" "${reset}"
sleep 0.8
printf '%syou   >%s Why did the rollback not fix checkout latency?\n' "${cyan}" "${reset}"
sleep 1
printf '%sagent >%s kmp_ask  about: incident:checkout-latency\n' "${purple}" "${reset}"
sleep 0.8

initialize='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"readme-demo","version":"1"}}}'
ask_request='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"kmp_ask","arguments":{"about":"incident:checkout-latency","question":"Why did the rollback not fix the latency?","answer_policy":"evidence_or_unknown","budget":{"detail":"balanced","max_bytes":10000}}}}'
ask_response="$(printf '%s\n%s\n' "${initialize}" "${ask_request}" | "${BIN}" 2>/dev/null | jq -c 'select(.id==2) | .result.structuredContent')"
confidence="$(jq -r '.proof.confidence' <<<"${ask_response}")"
evidence="$(jq -r '.proof.evidence[1].text' <<<"${ask_response}")"

printf '%skmp   <%s confidence: %s · evidence found\n' "${green}" "${reset}" "${confidence}"
sleep 0.5
printf '        %s%s%s\n' "${dim}" "${evidence/ measured at the gateway/}" "${reset}"
sleep 1.2
printf '%sagent <%s The rollback changed pool size, but retries still amplified\n' "${purple}" "${reset}"
printf '        each slow request 6.1x. More capacity was consumed just as fast.\n\n'
sleep 1.4

printf '%syou   >%s Remember: KMP stays local-first by default.\n' "${cyan}" "${reset}"
sleep 0.9
printf '%sagent >%s kmp_write_memory  decision + evidence\n' "${purple}" "${reset}"
sleep 0.8

now="$(date --utc +%Y-%m-%dT%H:%M:%SZ)"
write_request="$(jq -cn --arg now "${now}" '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"kmp_write_memory",arguments:{about:"project:readme-demo",intent:"record_decision",actor:"agent:readme-demo",observed_at:$now,scope:{task:"project:readme-demo",process:"readme-demo"},current:{kind:"decision",summary:"KMP stays local-first by default.",evidence:"The normal product path runs an embedded kernel over local stdio and a local store."},idempotency_key:"readme-demo:local-first",options:{dry_run:false,strict:true}}}}')"
write_response="$(printf '%s\n%s\n' "${initialize}" "${write_request}" | "${BIN}" 2>/dev/null | jq -c 'select(.id==2) | .result.structuredContent')"
accepted="$(jq -r '.accepted' <<<"${write_response}")"
ready="$(jq -r '.ingest_result.memory.read_after_write_ready' <<<"${write_response}")"

printf '%skmp   <%s accepted: %s · read_after_write_ready: %s\n' "${green}" "${reset}" "${accepted}" "${ready}"
sleep 0.8
printf '        %sstore: .kernel/ · engine: sqlite · still on your machine%s\n' "${dim}" "${reset}"
sleep 2
