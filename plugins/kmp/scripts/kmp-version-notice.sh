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
fi
exit 0
