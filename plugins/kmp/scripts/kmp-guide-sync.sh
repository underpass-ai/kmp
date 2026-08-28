#!/usr/bin/env bash
# Converge the versioned agent and human guides into the selected KMP store.

set -euo pipefail

PLUGIN_ROOT="${KMP_GUIDE_PLUGIN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
BUILDER="${PLUGIN_ROOT}/guide/build-guide.py"
REQUESTS="${PLUGIN_ROOT}/guide/guide.requests.json"
BUNDLE="${PLUGIN_ROOT}/guide/memory.jsonl"
DRY_RUN=0
BIN="${KMP_MCP_BIN:-}"

while [ $# -gt 0 ]; do
  case "$1" in
    sync) shift ;;
    --binary) BIN="${2:?--binary needs a path}"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help)
      printf '%s\n' \
        'usage: kmp-guide-sync.sh [sync] [--binary PATH] [--dry-run]' \
        '' \
        'sync converges guide:kmp-agent and guide:kmp to this plugin version.'
      exit 0
      ;;
    *) printf 'kmp-guide-sync: unknown argument %s\n' "$1" >&2; exit 2 ;;
  esac
done

if [ -z "$BIN" ]; then
  if [ -x "${PLUGIN_ROOT}/bin/kmp-mcp" ]; then
    BIN="${PLUGIN_ROOT}/bin/kmp-mcp"
  elif command -v kmp-mcp >/dev/null 2>&1; then
    BIN="$(command -v kmp-mcp)"
  else
    echo 'kmp-guide-sync: no kmp-mcp binary found; run kmp-setup first' >&2
    exit 1
  fi
fi

for asset in "$PLUGIN_ROOT/capabilities.json" "$BUILDER" "$REQUESTS" "$BUNDLE"; do
  [ -f "$asset" ] || {
    echo "kmp-guide-sync: installed guide asset is missing: $asset" >&2
    exit 1
  }
done

if [ "$DRY_RUN" -eq 1 ]; then
  printf 'would: validate and converge guide:kmp-agent and guide:kmp with %s\n' "$BIN"
  exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CURRENT="$WORK/current.jsonl"

if ! KMP_VIEWER_ADDR=off "$BIN" export "$CURRENT" >/dev/null 2>"$WORK/export.err"; then
  echo 'kmp-guide-sync: could not inspect the selected store' >&2
  sed 's/^/  /' "$WORK/export.err" >&2
  exit 1
fi

EVENT_COUNT="$(python3 - "$CURRENT" <<'PY'
import json
import pathlib
import sys

with pathlib.Path(sys.argv[1]).open(encoding="utf-8") as handle:
    print(json.loads(handle.readline())["event_count"])
PY
)"

if [ "$EVENT_COUNT" -eq 0 ]; then
  if KMP_VIEWER_ADDR=off "$BIN" import "$BUNDLE" >/dev/null 2>"$WORK/import.err"; then
    echo 'KMP guide: installed guide:kmp-agent and guide:kmp into the empty store'
    exit 0
  fi

  # Another process may have won the first-write race after our empty export.
  # Import remains restore-only; when the store is no longer empty, converge
  # through the same public kmp_ingest path used for every later update.
  if ! KMP_VIEWER_ADDR=off "$BIN" export "$CURRENT" >/dev/null 2>"$WORK/recheck.err"; then
    sed 's/^/  /' "$WORK/import.err" >&2
    sed 's/^/  /' "$WORK/recheck.err" >&2
    exit 1
  fi
  EVENT_COUNT="$(python3 - "$CURRENT" <<'PY'
import json
import pathlib
import sys

with pathlib.Path(sys.argv[1]).open(encoding="utf-8") as handle:
    print(json.loads(handle.readline())["event_count"])
PY
)"
  if [ "$EVENT_COUNT" -eq 0 ]; then
    echo 'kmp-guide-sync: guide import failed while the store remained empty' >&2
    sed 's/^/  /' "$WORK/import.err" >&2
    exit 1
  fi
fi

python3 "$BUILDER" apply --binary "$BIN"
