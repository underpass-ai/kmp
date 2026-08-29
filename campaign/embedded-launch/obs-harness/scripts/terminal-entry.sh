#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 5 ]; then
  echo 'usage: terminal-entry.sh SCENARIO RUN_DIR KMP_MCP_BIN REPO_ROOT VIEWER_PORT' >&2
  exit 2
fi

scenario="$1"
run_dir="$2"
binary="$3"
repo_root="$4"
viewer_port="$5"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

printf -v command '%q ' \
  env "KMP_CAPTURE_VIEWER_PORT=${viewer_port}" \
  node "${script_dir}/mcp-pty-client.mjs" \
  "${scenario}" "${run_dir}" "${binary}" "${repo_root}"

exec script --quiet --flush \
  --log-out "${run_dir}/pty.typescript" \
  --log-timing "${run_dir}/pty.timing" \
  --command "${command}"
