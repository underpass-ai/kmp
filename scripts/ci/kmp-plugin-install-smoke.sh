#!/usr/bin/env bash
set -euo pipefail

# Install-shaped plugin smoke: the host's view, not the packager's.
#
# kmp-plugin-smoke.sh proves the bundle we build is well formed. It cannot
# prove the plugin starts once a host installs it, because it builds bin/
# first and then invokes the launcher by path — so it exercises neither the
# marketplace shape nor the .mcp.json declaration the host actually reads.
#
# Four shipped defects lived in exactly that gap, each leaving an install
# that looked correct and did nothing:
#
#   1. the launcher exec'd bin/kmp-mcp, which only exists in a release
#      package, so a marketplace install died with exit 127;
#   2. the plugin manifests stayed at 0.1.0 through every release, so
#      `claude plugin update` reported "already at the latest version";
#   3. .mcp.json declared `cwd: "."` with a relative command, so the host
#      spawned the launcher from wherever the session started and got
#      ENOENT.
#   4. Codex loaded that Claude-specific declaration literally, so an enabled
#      plugin tried to execute a path rooted at `${CLAUDE_PLUGIN_ROOT}`.
#
# This gate reproduces a marketplace install and starts the server the way
# the host does. It fails on all four.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PLUGIN_DIR="${ROOT_DIR}/plugins/kmp"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

INSTALLED="${WORK_DIR}/plugin"
DATA_DIR="${WORK_DIR}/data"
mkdir -p "${DATA_DIR}"

fail() {
  echo "KMP plugin install smoke: $*" >&2
  exit 1
}

# --- 1. the marketplace shape -------------------------------------------
#
# A marketplace install is the repository contents, and bin/ is gitignored.
# Copying rather than building is the point: a bundled binary would hide a
# launcher that cannot survive without one.

mkdir -p "${INSTALLED}"
while IFS= read -r tracked; do
  destination="${INSTALLED}/${tracked#plugins/kmp/}"
  mkdir -p "$(dirname "${destination}")"
  cp -p "${ROOT_DIR}/${tracked}" "${destination}"
done < <(git -C "${ROOT_DIR}" ls-files plugins/kmp)

# Tracked files only, so a local build tree cannot smuggle in a binary the
# host would never receive. bin/ is gitignored and must stay absent.
if [[ -e "${INSTALLED}/bin" ]]; then
  fail "bin/ is tracked; a marketplace install would ship a binary it should not"
fi

# --- 2. the version the host will report --------------------------------
#
# The host reads the manifest, not Cargo.toml. When they disagree, every
# install reports a version that was never released and update checks
# compare against the wrong number.

WORKSPACE_VERSION="$(
  python3 - "${ROOT_DIR}/Cargo.toml" <<'PY'
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
match = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
if not match:
    sys.exit("could not read the workspace version")
print(match.group(1))
PY
)"

for manifest in .claude-plugin .codex-plugin; do
  manifest_version="$(
    python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version"])' \
      "${INSTALLED}/${manifest}/plugin.json"
  )"
  if [[ "${manifest_version}" != "${WORKSPACE_VERSION}" ]]; then
    fail "${manifest}/plugin.json says ${manifest_version}, workspace says ${WORKSPACE_VERSION}"
  fi
done

# --- 3. the binary the fallback will find -------------------------------
#
# With no bundled bin/, the launcher falls back to PATH. Build the
# workspace binary and put it there, which is what `cargo install kmp-mcp`
# leaves a user with.

cargo build --locked --quiet -p kmp-mcp
export PATH="${ROOT_DIR}/target/debug:${PATH}"
command -v kmp-mcp >/dev/null || fail "kmp-mcp is not on PATH after building it"

# Codex resolves its own MCP declaration from the manifest. It does not
# expand Claude's plugin-root placeholder in a stdio command, so the portable
# declaration is the installed executable name.
read -r CODEX_SERVER_NAME CODEX_SERVER_COMMAND <<<"$(
  python3 - "${INSTALLED}" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
manifest = json.loads((root / ".codex-plugin/plugin.json").read_text())
servers = manifest.get("mcpServers")
if not isinstance(servers, dict):
    sys.exit(f"unexpected Codex MCP manifest declaration: {servers!r}")
if len(servers) != 1:
    sys.exit(f"expected one Codex MCP server, found {len(servers)}")
name, entry = next(iter(servers.items()))
print(name, entry.get("command", ""))
PY
)"
[[ "${CODEX_SERVER_NAME}" == "kmp" ]] \
  || fail "the Codex plugin server id is ${CODEX_SERVER_NAME}; expected kmp"
[[ "${CODEX_SERVER_COMMAND}" == "kmp-mcp" ]] \
  || fail "the Codex plugin command is ${CODEX_SERVER_COMMAND}; expected kmp-mcp"
command -v "${CODEX_SERVER_COMMAND}" >/dev/null \
  || fail "the Codex plugin command is not resolvable from PATH"

# --- 4. start it the way the host does ----------------------------------
#
# Read the command out of .mcp.json and expand ${CLAUDE_PLUGIN_ROOT} exactly
# as the host would, instead of assuming a path. A declaration that only
# works from one directory fails here, because the run happens from a
# directory that is neither the repository nor the plugin.

read -r SERVER_NAME SERVER_COMMAND <<<"$(
  python3 - "${INSTALLED}/.mcp.json" "${INSTALLED}" <<'PY'
import json
import sys

config = json.load(open(sys.argv[1], encoding="utf-8"))
plugin_root = sys.argv[2]
servers = config.get("mcpServers", config)
if len(servers) != 1:
    sys.exit(f"expected exactly one MCP server, found {len(servers)}")
name, entry = next(iter(servers.items()))
if "command" not in entry:
    sys.exit(f"server {name} declares no command")
if "cwd" in entry:
    sys.exit(
        f"server {name} declares cwd={entry['cwd']!r}; the host does not resolve "
        "it to the plugin directory, so the command must stand on its own"
    )
command = entry["command"].replace("${CLAUDE_PLUGIN_ROOT}", plugin_root)
if not command.startswith("/"):
    sys.exit(f"server {name} command {command!r} is relative; the host spawns it from its own cwd")
print(name, command)
PY
)"

[[ "${SERVER_NAME}" == "kmp" ]] \
  || fail "the marketplace server id is ${SERVER_NAME}; expected kmp"
[[ -x "${SERVER_COMMAND}" ]] || fail "declared command is not executable: ${SERVER_COMMAND}"

export KMP_MCP_BACKEND=embedded
export KMP_MCP_DATA_DIR="${DATA_DIR}"

RESPONSES="$(
  cd "${WORK_DIR}" && printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"install-smoke","version":"1"}}}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
    | "${SERVER_COMMAND}" 2>/dev/null
)" || fail "the declared command failed to start"

# --- 5. the surface the user came for -----------------------------------

CHECKER="${WORK_DIR}/check_surface.py"
cat >"${CHECKER}" <<'PYCHECK'
import json
import sys

expected_version = sys.argv[1]
expected_tools = {
    "kmp_ingest",
    "kmp_write_memory",
    "kmp_wake",
    "kmp_ask",
    "kmp_goto",
    "kmp_near",
    "kmp_rewind",
    "kmp_forward",
    "kmp_trace",
    "kmp_inspect",
}

responses = {}
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    message = json.loads(line)
    responses[message.get("id")] = message

initialize = responses.get(1)
if initialize is None or "result" not in initialize:
    sys.exit(f"initialize did not answer: {initialize}")

listing = responses.get(2)
if listing is None or "result" not in listing:
    sys.exit(f"tools/list did not answer: {listing}")

advertised = {tool["name"] for tool in listing["result"]["tools"]}
missing = expected_tools - advertised
unexpected = advertised - expected_tools
if missing or unexpected:
    sys.exit(
        "the started server surface differs: "
        f"missing={sorted(missing)}, unexpected={sorted(unexpected)}"
    )

server_version = initialize["result"]["serverInfo"]["version"]
if server_version != expected_version:
    # A PATH fallback legitimately resolves to a previously installed
    # release, so this is worth saying out loud without failing the gate.
    print(
        f"note: the started binary reports {server_version}, workspace is "
        f"{expected_version} (PATH fallback resolved an older install)"
    )

print(f"started {len(advertised)} tools from a marketplace-shaped install")
PYCHECK

printf '%s\n' "${RESPONSES}" | python3 "${CHECKER}" "${WORKSPACE_VERSION}"

# The offer has to survive the marketplace shape too: a plugin that ships the
# notice but not the installer would tell a user to run a command that is not
# there. Both are tracked files, so both must land in the copy a host gets.
for required in scripts/kmp-install-binary.sh scripts/kmp-update.sh scripts/kmp-version-notice.sh hooks/hooks.json; do
  [ -f "${INSTALLED}/$required" ] \
    || fail "$required is missing from the marketplace copy"
done

# With no engine on the machine, the notice says so and names the one command
# that fixes it. This is the state a marketplace install leaves behind.
if ! KMP_MCP_BIN="${WORK_DIR}/no-such-kmp-mcp" CLAUDE_PLUGIN_ROOT="${INSTALLED}" \
     bash "${INSTALLED}/scripts/kmp-version-notice.sh" > "${WORK_DIR}/notice.txt" 2>&1; then
  fail "the version notice exited non-zero; a session-start hook must never break a session"
fi
grep -q "kmp:setup" "${WORK_DIR}/notice.txt" \
  || { cat "${WORK_DIR}/notice.txt" >&2; fail "the notice does not offer /kmp:setup"; }

# And when the engine matches, it says nothing at all. A hook that speaks every
# session is a hook people turn off, and then it is not there on the day it
# would have mattered.
if ! KMP_LATEST_VERSION="${WORKSPACE_VERSION}" CLAUDE_PLUGIN_ROOT="${INSTALLED}" \
     bash "${INSTALLED}/scripts/kmp-version-notice.sh" > "${WORK_DIR}/quiet.txt" 2>&1; then
  fail "the version notice exited non-zero with a matching engine"
fi

# Equality between the plugin and engine is not freshness. Reproduce someone
# exactly two patch releases behind and prove the in-product hook names both
# versions and the single command that catches up both halves.
STALE_VERSION="$(python3 - "$WORKSPACE_VERSION" <<'PY'
import sys

major, minor, patch = map(int, sys.argv[1].split(".")[:3])
if patch < 2:
    raise SystemExit("plugin install smoke needs a patch version >= 2")
print(f"{major}.{minor}.{patch - 2}")
PY
)"
STALE_PLUGIN="${WORK_DIR}/stale-plugin"
cp -R "${INSTALLED}" "$STALE_PLUGIN"
python3 - "$STALE_PLUGIN" "$STALE_VERSION" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
version = sys.argv[2]
for relative in (".claude-plugin/plugin.json", ".codex-plugin/plugin.json"):
    path = root / relative
    body = json.loads(path.read_text())
    body["version"] = version
    path.write_text(json.dumps(body, indent=2) + "\n")
PY
STALE_BIN="${WORK_DIR}/stale-kmp-mcp"
printf '#!/usr/bin/env bash\nprintf "kmp-mcp %s (store format 1)\\n"\n' \
  "$STALE_VERSION" > "$STALE_BIN"
chmod +x "$STALE_BIN"

KMP_MCP_BIN="$STALE_BIN" KMP_LATEST_VERSION="$WORKSPACE_VERSION" \
CLAUDE_PLUGIN_ROOT="$STALE_PLUGIN" \
  bash "$STALE_PLUGIN/scripts/kmp-version-notice.sh" > "$WORK_DIR/stale-notice.txt"
grep -q "${STALE_VERSION} is installed; ${WORKSPACE_VERSION} is out" \
  "$WORK_DIR/stale-notice.txt" \
  || { cat "$WORK_DIR/stale-notice.txt" >&2; fail "two-releases-behind notice lost the version delta"; }
grep -q 'kmp:setup' "$WORK_DIR/stale-notice.txt" \
  || fail "two-releases-behind notice does not offer the single catch-up command"

# The catch-up script is safe to preview and contains both operations: native
# plugin update and the matching checksummed engine install.
CLAUDE_PLUGIN_ROOT="$INSTALLED" \
  bash "$INSTALLED/scripts/kmp-update.sh" --claude --dry-run \
    --version "$WORKSPACE_VERSION" > "$WORK_DIR/update-dry-run.txt"
grep -q 'claude plugin update' "$WORK_DIR/update-dry-run.txt" \
  || { cat "$WORK_DIR/update-dry-run.txt" >&2; fail "update preview omitted the plugin"; }
grep -q 'kmp-install-binary.sh' "$WORK_DIR/update-dry-run.txt" \
  || { cat "$WORK_DIR/update-dry-run.txt" >&2; fail "update preview omitted the engine"; }
CLAUDE_PLUGIN_ROOT="$INSTALLED" \
  bash "$INSTALLED/scripts/kmp-update.sh" --codex --dry-run \
    --version "$WORKSPACE_VERSION" > "$WORK_DIR/codex-plugin-update-dry-run.txt"
grep -q 'codex plugin add kmp@underpass' "$WORK_DIR/codex-plugin-update-dry-run.txt" \
  || { cat "$WORK_DIR/codex-plugin-update-dry-run.txt" >&2; fail "Codex plugin update lost plugin ownership"; }
if grep -q 'standalone Codex prompts' "$WORK_DIR/codex-plugin-update-dry-run.txt"; then
  fail "Codex plugin update attempted to refresh standalone assets"
fi
if [ -s "${WORK_DIR}/quiet.txt" ]; then
  cat "${WORK_DIR}/quiet.txt" >&2
  fail "the notice spoke while the engine and the plugin agree"
fi

# A previous Codex install used the `kernel-memory` registration id. Re-running
# setup must migrate that exact block in place, keeping a recoverable copy and
# never leaving two servers pointing at the same store.
MIGRATION_HOME="${WORK_DIR}/migration-home"
mkdir -p "${MIGRATION_HOME}/.codex"
printf '%s\n' \
  '[mcp_servers.kernel-memory]' \
  'command = "/previous/kmp-mcp"' \
  'env = { KMP_MCP_BACKEND = "embedded" }' \
  '' \
  > "${MIGRATION_HOME}/.codex/config.toml"
for tool in ingest write_memory wake ask goto near rewind forward trace inspect; do
  printf '[mcp_servers.kernel-memory.tools.kernel_%s]\napproval_mode = "approve"\nnote = "kept-%s"\n\n' \
    "$tool" "$tool" >> "${MIGRATION_HOME}/.codex/config.toml"
done
HOME="${MIGRATION_HOME}" \
XDG_DATA_HOME="${MIGRATION_HOME}/.local/share" \
KMP_MCP_BIN="${ROOT_DIR}/target/debug/kmp-mcp" \
KMP_CODEX_PLUGIN_LIST='' \
  bash "${ROOT_DIR}/scripts/mcp/install-kmp-plugin.sh" --codex --standalone \
  > "${WORK_DIR}/migration.txt"
grep -q '^\[mcp_servers\.kmp\]$' "${MIGRATION_HOME}/.codex/config.toml" \
  || fail "the installer did not migrate the Codex server id to kmp"
grep -q '^\[mcp_servers\.kmp\.tools\.kmp_wake\]$' \
  "${MIGRATION_HOME}/.codex/config.toml" \
  || fail "the installer did not migrate the Codex tool policy table"
for tool in ingest write_memory wake ask goto near rewind forward trace inspect; do
  grep -q "^\[mcp_servers\.kmp\.tools\.kmp_${tool}\]$" \
    "${MIGRATION_HOME}/.codex/config.toml" \
    || fail "the installer did not migrate the ${tool} policy"
  grep -q "^note = \"kept-${tool}\"$" "${MIGRATION_HOME}/.codex/config.toml" \
    || fail "the installer rewrote fields in the ${tool} policy"
done
if grep -Eq '^\[mcp_servers\.(kmp|kernel-memory)\.tools\.kernel_' \
    "${MIGRATION_HOME}/.codex/config.toml"; then
  fail "the installer left a former Codex tool-policy name behind"
fi
if grep -q '^\[mcp_servers\.kernel-memory' "${MIGRATION_HOME}/.codex/config.toml"; then
  fail "the installer left a former Codex server table behind"
fi
grep -q '^\[mcp_servers\.kernel-memory\]$' \
  "${MIGRATION_HOME}/.codex/config.toml.kmp-backup" \
  || fail "the installer did not preserve the pre-migration Codex config"
python3 -c 'import pathlib,sys,tomllib;tomllib.loads(pathlib.Path(sys.argv[1]).read_text())' \
  "${MIGRATION_HOME}/.codex/config.toml" \
  || fail "the migrated Codex config is not valid TOML"

# Codex copies the updater beside the binary installer rather than inside a
# plugin root. Its one-command path must resolve that installed layout too.
HOME="${MIGRATION_HOME}" XDG_DATA_HOME="${MIGRATION_HOME}/.local/share" \
  bash "${MIGRATION_HOME}/.local/share/kmp/bin/kmp-update.sh" \
    --codex --standalone --dry-run --version "$WORKSPACE_VERSION" \
    > "${WORK_DIR}/codex-update-dry-run.txt"
grep -q 'refresh the standalone Codex prompts' "${WORK_DIR}/codex-update-dry-run.txt" \
  || { cat "${WORK_DIR}/codex-update-dry-run.txt" >&2; fail "standalone Codex updater changed ownership mode"; }

# Plugin-managed setup owns no global server, prompts, or AGENTS doctrine.
PLUGIN_HOME="${WORK_DIR}/plugin-home"
mkdir -p "$PLUGIN_HOME"
HOME="$PLUGIN_HOME" XDG_DATA_HOME="$PLUGIN_HOME/.local/share" \
KMP_MCP_BIN="${ROOT_DIR}/target/debug/kmp-mcp" \
KMP_CODEX_PLUGIN_LIST='kmp@underpass  installed, enabled  0.1.15  /plugin/kmp' \
  bash "${ROOT_DIR}/scripts/mcp/install-kmp-plugin.sh" --codex \
  > "${WORK_DIR}/plugin-mode.txt"
grep -q 'mode — plugin-managed' "${WORK_DIR}/plugin-mode.txt" \
  || fail "setup did not select plugin-managed mode"
if [ -f "$PLUGIN_HOME/.codex/config.toml" ] \
    || [ -d "$PLUGIN_HOME/.codex/prompts" ] \
    || [ -f "$PLUGIN_HOME/.codex/AGENTS.md" ]; then
  fail "plugin-managed setup created standalone Codex wiring"
fi

# A collision is diagnosed before config mutation. The plugin remains the
# intended owner and setup names the exact command that removes the duplicate.
COLLISION_HOME="${WORK_DIR}/collision-home"
mkdir -p "$COLLISION_HOME/.codex"
printf '%s\n' '[mcp_servers.kmp]' 'command = "/global/kmp-mcp"' \
  'env = { KMP_MCP_DATA_DIR = "/wrong/store" }' \
  > "$COLLISION_HOME/.codex/config.toml"
cp "$COLLISION_HOME/.codex/config.toml" "$WORK_DIR/collision-before.toml"
if HOME="$COLLISION_HOME" XDG_DATA_HOME="$COLLISION_HOME/.local/share" \
   KMP_MCP_BIN="${ROOT_DIR}/target/debug/kmp-mcp" \
   KMP_CODEX_PLUGIN_LIST='kmp@underpass  installed, enabled  0.1.15  /plugin/kmp' \
     bash "${ROOT_DIR}/scripts/mcp/install-kmp-plugin.sh" --codex \
     > "$WORK_DIR/collision.txt" 2>&1; then
  fail "plugin/global collision setup unexpectedly succeeded"
fi
cmp "$WORK_DIR/collision-before.toml" "$COLLISION_HOME/.codex/config.toml" \
  || fail "collision setup changed config before ownership was resolved"
grep -q 'codex mcp remove kmp' "$WORK_DIR/collision.txt" \
  || fail "collision setup did not name the owner repair"

# An old/new policy conflict fails atomically and preserves the source file.
CONFLICT_HOME="${WORK_DIR}/conflict-home"
mkdir -p "$CONFLICT_HOME/.codex"
printf '%s\n' \
  '[mcp_servers.kernel-memory]' \
  'command = "/old/kmp-mcp"' \
  '[mcp_servers.kernel-memory.tools.kernel_wake]' \
  'approval_mode = "approve"' \
  '[mcp_servers.kmp.tools.kmp_wake]' \
  'approval_mode = "deny"' \
  > "$CONFLICT_HOME/.codex/config.toml"
cp "$CONFLICT_HOME/.codex/config.toml" "$WORK_DIR/conflict-before.toml"
if HOME="$CONFLICT_HOME" XDG_DATA_HOME="$CONFLICT_HOME/.local/share" \
   KMP_MCP_BIN="${ROOT_DIR}/target/debug/kmp-mcp" KMP_CODEX_PLUGIN_LIST='' \
     bash "${ROOT_DIR}/scripts/mcp/install-kmp-plugin.sh" --codex --standalone \
     > "$WORK_DIR/conflict.txt" 2>&1; then
  fail "conflicting old/new policy migration unexpectedly succeeded"
fi
cmp "$WORK_DIR/conflict-before.toml" "$CONFLICT_HOME/.codex/config.toml" \
  || fail "conflicting migration replaced the original config"
grep -q 'old and new KMP tables conflict' "$WORK_DIR/conflict.txt" \
  || fail "conflicting migration did not explain the collision"

echo "KMP plugin install smoke passed"
