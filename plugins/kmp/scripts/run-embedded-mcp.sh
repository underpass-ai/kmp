#!/bin/sh
set -eu

plugin_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ -n "${KMP_MCP_BIN:-}" ]; then
  [ -x "$KMP_MCP_BIN" ] || { echo "KMP plugin: KMP_MCP_BIN is not executable" >&2; exit 127; }
  exec "$KMP_MCP_BIN" "$@"
fi

path_engine=$(command -v kmp-mcp 2>/dev/null || true)
bundled_engine="$plugin_root/bin/kmp-mcp"
[ -x "$bundled_engine" ] || bundled_engine="$plugin_root/bin/kmp-mcp.exe"

# The plugin-owned engine is the normal case. Trying it first avoids invoking
# an older PATH binary that cannot know this resolver command yet.
for resolver in "$bundled_engine" "$path_engine"; do
  [ -n "$resolver" ] && [ -x "$resolver" ] || continue
  resolution=$(
    "$resolver" plugin resolve-engine --plugin-root "$plugin_root" \
      --path-engine "${path_engine:-/no/path/kmp-mcp}" \
      --bundled-engine "$bundled_engine"
  ) || continue
  case "$resolution" in
    KMP_ENGINE=*)
      engine=${resolution#KMP_ENGINE=}
      KMP_MCP_BACKEND=${KMP_MCP_BACKEND:-embedded}
      export KMP_MCP_BACKEND
      exec "$engine" "$@"
      ;;
  esac
done

echo "KMP plugin: no engine matches this plugin; run kmp setup" >&2
exit 127
