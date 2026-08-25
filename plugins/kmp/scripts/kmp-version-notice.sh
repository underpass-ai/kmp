#!/usr/bin/env bash
#
# kmp-version-notice — say, once, when the engine and the plugin disagree.
#
# The binary and the plugin arrive through different commands and neither
# announces the other, so a stale half keeps working by luck: the launcher
# falls through to whatever `kmp-mcp` is on PATH. The fixes that live in the
# plugin — the launcher, the doctor, the skills — are then the ones silently
# missing.
#
# This runs at session start and after an update. It offers; it never installs.
# A hook that changes a machine while someone is opening a terminal is a
# surprise, and this is exactly the moment to not be surprising.

set -uo pipefail

manifest=""
for candidate in \
  "$(dirname "${BASH_SOURCE[0]}")/../.claude-plugin/plugin.json" \
  "${CLAUDE_PLUGIN_ROOT:-}/.claude-plugin/plugin.json"; do
  [ -f "$candidate" ] && { manifest="$candidate"; break; }
done
[ -n "$manifest" ] || exit 0

plugin_version="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version"])' \
  "$manifest" 2>/dev/null)" || exit 0
[ -n "$plugin_version" ] || exit 0

binary="${KMP_MCP_BIN:-$(command -v kmp-mcp 2>/dev/null || true)}"
if [ -z "$binary" ] || [ ! -x "$binary" ]; then
  printf 'KMP: the plugin is installed but there is no kmp-mcp engine on this machine.\n'
  printf '     Run /kmp:setup to install the %s engine this plugin expects.\n' "$plugin_version"
  exit 0
fi

binary_version="$("$binary" --version 2>/dev/null | head -1 | sed -E 's/^kmp-mcp ([^ ]+).*/\1/')"
[ -n "$binary_version" ] || exit 0

if [ "$binary_version" != "$plugin_version" ]; then
  printf 'KMP: engine %s, plugin %s. They update separately, so the plugin-side\n' \
    "$binary_version" "$plugin_version"
  printf '     fixes are the ones you are missing. Run /kmp:setup to line them up.\n'
  exit 0
fi

# A matching pair can still be old together. Check at most once a day and
# fail open when the network is absent: update discovery must never make a
# coding session slow or unavailable. Tests inject KMP_LATEST_VERSION, which
# also makes the behavior deterministic without touching the network/cache.
latest_version="${KMP_LATEST_VERSION:-}"
if [ -z "$latest_version" ] && [ "${KMP_VERSION_CHECK:-on}" != "off" ]; then
  cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/kmp"
  cache_file="${cache_dir}/latest-release"
  now="$(date +%s 2>/dev/null || printf '0')"

  if [ -f "$cache_file" ]; then
    read -r checked_at cached_version < "$cache_file" || true
    if [[ "${checked_at:-}" =~ ^[0-9]+$ ]] \
       && [ $((now - checked_at)) -lt 86400 ] 2>/dev/null; then
      latest_version="${cached_version:-}"
    fi
  fi

  if [ -z "$latest_version" ] && command -v curl >/dev/null 2>&1; then
    response="$(curl --proto '=https' --tlsv1.2 --connect-timeout 1 --max-time 2 \
      -fsSL -H 'Accept: application/vnd.github+json' -H 'User-Agent: kmp-version-notice' \
      https://api.github.com/repos/underpass-ai/kmp/releases/latest 2>/dev/null || true)"
    latest_version="$(printf '%s' "$response" | python3 -c \
      'import json,sys; print(json.load(sys.stdin).get("tag_name", "").removeprefix("v"))' \
      2>/dev/null || true)"
    if [[ "$latest_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?$ ]]; then
      mkdir -p "$cache_dir" 2>/dev/null || true
      printf '%s %s\n' "$now" "$latest_version" > "${cache_file}.tmp" 2>/dev/null \
        && mv "${cache_file}.tmp" "$cache_file" 2>/dev/null || true
    fi
  fi
fi

latest_version="${latest_version#v}"
if [[ "$latest_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?$ ]]; then
  newer="$(python3 - "$plugin_version" "$latest_version" <<'PY' 2>/dev/null || true
import re
import sys

def core(value: str) -> tuple[int, int, int]:
    match = re.match(r"^(\d+)\.(\d+)\.(\d+)", value)
    if not match:
        raise ValueError(value)
    return tuple(map(int, match.groups()))

print("yes" if core(sys.argv[2]) > core(sys.argv[1]) else "no")
PY
)"
  if [ "$newer" = "yes" ]; then
    printf 'KMP: %s is installed; %s is out. Run /kmp:setup once to update the\n' \
      "$plugin_version" "$latest_version"
    printf '     plugin and engine together, then restart the session.\n'
  fi
fi
exit 0
