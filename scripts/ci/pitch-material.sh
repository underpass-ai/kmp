#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MANIFEST="${ROOT}/archive/docs/showcase/claims.tsv"
BIN="${KMP_MCP_BIN:-${1:-${ROOT}/target/debug/kmp-mcp}}"

[ -x "$BIN" ] || {
  echo "pitch material: executable kmp-mcp not found at ${BIN}" >&2
  exit 1
}

FAILURES=0
COUNT=0
while IFS=$'\t' read -r claim recording tape scenario observation; do
  case "$claim" in ''|'#'*) continue ;; esac
  COUNT=$((COUNT + 1))

  for path in "$recording" "$tape" "$scenario"; do
    if [ ! -f "${ROOT}/${path}" ]; then
      echo "FAIL  ${claim}: missing ${path}" >&2
      FAILURES=$((FAILURES + 1))
    fi
  done
  if [ -f "${ROOT}/${tape}" ] && ! grep -qF "bash ${scenario}" "${ROOT}/${tape}"; then
    echo "FAIL  ${claim}: tape does not execute ${scenario}" >&2
    FAILURES=$((FAILURES + 1))
  fi
  if [ -z "$observation" ]; then
    echo "FAIL  ${claim}: no required observation" >&2
    FAILURES=$((FAILURES + 1))
    continue
  fi

  OUTPUT="$(KMP_MCP_BIN="$BIN" bash "${ROOT}/${scenario}")" || {
    echo "FAIL  ${claim}: scenario exited non-zero" >&2
    FAILURES=$((FAILURES + 1))
    continue
  }
  if ! grep -qF "$observation" <<<"$OUTPUT"; then
    echo "FAIL  ${claim}: scenario did not show '${observation}'" >&2
    FAILURES=$((FAILURES + 1))
  else
    echo "ok    ${claim} -> ${recording}"
  fi
done < "$MANIFEST"

[ "$COUNT" -gt 0 ] || {
  echo "FAIL  pitch material: no claims in ${MANIFEST}" >&2
  exit 1
}

if [ "$FAILURES" -gt 0 ]; then
  echo "pitch material: ${FAILURES} failure(s)" >&2
  exit 1
fi

echo "pitch material: ${COUNT} claims, ${COUNT} live recordings"
