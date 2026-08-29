#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PLUGIN_DIR="${ROOT_DIR}/plugins/kmp"
FIXTURE="${ROOT_DIR}/tests/plugin/kmp-smoke.jsonl"

fail() {
  echo "KMP plugin smoke: $*" >&2
  exit 1
}

# One data directory per run, outside any project or the operator's real
# data home: a smoke that inherited either would read memory a previous
# run — or a developer's own agent session — left behind.
SMOKE_DATA_DIR="$(mktemp -d)"
trap 'rm -rf "${SMOKE_DATA_DIR}"' EXIT
if command -v cygpath >/dev/null 2>&1; then
  # Native Windows binary: it cannot open an MSYS path.
  export KMP_MCP_DATA_DIR="$(cygpath -w "${SMOKE_DATA_DIR}")"
else
  export KMP_MCP_DATA_DIR="${SMOKE_DATA_DIR}"
fi

cd "${ROOT_DIR}"
python3 -m json.tool "${PLUGIN_DIR}/.codex-plugin/plugin.json" >/dev/null
python3 -m json.tool "${PLUGIN_DIR}/.claude-plugin/plugin.json" >/dev/null
python3 -m json.tool "${PLUGIN_DIR}/.mcp.json" >/dev/null

# Codex and Claude share the plugin but not the process-root contract. Codex
# must use an executable from PATH; a CLAUDE_PLUGIN_ROOT command is passed to
# exec literally and fails with ENOENT before MCP can initialize.
python3 - <<'PY'
import json
import pathlib

plugin = pathlib.Path("plugins/kmp")
manifest = json.loads((plugin / ".codex-plugin/plugin.json").read_text())
servers = manifest.get("mcpServers")
if servers != {"kmp": {"command": "kmp-mcp"}}:
    raise SystemExit(f"unexpected Codex MCP declaration: {servers!r}")
PY

# Setup needs all guide sources as well as the empty-store bundle. The builder
# later proves that these files match the live MCP surface and import cleanly.
for guide_asset in \
  guide/build-guide.py \
  guide/editorial.json \
  guide/guide.requests.json \
  guide/memory.jsonl \
  capabilities.json \
  scripts/kmp-guide-sync.sh
do
  [ -f "${PLUGIN_DIR}/${guide_asset}" ] \
    || fail "the shipped guide asset ${guide_asset} is missing"
done

# Both host manifests must carry the same version: a bundle that tells
# Codex one version and Claude Code another is a packaging defect.
python3 - <<'EOF'
import json
import pathlib
import sys

plugin_dir = pathlib.Path("plugins/kmp")
codex = json.loads((plugin_dir / ".codex-plugin/plugin.json").read_text())["version"]
claude = json.loads((plugin_dir / ".claude-plugin/plugin.json").read_text())["version"]
if codex != claude:
    sys.exit(f"KMP plugin smoke: manifest versions diverge ({codex} != {claude})")
EOF

bash scripts/plugin/build-local-kmp-plugin.sh
python3 "${PLUGIN_DIR}/guide/build-guide.py" check \
  --binary "${PLUGIN_DIR}/bin/kmp-mcp"

GUIDE_SYNC_STORE="${SMOKE_DATA_DIR}/guide-sync-store"
GUIDE_FIRST_EXPORT="${SMOKE_DATA_DIR}/guide-first.jsonl"
GUIDE_SECOND_EXPORT="${SMOKE_DATA_DIR}/guide-second.jsonl"
for export_path in "$GUIDE_FIRST_EXPORT" "$GUIDE_SECOND_EXPORT"; do
  KMP_MCP_DATA_DIR="$GUIDE_SYNC_STORE" KMP_VIEWER_ADDR=off \
    bash "${PLUGIN_DIR}/scripts/kmp-guide-sync.sh" sync \
      --binary "${PLUGIN_DIR}/bin/kmp-mcp" >/dev/null
  KMP_MCP_DATA_DIR="$GUIDE_SYNC_STORE" KMP_VIEWER_ADDR=off \
    "${PLUGIN_DIR}/bin/kmp-mcp" export "$export_path" >/dev/null
done
python3 - "$GUIDE_FIRST_EXPORT" "$GUIDE_SECOND_EXPORT" <<'PY'
import json
import pathlib
import sys

headers = []
for raw in sys.argv[1:]:
    with pathlib.Path(raw).open(encoding="utf-8") as handle:
        headers.append(json.loads(handle.readline()))
for header in headers:
    if header["event_count"] != 2:
        raise SystemExit(f"guide sync produced {header['event_count']} events, expected 2")
    if header["abouts"] != ["guide:kmp", "guide:kmp-agent"]:
        raise SystemExit(f"guide sync selected unexpected abouts: {header['abouts']}")
if headers[0]["content_digest"] != headers[1]["content_digest"]:
    raise SystemExit("an exact guide re-sync changed the exported memory")
PY

# An ignored binary can leak into a local marketplace snapshot, and an updater
# from the previous release cannot use installer behavior that only exists in
# the new plugin yet. The launcher must never give such a stale cache binary
# priority over a PATH engine that matches its own manifest.
LAUNCHER_PLUGIN="${SMOKE_DATA_DIR}/launcher-plugin"
LAUNCHER_PATH_BIN="${SMOKE_DATA_DIR}/launcher-path-bin"
mkdir -p "$LAUNCHER_PLUGIN/scripts" "$LAUNCHER_PLUGIN/bin" "$LAUNCHER_PATH_BIN"
cp "$PLUGIN_DIR/scripts/run-embedded-mcp.sh" "$LAUNCHER_PLUGIN/scripts/"
cp "$PLUGIN_DIR/scripts/kmp-doctor.sh" "$LAUNCHER_PLUGIN/scripts/"
cp -R "$PLUGIN_DIR/.codex-plugin" "$LAUNCHER_PLUGIN/.codex-plugin"
cp -R "$PLUGIN_DIR/.claude-plugin" "$LAUNCHER_PLUGIN/.claude-plugin"
PLUGIN_ENGINE_VERSION="$(python3 - "$LAUNCHER_PLUGIN" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
version = json.loads((root / ".codex-plugin/plugin.json").read_text())["version"]
engine = version.split("+", 1)[0]
for relative in (".codex-plugin/plugin.json", ".claude-plugin/plugin.json"):
    path = root / relative
    body = json.loads(path.read_text())
    body["version"] = f"{engine}+launcher-smoke"
    path.write_text(json.dumps(body, indent=2) + "\n")
print(engine)
PY
)"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'if [ "${1:-}" = "--version" ]; then printf "kmp-mcp 0.0.1 (store format 1)\\n"; else printf "stale-cache-ran\\n"; fi' \
  > "$LAUNCHER_PLUGIN/bin/kmp-mcp"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'if [ "${1:-}" = "--version" ]; then printf "kmp-mcp ${KMP_FAKE_VERSION:?} (store format 1)\\n"; else printf "matching-path-ran\\n"; fi' \
  > "$LAUNCHER_PATH_BIN/kmp-mcp"
chmod +x "$LAUNCHER_PLUGIN/bin/kmp-mcp" "$LAUNCHER_PATH_BIN/kmp-mcp"
KMP_FAKE_VERSION="$PLUGIN_ENGINE_VERSION" PATH="$LAUNCHER_PATH_BIN:$PATH" \
  bash "$LAUNCHER_PLUGIN/scripts/run-embedded-mcp.sh" \
    > "$SMOKE_DATA_DIR/launcher-selection.txt" \
    2> "$SMOKE_DATA_DIR/launcher-selection.err"
grep -qx 'matching-path-ran' "$SMOKE_DATA_DIR/launcher-selection.txt" \
  || fail "plugin launcher selected a stale cache engine over matching PATH"
grep -q 'cache engine 0.0.1 does not match plugin' \
  "$SMOKE_DATA_DIR/launcher-selection.err" \
  || fail "plugin launcher did not diagnose the stale cache engine"

if KMP_FAKE_VERSION=0.0.2 PATH="$LAUNCHER_PATH_BIN:$PATH" \
  bash "$LAUNCHER_PLUGIN/scripts/run-embedded-mcp.sh" \
    > "$SMOKE_DATA_DIR/launcher-mismatch.txt" \
    2> "$SMOKE_DATA_DIR/launcher-mismatch.err"; then
  fail "plugin launcher started without any engine matching its manifest"
fi
grep -q 'run kmp setup' "$SMOKE_DATA_DIR/launcher-mismatch.err" \
  || fail "plugin launcher version failure did not name the repair"

KMP_MCP_BIN="$LAUNCHER_PLUGIN/bin/kmp-mcp" \
  bash "$LAUNCHER_PLUGIN/scripts/run-embedded-mcp.sh" \
    > "$SMOKE_DATA_DIR/launcher-explicit.txt"
grep -qx 'stale-cache-ran' "$SMOKE_DATA_DIR/launcher-explicit.txt" \
  || fail "plugin launcher stopped honoring the explicit KMP_MCP_BIN override"

# Doctor must probe the launcher the host actually starts. Reproduce a healthy
# PATH engine whose version the plugin rejects: the direct probe should prove
# the engine answers while the verdict remains blocking because the host gets
# exit 127 and no tools.
DOCTOR_MISMATCH_BIN="${SMOKE_DATA_DIR}/doctor-mismatch-bin"
mkdir -p "$DOCTOR_MISMATCH_BIN"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'if [ "${1:-}" = "--version" ]; then' \
  '  printf "kmp-mcp 0.0.2 (store formats 1, 2)\\n"' \
  'elif [ "${1:-}" = "config" ]; then' \
  '  printf "ask fallback languages: en\\n"' \
  'else' \
  '  while IFS= read -r _line; do' \
  '    printf "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[{\"name\":\"kmp_wake\"}]}}\n"' \
  '  done' \
  'fi' \
  > "$DOCTOR_MISMATCH_BIN/kmp-mcp"
chmod +x "$DOCTOR_MISMATCH_BIN/kmp-mcp"
set +e
doctor_mismatch_output="$(
  HOME="${SMOKE_DATA_DIR}/doctor-mismatch-home" \
  PATH="$DOCTOR_MISMATCH_BIN:$PATH" \
  NO_COLOR=1 \
  KMP_MCP_DATA_DIR="${SMOKE_DATA_DIR}/doctor-mismatch-store" \
  KMP_VIEWER_ADDR=off \
  KMP_DOCTOR_CLAUDE_MCP_LIST='' \
  KMP_DOCTOR_CODEX_PLUGIN_LIST='' \
  KMP_DOCTOR_CODEX_MCP_LIST='' \
    bash "$LAUNCHER_PLUGIN/scripts/kmp-doctor.sh" 2>&1
)"
doctor_mismatch_status=$?
set -e
[ "$doctor_mismatch_status" -eq 1 ] \
  || fail "doctor called a launcher-rejected version pair usable"
for expected in \
  '[✗] Tools      the plugin launcher cannot start a usable KMP session' \
  '1 tools answered from the binary alone' \
  'the engine works; the plugin launcher is the blocking layer' \
  'Your memory is not answering in this session. the plugin launcher exits 127' \
  'Run: /kmp:setup'; do
  grep -Fq "$expected" <<<"$doctor_mismatch_output" \
    || { printf '%s\n' "$doctor_mismatch_output" >&2; fail "doctor mismatch output omitted $expected"; }
done
grep -Fq 'Your memory works' <<<"$doctor_mismatch_output" \
  && fail "doctor contradicted a launcher that exits before serving tools"

# Windows executes the sibling cmd launcher. Its runtime smoke happens in the
# Windows package job; pin the version-selection contract here as well so the
# two launchers cannot silently drift during a POSIX-only edit.
for required in EXPECTED_ENGINE_VERSION BUNDLED_VERSION PATH_BINARY noMatchingBinary; do
  grep -q "$required" "$PLUGIN_DIR/scripts/run-embedded-mcp.cmd" \
    || fail "Windows plugin launcher lost version-selection field $required"
done

responses="$("${PLUGIN_DIR}/scripts/run-embedded-mcp.sh" <"${FIXTURE}")"

response_contains() {
  local needle="$1"
  if command -v rg >/dev/null 2>&1; then
    rg -Fq -- "${needle}"
  else
    grep -Fq -- "${needle}"
  fi
}

if [[ "$(printf '%s\n' "${responses}" | wc -l)" -ne 2 ]]; then
  echo "KMP plugin smoke expected two MCP responses" >&2
  exit 1
fi

for tool in kmp_wake kmp_ask kmp_write_memory kmp_trace; do
  if ! response_contains "\"name\":\"${tool}\"" <<<"${responses}"; then
    echo "KMP plugin smoke did not advertise ${tool}" >&2
    exit 1
  fi
done

# Claude Code names a plugin MCP server `plugin:<plugin>:<server>`. Doctor
# must accept that native registration, not prescribe a redundant direct MCP
# entry after setup has already succeeded.
DOCTOR_BIN="${PLUGIN_DIR}/bin/kmp-mcp"
if [[ ! -x "${DOCTOR_BIN}" ]]; then
  DOCTOR_BIN="${PLUGIN_DIR}/bin/kmp-mcp.exe"
fi

create_doctor_sqlite_store() {
  local data_dir="$1"
  mkdir -p "$data_dir/store" "$data_dir/logs"
  printf '%s\n' '*' > "$data_dir/.gitignore"
  printf '%s\n' '2' > "$data_dir/FORMAT_VERSION"
}

write_doctor_store_file() {
  local path="$1" blocks="$2"
  dd if=/dev/zero of="$path" bs=4096 count="$blocks" 2>/dev/null
}

run_doctor_for_store() {
  local data_dir="$1"
  HOME="${SMOKE_DATA_DIR}/doctor-home" \
  NO_COLOR=1 \
  KMP_MCP_BIN="${DOCTOR_BIN}" \
  KMP_MCP_BACKEND=embedded \
  KMP_MCP_DATA_DIR="$data_dir" \
  KMP_VIEWER_ADDR=off \
  KMP_DOCTOR_CLAUDE_MCP_LIST='plugin:kmp:kmp: bundled launcher - connected' \
  KMP_DOCTOR_CODEX_PLUGIN_LIST='kmp@underpass  installed, enabled  0.1.15  /plugin/kmp' \
  KMP_DOCTOR_CODEX_MCP_LIST='kmp  kmp-mcp  enabled' \
    bash "${PLUGIN_DIR}/scripts/kmp-doctor.sh"
}

doctor_memory_line() {
  run_doctor_for_store "$1" | grep -F '[✓] Memory     '
}

assert_doctor_memory() {
  local data_dir="$1" expected_size="$2" expected_time="$3"
  local expected="[✓] Memory     ${expected_size} · sqlite · last written ${expected_time}"
  local actual
  actual="$(doctor_memory_line "$data_dir")"
  if [ "$actual" != "$expected" ]; then
    fail "doctor store stats mismatch: expected '$expected', got '$actual'"
  fi
}

# SQLite commits can live in the WAL until a checkpoint updates the main
# database. Doctor must report the complete physical store, but its write time
# comes only from the database and WAL. Readers update SHM read marks, so a
# newer SHM must not make read activity look like a memory write. Cover active
# WAL, retained sidecars after a checkpoint, and a store with no sidecars.
WAL_STORE="${SMOKE_DATA_DIR}/doctor-store-wal"
create_doctor_sqlite_store "$WAL_STORE"
write_doctor_store_file "$WAL_STORE/store/kernel.sqlite3" 1
write_doctor_store_file "$WAL_STORE/store/kernel.sqlite3-wal" 2
write_doctor_store_file "$WAL_STORE/store/kernel.sqlite3-shm" 3
touch -t 202608260101 "$WAL_STORE/store/kernel.sqlite3"
touch -t 202608260303 "$WAL_STORE/store/kernel.sqlite3-wal"
touch -t 202608260304 "$WAL_STORE/store/kernel.sqlite3-shm"
WAL_SIZE="$(du -ch "$WAL_STORE"/store/kernel.sqlite3* | tail -1 | cut -f1)"
assert_doctor_memory "$WAL_STORE" "$WAL_SIZE" '2026-08-26 03:03'

CHECKPOINT_STORE="${SMOKE_DATA_DIR}/doctor-store-checkpoint"
create_doctor_sqlite_store "$CHECKPOINT_STORE"
write_doctor_store_file "$CHECKPOINT_STORE/store/kernel.sqlite3" 3
write_doctor_store_file "$CHECKPOINT_STORE/store/kernel.sqlite3-wal" 2
write_doctor_store_file "$CHECKPOINT_STORE/store/kernel.sqlite3-shm" 1
touch -t 202608260401 "$CHECKPOINT_STORE/store/kernel.sqlite3-wal"
touch -t 202608260403 "$CHECKPOINT_STORE/store/kernel.sqlite3"
touch -t 202608260404 "$CHECKPOINT_STORE/store/kernel.sqlite3-shm"
CHECKPOINT_SIZE="$(du -ch "$CHECKPOINT_STORE"/store/kernel.sqlite3* | tail -1 | cut -f1)"
assert_doctor_memory "$CHECKPOINT_STORE" "$CHECKPOINT_SIZE" '2026-08-26 04:03'

NO_WAL_STORE="${SMOKE_DATA_DIR}/doctor-store-no-wal"
create_doctor_sqlite_store "$NO_WAL_STORE"
write_doctor_store_file "$NO_WAL_STORE/store/kernel.sqlite3" 2
touch -t 202608260503 "$NO_WAL_STORE/store/kernel.sqlite3"
NO_WAL_SIZE="$(du -ch "$NO_WAL_STORE/store/kernel.sqlite3" | tail -1 | cut -f1)"
assert_doctor_memory "$NO_WAL_STORE" "$NO_WAL_SIZE" '2026-08-26 05:03'

# A layout stamp is a gate, not an inventory. The physical SQLite file must
# remain visible — and the Memory area must fail — when that gate is newer,
# corrupt or absent. This is the exact shape that used to say "empty" over a
# non-empty store and invite the user to discard recoverable memory.
assert_doctor_rejects_layout() {
  local label="$1" stamp="$2" expected="$3"
  local data_dir="${SMOKE_DATA_DIR}/doctor-store-${label}"
  create_doctor_sqlite_store "$data_dir"
  write_doctor_store_file "$data_dir/store/kernel.sqlite3" 3
  if [ "$stamp" = "<missing>" ]; then
    rm "$data_dir/FORMAT_VERSION"
  else
    printf '%s\n' "$stamp" > "$data_dir/FORMAT_VERSION"
  fi

  set +e
  local output
  output="$(run_doctor_for_store "$data_dir" 2>&1)"
  local status=$?
  set -e
  [ "$status" -eq 1 ] \
    || { printf '%s\n' "$output" >&2; fail "doctor accepted the $label layout"; }
  grep -Fq '[✗] Memory' <<<"$output" \
    || { printf '%s\n' "$output" >&2; fail "doctor did not fail Memory for $label"; }
  grep -Fq "$expected" <<<"$output" \
    || { printf '%s\n' "$output" >&2; fail "doctor omitted $label diagnosis"; }
  grep -Fq 'store size:' <<<"$output" \
    || { printf '%s\n' "$output" >&2; fail "doctor hid physical memory for $label"; }
  if grep -Fq 'empty, created on first write' <<<"$output"; then
    printf '%s\n' "$output" >&2
    fail "doctor called the $label store empty"
  fi
  [ -f "$data_dir/store/kernel.sqlite3" ] \
    || fail "doctor changed the $label memory while diagnosing it"
}

assert_doctor_rejects_layout newer 3 'store format 3 is not supported'
assert_doctor_rejects_layout corrupt banana 'FORMAT_VERSION is corrupt'
assert_doctor_rejects_layout missing '<missing>' 'a store file exists but FORMAT_VERSION is missing'

# Format 1 is inventory, never a health probe. Doctor must name the external
# export bridge and leave even malformed bytes exactly as they were.
LEGACY_STORE="${SMOKE_DATA_DIR}/doctor-store-format-1"
mkdir -p "$LEGACY_STORE/store" "$LEGACY_STORE/logs"
printf '*\n' > "$LEGACY_STORE/.gitignore"
printf '1\n' > "$LEGACY_STORE/FORMAT_VERSION"
printf 'truncated legacy bytes\n' > "$LEGACY_STORE/store/retired-layout.bin"
cp "$LEGACY_STORE/store/retired-layout.bin" "$LEGACY_STORE/source.before"
set +e
legacy_output="$(run_doctor_for_store "$LEGACY_STORE" 2>&1)"
legacy_status=$?
set -e
[ "$legacy_status" -eq 1 ] || fail "doctor accepted retired format 1"
grep -Fq 'store format 1 is unsupported' <<<"$legacy_output" \
  || fail "doctor did not explain the unsupported format"
grep -Fq 'archived compatible exporter' <<<"$legacy_output" \
  || fail "doctor omitted the format-1 export bridge"
grep -Fq 'inventory only; Doctor did not open or probe' <<<"$legacy_output" \
  || fail "doctor treated legacy bytes as a live engine"
if grep -Fq 'store is free' <<<"$legacy_output"; then
  fail "doctor presented retired format 1 as usable"
fi
cmp "$LEGACY_STORE/source.before" "$LEGACY_STORE/store/retired-layout.bin" \
  || fail "doctor changed the retired source"

doctor_output="$(
  HOME="${SMOKE_DATA_DIR}/doctor-home" \
  NO_COLOR=1 \
  KMP_MCP_BIN="${DOCTOR_BIN}" \
  KMP_MCP_BACKEND=embedded \
  KMP_VIEWER_ADDR=off \
  KMP_DOCTOR_CLAUDE_MCP_LIST='plugin:kmp:memory: bundled launcher - connected' \
  KMP_DOCTOR_CODEX_PLUGIN_LIST='kmp@underpass  installed, enabled  0.1.15  /plugin/kmp' \
  KMP_DOCTOR_CODEX_MCP_LIST='kmp  kmp-mcp  enabled' \
    bash "${PLUGIN_DIR}/scripts/kmp-doctor.sh"
)"
if ! grep -Fq '[✓] Hosts      Claude Code — kmp registered' <<<"${doctor_output}"; then
  echo "KMP plugin smoke: doctor rejected Claude Code's native plugin registration" >&2
  printf '%s\n' "${doctor_output}" >&2
  exit 1
fi
if ! grep -Fq 'Agent      semantic Ask fallback: en (default)' <<<"${doctor_output}"; then
  echo "KMP plugin smoke: doctor did not report the active agent language policy" >&2
  printf '%s\n' "${doctor_output}" >&2
  exit 1
fi

# The capability is generated by the live MCP process, not by this diagnostic.
# A configured address is therefore not a URL the doctor can authorise:
# advertising it sends the user to a truthful but avoidable 401.
viewer_output="$(
  HOME="${SMOKE_DATA_DIR}/doctor-home" \
  NO_COLOR=1 \
  KMP_MCP_BIN="${DOCTOR_BIN}" \
  KMP_MCP_BACKEND=embedded \
  KMP_MCP_DATA_DIR="${SMOKE_DATA_DIR}/doctor-viewer-store" \
  KMP_VIEWER_ADDR=127.0.0.1:1 \
  KMP_DOCTOR_CLAUDE_MCP_LIST='plugin:kmp:memory: bundled launcher - connected' \
  KMP_DOCTOR_CODEX_PLUGIN_LIST='kmp@underpass  installed, enabled  0.1.15  /plugin/kmp' \
  KMP_DOCTOR_CODEX_MCP_LIST='kmp  kmp-mcp  enabled' \
    bash "${PLUGIN_DIR}/scripts/kmp-doctor.sh"
)"
for expected in \
  '[✓] Viewer     ChronoLoom comes with the session — ask the agent to open it'; do
  grep -Fq "$expected" <<<"${viewer_output}" \
    || { printf '%s\n' "${viewer_output}" >&2; fail "doctor omitted viewer guidance: $expected"; }
done
if grep -Fq 'http://127.0.0.1:1/' <<<"${viewer_output}"; then
  printf '%s\n' "${viewer_output}" >&2
  fail "doctor advertised a bare viewer URL that cannot carry the capability"
fi

# A clean user config can still be poisoned by an enabled pre-rename Codex
# plugin. Doctor must inspect the effective MCP inventory and name that stale
# server instead of declaring the host healthy from config.toml alone.
CODEX_DOCTOR_HOME="${SMOKE_DATA_DIR}/codex-doctor-home"
mkdir -p "${CODEX_DOCTOR_HOME}/.codex/prompts"
printf '%s\n' \
  '[mcp_servers.kmp]' \
  'command = "kmp-mcp"' \
  > "${CODEX_DOCTOR_HOME}/.codex/config.toml"
for prompt in kmp-setup kmp-doctor kmp-info kmp-moves kmp-guide kmp-catchup kmp-save kmp-restore kmp-revert kmp-uninstall; do
  : > "${CODEX_DOCTOR_HOME}/.codex/prompts/${prompt}.md"
done
stale_codex_output="$(
  HOME="${CODEX_DOCTOR_HOME}" \
  NO_COLOR=1 \
  KMP_MCP_BIN="${DOCTOR_BIN}" \
  KMP_MCP_BACKEND=embedded \
  KMP_VIEWER_ADDR=off \
  KMP_DOCTOR_CLAUDE_MCP_LIST='plugin:kmp:memory: bundled launcher - connected' \
  KMP_DOCTOR_CODEX_PLUGIN_LIST='' \
  KMP_DOCTOR_CODEX_MCP_LIST='kernel-memory  ${CLAUDE_PLUGIN_ROOT}/scripts/run-embedded-mcp.sh  enabled' \
    bash "${PLUGIN_DIR}/scripts/kmp-doctor.sh"
)"
if ! grep -Fq 'Codex CLI — effective MCP list still contains kernel-memory' <<<"${stale_codex_output}"; then
  echo "KMP plugin smoke: doctor missed a stale plugin-provided Codex server" >&2
  printf '%s\n' "${stale_codex_output}" >&2
  exit 1
fi

# Reproduce the half-migrated file that made Codex reject config.toml before
# it could start: the transport moved to kmp but one legacy tool table did not.
printf '%s\n' \
  '[mcp_servers.kmp]' \
  'command = "kmp-mcp"' \
  '' \
  '[mcp_servers.kernel-memory.tools.kernel_wake]' \
  'approval_mode = "approve"' \
  > "${CODEX_DOCTOR_HOME}/.codex/config.toml"
legacy_table_output="$(
  HOME="${CODEX_DOCTOR_HOME}" \
  NO_COLOR=1 \
  KMP_MCP_BIN="${DOCTOR_BIN}" \
  KMP_MCP_BACKEND=embedded \
  KMP_VIEWER_ADDR=off \
  KMP_DOCTOR_CLAUDE_MCP_LIST='plugin:kmp:memory: bundled launcher - connected' \
  KMP_DOCTOR_CODEX_PLUGIN_LIST='' \
  KMP_DOCTOR_CODEX_MCP_LIST='kmp  kmp-mcp  enabled' \
    bash "${PLUGIN_DIR}/scripts/kmp-doctor.sh"
)"
if ! grep -Fq 'Codex CLI — approval policy still names retired kernel_* tools' <<<"${legacy_table_output}"; then
  echo "KMP plugin smoke: doctor missed a legacy Codex tool policy" >&2
  printf '%s\n' "${legacy_table_output}" >&2
  exit 1
fi

# An enabled native plugin plus a global registration is two owners, even
# when both currently start. Doctor must name the collision and the global
# environment that can redirect the store.
printf '%s\n' \
  '[mcp_servers.kmp]' \
  'command = "/global/kmp-mcp"' \
  'env = { KMP_MCP_BACKEND = "embedded", KMP_MCP_DATA_DIR = "/global/store", KMP_MCP_ENGINE = "sqlite" }' \
  > "${CODEX_DOCTOR_HOME}/.codex/config.toml"
collision_output="$(
  HOME="${CODEX_DOCTOR_HOME}" \
  NO_COLOR=1 \
  KMP_MCP_BIN="${DOCTOR_BIN}" \
  KMP_MCP_BACKEND=embedded \
  KMP_VIEWER_ADDR=off \
  KMP_DOCTOR_CLAUDE_MCP_LIST='plugin:kmp:memory: bundled launcher - connected' \
  KMP_DOCTOR_CODEX_PLUGIN_LIST='kmp@underpass  installed, enabled  0.1.15  /plugin/kmp' \
  KMP_DOCTOR_CODEX_MCP_LIST='kmp  /global/kmp-mcp  enabled' \
    bash "${PLUGIN_DIR}/scripts/kmp-doctor.sh"
)"
for expected in \
  'both plugin and global config claim the KMP MCP server' \
  'plugin owner: kmp@underpass -> kmp-mcp' \
  'global owner: /global/kmp-mcp' \
  'global KMP_MCP_DATA_DIR=/global/store' \
  'codex mcp remove kmp'; do
  grep -Fq "$expected" <<<"${collision_output}" \
    || { printf '%s\n' "${collision_output}" >&2; echo "KMP plugin smoke: collision output omitted $expected" >&2; exit 1; }
done

echo "KMP plugin smoke passed"
