#!/usr/bin/env bash
# ADR-018: the SQLite engine behind the storage seam.
#
# Everything the default build proves about the embedded store, proved again
# with the opt-in engine compiled in — plus the two things only this engine
# claims: it survives kill -9 like redb does, and two processes can write the
# same store without losing an event. The default build's own gates stay
# untouched; this runs alongside them, never instead of them.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

echo "sqlite-gates: clippy with the engine compiled in"
cargo clippy -p kmp-adapter-embedded -p kmp-conformance -p kmp-embedded -p kmp-mcp \
  --all-targets --features sqlite --locked -- -D warnings

echo "sqlite-gates: the sixteen conformance scenarios on the sqlite engine"
cargo test -p kmp-conformance --features sqlite --locked --test embedded_sqlite_conformance

echo "sqlite-gates: adapter suite with the engine compiled in (redb arm must be unchanged)"
cargo test -p kmp-adapter-embedded --features sqlite --locked

echo "sqlite-gates: kmp-embedded and kmp-mcp still build and pass with the engine in"
cargo test -p kmp-embedded -p kmp-mcp --features sqlite --locked

# `cargo install` is how a user gets this binary, and it resolves features
# without the workspace and without dev-dependencies. A feature that names a
# dev-dependency builds and tests green and then fails the one command that
# matters. This is the check that would have caught it.
echo "sqlite-gates: cargo install with the engine, the way a user gets it"
INSTALL_ROOT="$(mktemp -d)"
trap 'rm -rf "${INSTALL_ROOT}"' EXIT
cargo install --path crates/kmp-mcp --features sqlite --locked --root "${INSTALL_ROOT}" --quiet
"${INSTALL_ROOT}/bin/kmp-mcp" --version

# The engine only matters if a host can reach it. A release-bundle install
# ships bin/kmp-mcp built WITHOUT the engine, and the launcher prefers it over
# anything on PATH — so an operator who installed the engine could not use it.
# This reproduces that install: a plugin copy with a bundled binary that has
# no engine, driven twice at once against one shared store.
echo "sqlite-gates: two hosts share one store through the plugin launcher"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${INSTALL_ROOT}" "${WORK_DIR}"' EXIT
SHARED_DIR="${WORK_DIR}/data"
BUNDLE="${WORK_DIR}/plugin"
mkdir -p "${SHARED_DIR}"
cp -r plugins/kmp "${BUNDLE}"
mkdir -p "${BUNDLE}/bin"
cargo build -p kmp-mcp --locked --quiet          # default features: no engine
cp target/debug/kmp-mcp "${BUNDLE}/bin/kmp-mcp"

LAUNCHER="${BUNDLE}/scripts/run-embedded-mcp.sh"
LIST='{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'

# First start creates the directory on the chosen engine.
printf '%s\n' "${LIST}" | env KMP_MCP_BACKEND=embedded KMP_MCP_ENGINE=sqlite \
  KMP_MCP_DATA_DIR="${SHARED_DIR}" "${INSTALL_ROOT}/bin/kmp-mcp" >/dev/null 2>&1
grep -q 2 "${SHARED_DIR}/FORMAT_VERSION" \
  || { echo "sqlite-gates: the shared directory is not on the sqlite engine" >&2; exit 1; }

count_tools() {
  python3 -c '
import json, sys
total = 0
for line in open(sys.argv[1], encoding="utf-8"):
    line = line.strip()
    if not line:
        continue
    try:
        payload = json.loads(line)
    except ValueError:
        continue
    total += len(payload.get("result", {}).get("tools", []))
print(total)' "$1"
}

start_hosts() {
  for host in a b; do
    ( printf '%s\n' "${LIST}"; sleep 4 ) \
      | env KMP_MCP_DATA_DIR="${SHARED_DIR}" "$@" bash "${LAUNCHER}" \
        > "${WORK_DIR}/out-${host}.json" 2> "${WORK_DIR}/err-${host}.log" &
    sleep 1
  done
  wait
}

# The bundled binary alone: it must refuse the store rather than pretend.
# If this ever starts working, the override below has stopped being needed
# and this gate should be rewritten rather than deleted.
start_hosts env
if [[ "$(count_tools "${WORK_DIR}/out-a.json")" -ne 0 ]]; then
  echo "sqlite-gates: the bundled binary served a sqlite store it cannot open" >&2
  exit 1
fi
echo "  bundled binary alone: refused, as it must"

# And now the binary the operator built, named the only way a launcher
# accepts one.
start_hosts env KMP_MCP_BIN="${INSTALL_ROOT}/bin/kmp-mcp"
for host in a b; do
  tools="$(count_tools "${WORK_DIR}/out-${host}.json")"
  echo "  host ${host} with KMP_MCP_BIN: ${tools} tools"
  if [[ "${tools}" -lt 10 ]]; then
    echo "sqlite-gates: host ${host} got ${tools} tools, expected the ten moves" >&2
    sed 's/^/    /' "${WORK_DIR}/err-${host}.log" | head -5 >&2
    exit 1
  fi
done

echo "sqlite-gates: passed"
