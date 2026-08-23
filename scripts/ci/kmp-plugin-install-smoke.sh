#!/usr/bin/env bash
set -euo pipefail

# Install-shaped plugin smoke: the host's view, not the packager's.
#
# kmp-plugin-smoke.sh proves the bundle we build is well formed. It cannot
# prove the plugin starts once a host installs it, because it builds bin/
# first and then invokes the launcher by path — so it exercises neither the
# marketplace shape nor the .mcp.json declaration the host actually reads.
#
# Three shipped defects lived in exactly that gap, each leaving an install
# that looked correct and did nothing:
#
#   1. the launcher exec'd bin/kmp-mcp, which only exists in a release
#      package, so a marketplace install died with exit 127;
#   2. the plugin manifests stayed at 0.1.0 through every release, so
#      `claude plugin update` reported "already at the latest version";
#   3. .mcp.json declared `cwd: "."` with a relative command, so the host
#      spawned the launcher from wherever the session started and got
#      ENOENT.
#
# This gate reproduces a marketplace install and starts the server the way
# the host does. It fails on all three.

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
    "kernel_ingest",
    "kernel_write_memory",
    "kernel_wake",
    "kernel_ask",
    "kernel_goto",
    "kernel_near",
    "kernel_rewind",
    "kernel_forward",
    "kernel_trace",
    "kernel_inspect",
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
if missing:
    sys.exit(f"the started server does not advertise {sorted(missing)}")

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
for required in scripts/kmp-install-binary.sh scripts/kmp-version-notice.sh hooks/hooks.json; do
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
if ! CLAUDE_PLUGIN_ROOT="${INSTALLED}" \
     bash "${INSTALLED}/scripts/kmp-version-notice.sh" > "${WORK_DIR}/quiet.txt" 2>&1; then
  fail "the version notice exited non-zero with a matching engine"
fi
if [ -s "${WORK_DIR}/quiet.txt" ]; then
  cat "${WORK_DIR}/quiet.txt" >&2
  fail "the notice spoke while the engine and the plugin agree"
fi

echo "KMP plugin install smoke passed"
