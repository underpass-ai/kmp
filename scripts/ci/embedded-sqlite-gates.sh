#!/usr/bin/env bash
# ADR-018: the SQLite engine behind the storage seam.
#
# SQLite is the product engine: it survives kill -9 and two processes can
# write the same store without losing an event. The feature name remains as
# a downstream compatibility alias.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

echo "sqlite-gates: clippy with the engine compiled in"
cargo clippy -p kmp-adapter-embedded -p kmp-conformance -p kmp-embedded -p kmp-mcp \
  --all-targets --features sqlite --locked -- -D warnings

echo "sqlite-gates: the sixteen conformance scenarios on the sqlite engine"
cargo test -p kmp-conformance --features sqlite --locked --test embedded_sqlite_conformance

echo "sqlite-gates: adapter suite including fail-closed format-1 detection"
cargo test -p kmp-adapter-embedded --features sqlite --locked

echo "sqlite-gates: obsolete migration commands are absent from the public surface"
if rg -n 'kmp-mcp migrate|migrate <src>|migrate <source' \
    README.md crates/*/README.md plugins/kmp docs; then
  echo "sqlite-gates: obsolete store-migration command leaked into public product material" >&2
  exit 1
fi

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
  if [[ "${tools}" -ne 14 ]]; then
    echo "sqlite-gates: host ${host} got ${tools} tools, expected the fourteen-tool surface" >&2
    sed 's/^/    /' "${WORK_DIR}/err-${host}.log" | head -5 >&2
    exit 1
  fi
done

# The old share-memory shortcut must continue to explain its replacement.
echo "sqlite-gates: share-memory is retired"
RETIRED_DIR="$(mktemp -d)"
trap 'rm -rf "${INSTALL_ROOT}" "${WORK_DIR}" "${RETIRED_DIR}"' EXIT
set +e
"${INSTALL_ROOT}/bin/kmp-mcp" share-memory \
  >"${RETIRED_DIR}/share.out" 2>"${RETIRED_DIR}/share.err"
share_status=$?
set -e
[ "${share_status}" -eq 2 ] && grep -q "share-memory was retired" "${RETIRED_DIR}/share.err" \
  || { echo "sqlite-gates: retired share-memory did not explain the replacement" >&2; exit 1; }

echo "sqlite-gates: passed"
