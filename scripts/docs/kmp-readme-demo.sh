#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${KMP_DEMO_BIN:-}"
if [[ -z "${BIN}" ]]; then
  for candidate in "${ROOT_DIR}/target/release/kmp-mcp" "${ROOT_DIR}/target/debug/kmp-mcp"; do
    if [[ -x "${candidate}" ]]; then
      BIN="${candidate}"
      break
    fi
  done
fi
if [[ -z "${BIN}" || ! -x "${BIN}" ]]; then
  cargo build --quiet --locked -p kmp-mcp --manifest-path "${ROOT_DIR}/Cargo.toml"
  BIN="${ROOT_DIR}/target/debug/kmp-mcp"
fi
DATA_DIR="${ROOT_DIR}/tmp/readme-gif"
BUNDLE="${ROOT_DIR}/plugins/kmp/demo/checkout-latency.jsonl"

rm -rf "${DATA_DIR}"
mkdir -p "${DATA_DIR}"
export KMP_MCP_DATA_DIR="${DATA_DIR}"
export KMP_VIEWER_ADDR=off

# The three voices wear three stops of the mark's gradient.
you=$'\033[38;2;52;141;192m'
agent=$'\033[38;2;129;91;240m'
kmp=$'\033[38;2;27;175;122m'
accent=$'\033[1;38;2;72;120;224m'
dim=$'\033[2m'
reset=$'\033[0m'

# The mark and the version line, exactly as `kmp-mcp info` draws them.
CLICOLOR_FORCE=1 "${BIN}" info 2>/dev/null | head -8
printf '\n'
sleep 1.2

printf '%syou   >%s Load the incident memory.\n' "${you}" "${reset}"
sleep 0.9
printf '%sagent >%s kmp-mcp import checkout-latency.jsonl\n' "${agent}" "${reset}"
sleep 0.6
"${BIN}" import "${BUNDLE}" >/dev/null
sleep 2.6
printf '\n'

printf '%syou   >%s Why did the rollback not fix checkout latency?\n' "${you}" "${reset}"
sleep 1
printf '%sagent >%s kmp_ask  about: incident:checkout-latency\n' "${agent}" "${reset}"
sleep 0.8

initialize='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"readme-demo","version":"1"}}}'
ask_request='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"kmp_ask","arguments":{"about":"incident:checkout-latency","question":"Why did the rollback not fix the latency?","answer_policy":"evidence_or_unknown","budget":{"detail":"balanced","max_bytes":10000}}}}'
ask_response="$(printf '%s\n%s\n' "${initialize}" "${ask_request}" | "${BIN}" 2>/dev/null | jq -c 'select(.id==2) | .result.structuredContent')"
confidence="$(jq -r '.proof.confidence' <<<"${ask_response}")"
evidence="$(jq -r '.proof.evidence[1].text' <<<"${ask_response}")"

printf '%skmp   <%s confidence: %s · evidence found\n' "${kmp}" "${reset}" "${confidence}"
sleep 0.5
printf '        %s%s%s\n' "${dim}" "${evidence/ measured at the gateway/}" "${reset}"
sleep 1.2
printf '%sagent <%s The rollback changed pool size, but retries still amplified\n' "${agent}" "${reset}"
printf '        each slow request 6.1x. More capacity was consumed just as fast.\n\n'
sleep 2.8

printf '%syou   >%s Remember: KMP stays local-first by default.\n' "${you}" "${reset}"
sleep 0.9
printf '%sagent >%s kmp_write_memory  decision + evidence\n' "${agent}" "${reset}"
sleep 0.8

now="$(date --utc +%Y-%m-%dT%H:%M:%SZ)"
write_request="$(jq -cn --arg now "${now}" '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"kmp_write_memory",arguments:{about:"project:readme-demo",intent:"record_decision",actor:"agent:readme-demo",observed_at:$now,scope:{task:"project:readme-demo",process:"readme-demo"},current:{kind:"decision",summary:"KMP stays local-first by default.",evidence:"The normal product path runs an embedded kernel over local stdio and a local store."},idempotency_key:"readme-demo:local-first",options:{dry_run:false,strict:true}}}}')"
write_response="$(printf '%s\n%s\n' "${initialize}" "${write_request}" | "${BIN}" 2>/dev/null | jq -c 'select(.id==2) | .result.structuredContent')"
accepted="$(jq -r '.accepted' <<<"${write_response}")"
ready="$(jq -r '.ingest_result.memory.read_after_write_ready' <<<"${write_response}")"

printf '%skmp   <%s accepted: %s · read_after_write_ready: %s\n' "${kmp}" "${reset}" "${accepted}" "${ready}"
sleep 0.5
printf '        %sstore: .kernel/ · engine: sqlite · still on your machine%s\n\n' "${dim}" "${reset}"
sleep 2

printf '%s▌KMP▐%s %stime travel over a graph, proofs attached%s %s────────────────────%s\n' "${accent}" "${reset}" "${dim}" "${reset}" "${dim}" "${reset}"
sleep 4
