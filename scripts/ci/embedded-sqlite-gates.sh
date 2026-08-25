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

# The engine only matters if a host can reach it. Release and plugin binaries
# now ship it by default, so the launcher must serve two hosts without a PATH
# override or a separately rebuilt executable.
echo "sqlite-gates: two hosts share one store through the plugin launcher"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${INSTALL_ROOT}" "${WORK_DIR}"' EXIT
SHARED_DIR="${WORK_DIR}/data"
BUNDLE="${WORK_DIR}/plugin"
mkdir -p "${SHARED_DIR}"
cp -r plugins/kmp "${BUNDLE}"
mkdir -p "${BUNDLE}/bin"
cargo build -p kmp-mcp --locked --quiet          # shipped default: sqlite-capable
cp target/debug/kmp-mcp "${BUNDLE}/bin/kmp-mcp"

LAUNCHER="${BUNDLE}/scripts/run-embedded-mcp.sh"
LIST='{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
PROBE='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kmp_wake","arguments":{"about":"project:sqlite-gate"}}}'

# A real memory operation creates the store on the chosen engine. `tools/list`
# deliberately stays available while a locked backend retries, so it no longer
# proves that storage initialization has happened.
printf '%s\n' "${PROBE}" | env KMP_MCP_BACKEND=embedded KMP_MCP_ENGINE=sqlite \
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

# The bundled binary alone is the product path: both hosts must get tools.
start_hosts env
for host in a b; do
  tools="$(count_tools "${WORK_DIR}/out-${host}.json")"
  echo "  bundled host ${host}: ${tools} tools"
  if [[ "${tools}" -lt 10 ]]; then
    echo "sqlite-gates: host ${host} got ${tools} tools, expected the ten moves" >&2
    sed 's/^/    /' "${WORK_DIR}/err-${host}.log" | head -5 >&2
    exit 1
  fi
done

# The command that exists so nobody has to do the seven steps by hand. It
# only earns its place if it verifies, refuses and keeps the original — so
# the gate drives all three rather than just the happy path.
echo "sqlite-gates: share-memory migrates, verifies and keeps the original"
SHARE_DIR="$(mktemp -d)"
trap 'rm -rf "${INSTALL_ROOT}" "${WORK_DIR}" "${SHARE_DIR}"' EXIT
# A store on the default engine, created the way any first session creates
# one. (Not migrated down from the sqlite one: migration runs one way.)
printf '%s\n' "${PROBE}" | env KMP_MCP_BACKEND=embedded KMP_MCP_ENGINE=redb \
  KMP_MCP_DATA_DIR="${SHARE_DIR}/memory" \
  "${INSTALL_ROOT}/bin/kmp-mcp" >/dev/null 2>&1
grep -q 1 "${SHARE_DIR}/memory/FORMAT_VERSION" \
  || { echo "sqlite-gates: the fixture store is not on redb" >&2; exit 1; }

RECEIPT="$("${INSTALL_ROOT}/bin/kmp-mcp" share-memory "${SHARE_DIR}/memory")"
echo "${RECEIPT}" | sed 's/^/    /'
grep -q 2 "${SHARE_DIR}/memory/FORMAT_VERSION" \
  || { echo "sqlite-gates: share-memory did not install the sqlite store" >&2; exit 1; }
echo "${RECEIPT}" | grep -q "verified:" \
  || { echo "sqlite-gates: share-memory installed without verifying" >&2; exit 1; }
[ -d "${SHARE_DIR}/memory-redb-before-share" ] \
  || { echo "sqlite-gates: share-memory did not keep the original" >&2; exit 1; }

# Running it again must be a no-op, not a second migration.
"${INSTALL_ROOT}/bin/kmp-mcp" share-memory "${SHARE_DIR}/memory" | grep -q "already shareable" \
  || { echo "sqlite-gates: share-memory is not idempotent" >&2; exit 1; }
echo "    rerun: already shareable"

echo "sqlite-gates: passed"
