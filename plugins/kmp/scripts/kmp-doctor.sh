#!/usr/bin/env bash
#
# kmp-doctor — diagnose a KMP agent-memory setup end to end.
#
# Answers, in order: is the binary there, which backend would run, which data
# directory wins, is the store free, does the tool surface actually respond,
# and is the MCP registered with the hosts on this machine.
#
# Standalone by design: Claude Code runs it from /kmp:doctor, Codex from
# /kmp-doctor, and a human can run it directly. No arguments required.

set -uo pipefail

VERBOSE=0
for argument in "$@"; do
  case "$argument" in
    -v|--verbose) VERBOSE=1 ;;
    -h|--help) sed -n '2,11p' "$0"; exit 0 ;;
    *) printf 'kmp-doctor: unknown argument %s\n' "$argument" >&2; exit 2 ;;
  esac
done

FAILURES=0
WARNINGS=0
SESSION_USABLE=1
SESSION_REASON=""
NEXT_COMMAND=""
NEXT_REASON=""

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  B=$'\033[1m'; R=$'\033[31m'; G=$'\033[32m'; Y=$'\033[33m'; D=$'\033[2m'; Z=$'\033[0m'
else
  B=''; R=''; G=''; Y=''; D=''; Z=''
fi

# One line per area, detail only where something is wrong — the shape
# `flutter doctor`, `brew doctor` and `gh auth status` share. The full record
# is still there under --verbose; what changes is which of it you have to read
# to learn whether your memory works.
AREA=""
AREA_STATUS="ok"
AREA_HEADLINE=""
AREA_HEADLINE_INDEX=-1
AREA_LINES=()

render_area() {
  [ -z "$AREA" ] && return 0
  local glyph colour
  case "$AREA_STATUS" in
    ok)   glyph="✓"; colour="$G" ;;
    warn) glyph="!"; colour="$Y" ;;
    *)    glyph="✗"; colour="$R" ;;
  esac
  printf '%s[%s]%s %s%-10s%s %s\n' "$colour" "$glyph" "$Z" "$B" "$AREA" "$Z" "$AREA_HEADLINE"
  if [ "$AREA_STATUS" != "ok" ] || [ "$VERBOSE" -eq 1 ]; then
    local index=0
    local line
    for line in "${AREA_LINES[@]:-}"; do
      # The line that became the headline is already on screen.
      if [ "$index" -ne "$AREA_HEADLINE_INDEX" ] || [ "$VERBOSE" -eq 1 ]; then
        [ -n "$line" ] && printf '%s\n' "$line"
      fi
      index=$((index + 1))
    done
  fi
}

section() {
  render_area
  AREA="$1"
  AREA_STATUS="ok"
  AREA_HEADLINE=""
  AREA_HEADLINE_INDEX=-1
  AREA_LINES=()
}

# The compact headline for an area. Without it the first check speaks for the
# area, which is right when the first check is the point and wrong when it is
# a preamble.
brief()   { AREA_HEADLINE="$1"; AREA_HEADLINE_INDEX=-1; }

# The command to put in front of the reader at the end. The first problem to
# name one wins: a doctor that lists five fixes has not diagnosed anything.
offer()   { [ -z "$NEXT_COMMAND" ] && { NEXT_COMMAND="$1"; NEXT_REASON="${2:-}"; }; return 0; }

ok()      { AREA_LINES+=("  ${G}ok${Z}    $1")
            if [ -z "$AREA_HEADLINE" ]; then
              AREA_HEADLINE="$1"; AREA_HEADLINE_INDEX=$((${#AREA_LINES[@]} - 1))
            fi
            return 0; }
warn()    { AREA_LINES+=("  ${Y}warn${Z}  $1")
            if [ "$AREA_STATUS" = "ok" ]; then
              AREA_STATUS="warn"; AREA_HEADLINE="$1"
              AREA_HEADLINE_INDEX=$((${#AREA_LINES[@]} - 1))
            fi
            WARNINGS=$((WARNINGS + 1)); return 0; }
fail()    { AREA_LINES+=("  ${R}FAIL${Z}  $1")
            if [ "$AREA_STATUS" != "fail" ]; then
              AREA_STATUS="fail"; AREA_HEADLINE="$1"
              AREA_HEADLINE_INDEX=$((${#AREA_LINES[@]} - 1))
            fi
            FAILURES=$((FAILURES + 1)); return 0; }
info()    { AREA_LINES+=("        ${D}$1${Z}"); return 0; }

# Claude Code prefixes plugin-provided servers as `plugin:<plugin>:<server>`.
# Treat both whitespace and that colon namespace separator as identifier
# boundaries; matching only a bare `kmp` made a healthy native plugin look
# unregistered.
host_list_has_server() {
  local listing="$1" server_id="$2"
  printf '%s\n' "$listing" \
    | grep -Eqi "(^|[[:space:]:])${server_id}([[:space:]:])"
}

printf '%sKMP doctor%s\n\n' "$B" "$Z"

# ---------------------------------------------------------------- binary ----
section "Binary"

BIN="${KMP_MCP_BIN:-}"
if [ -z "$BIN" ]; then
  if command -v kmp-mcp >/dev/null 2>&1; then
    BIN="$(command -v kmp-mcp)"
  elif [ -x "$HOME/.cargo/bin/kmp-mcp" ]; then
    BIN="$HOME/.cargo/bin/kmp-mcp"
    warn "kmp-mcp exists but is not on PATH"
    info "found at $BIN"
    info 'add it with: export PATH="$HOME/.cargo/bin:$PATH"'
  fi
fi

if [ -z "$BIN" ] || [ ! -x "$BIN" ]; then
  fail "kmp-mcp not found"
  offer "/kmp:setup" "there is no engine on this machine yet"
  info "install it with:"
  info "  cargo install kmp-mcp"
  info "or, in a checkout:  bash scripts/mcp/install-kmp-mcp.sh"
  printf '\n%sNothing else can be checked without the binary.%s\n' "$R" "$Z"
  exit 1
fi

ok "kmp-mcp at $BIN"
VERSION="$("$BIN" --version 2>/dev/null | head -1)"
[ -n "$VERSION" ] && brief "$VERSION"
[ -n "$VERSION" ] && info "$VERSION"

# --------------------------------------------------------------- plugin ----
#
# The binary and the plugin update through different commands — `cargo
# install --force` and `/plugin update` — and neither knows about the other.
# A stale plugin and a fresh binary are two independently updated halves. The
# launcher accepts an automatically discovered engine only when its version
# matches the plugin; the live launcher probe below decides whether this pair
# can actually start rather than inferring that from either half alone.
section "Plugin"

PLUGIN_MANIFEST=""
for candidate in \
  "$(dirname "${BASH_SOURCE[0]}")/../.claude-plugin/plugin.json" \
  "${CLAUDE_PLUGIN_ROOT:-}/.claude-plugin/plugin.json"; do
  [ -f "$candidate" ] && { PLUGIN_MANIFEST="$candidate"; break; }
done

if [ -z "$PLUGIN_MANIFEST" ]; then
  info "not running from an installed plugin — nothing to compare"
else
  PLUGIN_VERSION="$(
    python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version"])' \
      "$PLUGIN_MANIFEST" 2>/dev/null
  )"
  BINARY_VERSION="$(printf '%s' "$VERSION" | sed -E 's/^kmp-mcp ([^ ]+).*/\1/')"
  if [ -z "$PLUGIN_VERSION" ]; then
    warn "cannot read the plugin version from $PLUGIN_MANIFEST"
  elif [ "$PLUGIN_VERSION" = "$BINARY_VERSION" ]; then
    ok "plugin $PLUGIN_VERSION matches the binary"
  else
    warn "plugin files are $PLUGIN_VERSION, binary is $BINARY_VERSION"
    info "these update separately and neither announces the other:"
    info "  binary:  cargo install kmp-mcp --force"
    info "  plugin:  /plugin update kmp@underpass   (then restart the session)"
    info "the launcher accepts a discovered PATH engine only when its version"
    info "matches the plugin; an explicit KMP_MCP_BIN is the only override."
    if [ "$PLUGIN_VERSION" != "$BINARY_VERSION" ]; then
      offer "/kmp:setup" "your engine is $BINARY_VERSION and the plugin is $PLUGIN_VERSION"
    fi
  fi
fi

# --------------------------------------------------------------- backend ----
section "Backend"

BACKEND="${KMP_MCP_BACKEND:-}"
ENDPOINT="${KMP_KERNEL_GRPC_ENDPOINT:-}"

if [ -n "$BACKEND" ]; then
  case "$BACKEND" in
    embedded) ok "embedded — the kernel is right here" ;;
    grpc)     if [ -n "$ENDPOINT" ]; then
                ok "grpc — talking to $ENDPOINT"
              else
                fail "grpc, with no kernel to talk to"
                info "set KMP_KERNEL_GRPC_ENDPOINT, or unset KMP_MCP_BACKEND and"
                info "the kernel runs right here"
              fi ;;
    fixture)  warn "fixture — canned answers that look real"
              info "nothing you write is stored; unset KMP_MCP_BACKEND for the real kernel" ;;
    *)        fail "\`$BACKEND\` is not a backend"
              info "use embedded (the default), grpc or fixture" ;;
  esac
elif [ -n "$ENDPOINT" ]; then
  ok "grpc — talking to $ENDPOINT"
  info "an endpoint in the environment is how the cluster edition is chosen"
else
  # Nothing set stopped being a gap when embedded became the default. The
  # old warning cancelled itself in its own text — "this can be fine" — and a
  # warning that does that teaches people to skip the block where a real
  # backend problem would one day appear.
  ok "embedded — the default, nothing to configure"
fi

# -------------------------------------------------------------- data dir ----
section "Memory"

if [ "${BACKEND:-embedded}" = "grpc" ] || { [ -z "$BACKEND" ] && [ -n "$ENDPOINT" ]; }; then
  info "not applicable in grpc mode — storage lives behind the server"
else
  # ADR-012 resolution order, same rule the binary logs at startup.
  DATA_DIR=""
  ORIGIN=""
  if [ -n "${KMP_MCP_DATA_DIR:-}" ]; then
    DATA_DIR="$KMP_MCP_DATA_DIR"; ORIGIN="KMP_MCP_DATA_DIR"
  else
    probe="$PWD"
    while [ "$probe" != "/" ]; do
      if [ -e "$probe/.git" ]; then
        DATA_DIR="$probe/.kernel"; ORIGIN="project root at $probe"
        break
      fi
      probe="$(dirname "$probe")"
    done
    if [ -z "$DATA_DIR" ]; then
      DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/kmp/default"
      ORIGIN="XDG fallback (not inside a git project)"
    fi
  fi

  ok "$DATA_DIR"
  info "chosen by: $ORIGIN"

  if [ -d "$DATA_DIR" ]; then
    if [ -f "$DATA_DIR/.gitignore" ] && grep -qxF '*' "$DATA_DIR/.gitignore" 2>/dev/null; then
      ok "data-dir self-ignore guard is present"
    else
      fail "$DATA_DIR/.gitignore is missing the '*' safety guard"
      info "run a current KMP startup or migration command to restore the skeleton"
    fi
    if [ -d "$DATA_DIR/logs" ]; then
      ok "startup log directory is present"
    else
      warn "$DATA_DIR/logs is missing — startup failures cannot be audited here"
    fi

    # FORMAT_VERSION names the layout, but it is not evidence that memory is
    # absent. Discover physical engine files independently first: a missing,
    # corrupt or newer stamp must never turn a real store into "empty".
    REDB_STORE="$DATA_DIR/store/kernel.redb"
    SQLITE_STORE="$DATA_DIR/store/kernel.sqlite3"
    STORE_FILE=""
    ENGINE=""
    PHYSICAL_STORES=0
    if [ -f "$REDB_STORE" ]; then
      STORE_FILE="$REDB_STORE"; ENGINE="redb"
      PHYSICAL_STORES=$((PHYSICAL_STORES + 1))
    fi
    if [ -f "$SQLITE_STORE" ]; then
      STORE_FILE="$SQLITE_STORE"; ENGINE="sqlite"
      PHYSICAL_STORES=$((PHYSICAL_STORES + 1))
    fi
    if [ "$PHYSICAL_STORES" -gt 1 ]; then
      fail "multiple engine files exist; refusing to guess which memory is live"
      SESSION_USABLE=0
      SESSION_REASON="the data dir contains both redb and sqlite store files"
      info "$REDB_STORE"
      info "$SQLITE_STORE"
    fi

    EXPECTED_STORE=""
    EXPECTED_ENGINE=""
    if [ -e "$DATA_DIR/FORMAT_VERSION" ]; then
      if FORMAT="$(tr -d '[:space:]' < "$DATA_DIR/FORMAT_VERSION" 2>/dev/null)"; then
        case "$FORMAT" in
          1) EXPECTED_ENGINE="redb"; EXPECTED_STORE="$REDB_STORE" ;;
          2) EXPECTED_ENGINE="sqlite"; EXPECTED_STORE="$SQLITE_STORE" ;;
          0)
            fail "store format 0 is older than this binary supports"
            SESSION_USABLE=0
            SESSION_REASON="store format 0 needs migration"
            info "migrate it with: kmp-mcp migrate <this-dir> <new-dir>"
            ;;
          ''|*[!0-9]*)
            fail "FORMAT_VERSION is corrupt (`${FORMAT:-empty}`); the store cannot open"
            SESSION_USABLE=0
            SESSION_REASON="FORMAT_VERSION is corrupt"
            ;;
          *)
            fail "store format $FORMAT is not supported by this binary (newest: 2)"
            SESSION_USABLE=0
            SESSION_REASON="store format $FORMAT needs a different KMP binary"
            info "upgrade the binary before opening or changing this memory"
            offer "/kmp:setup" "the selected memory uses store format $FORMAT"
            ;;
        esac
        if [ -n "$EXPECTED_ENGINE" ]; then
          info "store format: $FORMAT ($EXPECTED_ENGINE engine)"
        else
          info "store format: ${FORMAT:-unreadable} (unsupported)"
        fi
      else
        fail "FORMAT_VERSION cannot be read; the store cannot open"
        SESSION_USABLE=0
        SESSION_REASON="FORMAT_VERSION cannot be read"
      fi
    elif [ "$PHYSICAL_STORES" -gt 0 ]; then
      fail "a store file exists but FORMAT_VERSION is missing"
      SESSION_USABLE=0
      SESSION_REASON="the memory layout has a store file but no FORMAT_VERSION"
      info "the memory file is present and was left untouched"
    else
      info "store format: not stamped yet"
    fi

    if [ -n "$EXPECTED_STORE" ] && [ "$PHYSICAL_STORES" -gt 0 ] \
        && [ "$STORE_FILE" != "$EXPECTED_STORE" ]; then
      fail "FORMAT_VERSION selects $EXPECTED_ENGINE but the store file is $ENGINE"
      SESSION_USABLE=0
      SESSION_REASON="FORMAT_VERSION and the physical store engine disagree"
      info "the memory file is present and was left untouched"
    fi

    if [ -n "$STORE_FILE" ]; then
      STORE_FILES=("$STORE_FILE")
      STORE_NEWEST_FILE="$STORE_FILE"
      STORE_SIZE="$(du -h "$STORE_FILE" 2>/dev/null | cut -f1)"
      if [ "$ENGINE" = "sqlite" ]; then
        for SIDECAR in "$STORE_FILE-wal" "$STORE_FILE-shm"; do
          if [ -f "$SIDECAR" ]; then
            STORE_FILES+=("$SIDECAR")
          fi
        done
        # The SHM file belongs to the physical store and counts towards its
        # size, but readers update its WAL-index read marks. Only the database
        # and WAL carry committed memory, so a read must not look like a write.
        if [ -f "$STORE_FILE-wal" ] && [ "$STORE_FILE-wal" -nt "$STORE_NEWEST_FILE" ]; then
          STORE_NEWEST_FILE="$STORE_FILE-wal"
        fi
        STORE_SIZE="$(du -ch "${STORE_FILES[@]}" 2>/dev/null | tail -1 | cut -f1)"
      fi
      STORE_WHEN="$(date -r "$STORE_NEWEST_FILE" '+%Y-%m-%d %H:%M' 2>/dev/null)"
      if [ -z "$STORE_WHEN" ] && [ "$STORE_NEWEST_FILE" != "$STORE_FILE" ]; then
        # A checkpoint can remove a sidecar between discovery and stat.
        STORE_WHEN="$(date -r "$STORE_FILE" '+%Y-%m-%d %H:%M' 2>/dev/null)"
      fi
      info "store size: ${STORE_SIZE:-?}"
      HOLDER=""
      if command -v fuser >/dev/null 2>&1; then
        HOLDER="$(fuser "$STORE_FILE" 2>/dev/null | tr -s ' ')"
      elif command -v lsof >/dev/null 2>&1; then
        HOLDER="$(lsof -t "$STORE_FILE" 2>/dev/null | tr '\n' ' ')"
      fi
      if [ -n "$(printf '%s' "$HOLDER" | tr -d ' ')" ]; then
        if [ "$ENGINE" = "sqlite" ]; then
          # WAL-mode sqlite is shared by design: another holder is a
          # second host at work, not a conflict.
          ok "another process has this store open (pid$HOLDER) — sqlite shares it"
        else
          # Single-writer contract (ADR-011). Checked by looking, never by
          # opening: acquiring the lock to test it would be the very
          # conflict this is meant to report.
          warn "another process holds this store (pid$HOLDER)"
          SESSION_USABLE=0
          SESSION_REASON="redb store held by pid$HOLDER"
          info "the redb engine is single-writer (ADR-011): a second host"
          info "session on the same data dir gets no tools at all. Close that"
          info "session, point this one at a different KMP_MCP_DATA_DIR, or"
          info "move the store to SQLite. Close every host first, then run:"
          info ""
          info "  kmp-mcp migrate \"$DATA_DIR\" \"$DATA_DIR-sqlite\""
          info ""
          info "the source is left untouched and the destination carries a"
          info "verified migration receipt. Point both hosts at the new path."
        fi
      else
        ok "store is free — no other process holds it"
      fi
      if [ "$AREA_STATUS" = "ok" ]; then
        brief "${STORE_SIZE:-?} · ${ENGINE} · last written ${STORE_WHEN:-unknown}"
      fi
    else
      info "no store yet — it is created on first write"
      [ "$AREA_STATUS" = "ok" ] && brief "$DATA_DIR — empty, created on first write"
    fi
  else
    info "does not exist yet — it is created on first write"
  fi
fi

# ------------------------------------------------------ startup history ----
#
# A server that dies at startup leaves the session with no tools and the
# host shows nothing but their absence. Everything the binary knew went to
# stderr, which the host consumes — so this checked a healthy setup while
# the session it was diagnosing had no memory at all. It reads the record
# now instead of only testing what happens this second.
section "History"

if [ -z "${DATA_DIR:-}" ] || [ ! -d "$DATA_DIR/logs" ]; then
  brief "no startup recorded here yet"
  info "no startup log yet at ${DATA_DIR:-<unresolved>}/logs"
else
  LAST_STARTS="$(
    cat "$DATA_DIR"/logs/kmp-mcp.log* 2>/dev/null | python3 -c '
import json, sys

starts = []
for line in sys.stdin:
    try:
        entry = json.loads(line)
    except ValueError:
        continue
    fields = entry.get("fields", {})
    message = fields.get("message", "")
    if message not in ("startup succeeded", "startup failed"):
        continue
    when = entry.get("timestamp", "")[:19].replace("T", " ")
    if message == "startup failed":
        starts.append(("FAIL", when, fields.get("reason", "no reason recorded")))
    else:
        backend = fields.get("backend", "?")
        engine = fields.get("engine") or "default"
        starts.append(("ok", when, backend + " backend, " + engine + " engine"))
for verdict, when, detail in starts[-5:]:
    print(verdict + "|" + when + "|" + detail[:88])
'
  )"
  if [ -z "$LAST_STARTS" ]; then
    info "the log has no startup lines yet — this binary predates them"
  else
    # Only the newest start is a verdict; the ones before it are history.
    # A failure that a later start already recovered from is not something to
    # fix, and counting it said "your memory will not work" over a memory that
    # was working — in the same block whose own headline said it had started
    # without failing.
    #
    # A here-string, not a pipe: a piped loop runs in a subshell, so every
    # startup failure it read would be counted and then thrown away with it.
    TOTAL_STARTS="$(printf '%s\n' "$LAST_STARTS" | grep -c '^')"
    SEEN_STARTS=0
    while IFS='|' read -r verdict when detail; do
      [ -z "$verdict" ] && continue
      SEEN_STARTS=$((SEEN_STARTS + 1))
      if [ "$SEEN_STARTS" -lt "$TOTAL_STARTS" ]; then
        if [ "$verdict" = "FAIL" ]; then
          info "$when  failed, and recovered since: $detail"
        else
          info "$when  $detail"
        fi
      elif [ "$verdict" = "FAIL" ]; then
        fail "$when  $detail"
      else
        ok "$when  $detail"
      fi
    done <<< "$LAST_STARTS"
    LAST_LINE="$(printf '%s\n' "$LAST_STARTS" | tail -1)"
    LAST_WHEN="$(printf '%s' "$LAST_LINE" | cut -d'|' -f2)"
    if printf '%s' "$LAST_LINE" | grep -q '^FAIL'; then
      brief "the last start failed, at $LAST_WHEN"
    else
      brief "last started $LAST_WHEN, without failing"
    fi
    if printf '%s' "$LAST_STARTS" | tail -1 | grep -q '^FAIL'; then
      SESSION_USABLE=0
      SESSION_REASON="last start failed"
      info "the most recent start failed — that is why the tools are missing,"
      info "and the reason above is what the host swallowed."
    fi
  fi
fi

# --------------------------------------------------------- tool surface ----
section "Tools"

REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
LAUNCHER_ERR_LOG="$(mktemp)"
BINARY_ERR_LOG="$(mktemp)"
PROBE_DIR="$(mktemp -d)"
trap 'rm -rf "$LAUNCHER_ERR_LOG" "$BINARY_ERR_LOG" "$PROBE_DIR"' EXIT

RUNNER=()
command -v timeout >/dev/null 2>&1 && RUNNER=(timeout 30)

# Probe a throwaway data dir, never the real one. A diagnostic must not
# create a store as a side effect, and must not take the single-writer lock
# out from under a live session. Whether the real store is free is answered
# above, by looking rather than by opening.
#
# The viewer is off for the same reason: the probe would otherwise bind the
# viewer's port, and on the ordinary happy path — a live session already
# serving there — it would collide with the very session it is diagnosing.
#
# Empty rather than `off`: this script ships with the plugin and can meet an
# older binary that has no word for declining, but every version has always
# read an empty value as no viewer.
tool_names_from_response() {
  local response="$1"
  if [ -n "$response" ]; then
    if command -v python3 >/dev/null 2>&1; then
      printf '%s' "$response" | python3 -c '
import json, sys
names = []
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        payload = json.loads(line)
    except ValueError:
        continue
    for tool in payload.get("result", {}).get("tools", []):
        if "name" in tool:
            names.append(tool["name"])
print(" ".join(names))
' 2>/dev/null
    else
      printf '%s' "$response" | grep -o '"name":"kmp_[a-z_]*"' \
        | sed 's/.*"kmp_/kmp_/; s/"$//' | tr '\n' ' '
    fi
  fi
}

run_tool_probe() {
  local executable="$1" error_log="$2"
  PROBE_RESPONSE="$(printf '%s\n' "$REQUEST" \
    | env KMP_MCP_BACKEND="${BACKEND:-embedded}" KMP_MCP_DATA_DIR="$PROBE_DIR" \
      KMP_VIEWER_ADDR= \
      "${RUNNER[@]}" "$executable" 2>"$error_log")"
  PROBE_STATUS=$?
  PROBE_TOOLS="$(tool_names_from_response "$PROBE_RESPONSE")"
  PROBE_COUNT="$(printf '%s' "$PROBE_TOOLS" | wc -w | tr -d ' ')"
}

PLUGIN_LAUNCHER="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/run-embedded-mcp.sh"
LAUNCHER_CHECKED=0
LAUNCHER_WORKS=0
if [ -f "$PLUGIN_LAUNCHER" ]; then
  LAUNCHER_CHECKED=1
  run_tool_probe "$PLUGIN_LAUNCHER" "$LAUNCHER_ERR_LOG"
  if [ "$PROBE_STATUS" -eq 0 ] && [ "$PROBE_COUNT" -gt 0 ]; then
    LAUNCHER_WORKS=1
    ok "$PROBE_COUNT tools answered through the plugin launcher"
    info "$PROBE_TOOLS"
    [ "$PROBE_COUNT" -lt 13 ] \
      && warn "expected the 13 declared moves; the launcher exposes $PROBE_COUNT"
  else
    fail "the plugin launcher cannot start a usable KMP session"
    SESSION_USABLE=0
    if [ "$PROBE_STATUS" -ne 0 ]; then
      SESSION_REASON="the plugin launcher exits $PROBE_STATUS before the host gets KMP tools."
    else
      SESSION_REASON="the plugin launcher returns no KMP tools to the host."
    fi
    offer "/kmp:setup" "repair the plugin and engine version pair"
    if [ -s "$LAUNCHER_ERR_LOG" ]; then
      info "launcher stderr said:"
      while IFS= read -r line; do info "  $line"; done < <(head -8 "$LAUNCHER_ERR_LOG")
    fi
  fi
else
  info "no sibling run-embedded-mcp.sh found; checking the binary directly"
fi

# Keep a direct engine probe even when the launcher fails. Its result tells a
# person whether setup needs to repair the engine or only the plugin/version
# pair instead of collapsing both failures into "no tools".
run_tool_probe "$BIN" "$BINARY_ERR_LOG"
if [ "$PROBE_STATUS" -eq 0 ] && [ "$PROBE_COUNT" -gt 0 ]; then
  if [ "$LAUNCHER_CHECKED" -eq 1 ] && [ "$LAUNCHER_WORKS" -eq 0 ]; then
    ok "$PROBE_COUNT tools answered from the binary alone"
    info "the engine works; the plugin launcher is the blocking layer"
  elif [ "$LAUNCHER_CHECKED" -eq 0 ]; then
    ok "$PROBE_COUNT tools answered from the binary"
  else
    info "direct binary probe also answered $PROBE_COUNT tools"
  fi
  [ "$PROBE_COUNT" -lt 13 ] \
    && warn "expected the 13 declared moves; this build exposes $PROBE_COUNT"
else
  if [ "$LAUNCHER_WORKS" -eq 1 ]; then
    warn "the separately selected binary did not return a usable tool list"
    info "the plugin launcher did answer, so this does not block the host session"
  else
    fail "the binary itself did not return a usable tool list"
    SESSION_USABLE=0
    [ -z "$SESSION_REASON" ] \
      && SESSION_REASON="the KMP engine itself returns no usable tool list."
    info "the direct probe ran against a scratch store, not your project's memory"
  fi
  if [ -s "$BINARY_ERR_LOG" ]; then
    info "stderr said:"
    while IFS= read -r line; do info "  $line"; done < <(head -8 "$BINARY_ERR_LOG")
  fi
fi

# --------------------------------------------------------- agent policy ----
section "Agent"

# This is orchestration policy, not retrieval behavior inside the kernel.
# Reading it must never create the config: an absent file means the bounded
# English fallback default is active.
if POLICY_OUTPUT="$("$BIN" config 2>&1)"; then
  POLICY_LANGUAGES="$(
    printf '%s\n' "$POLICY_OUTPUT" \
      | sed -n 's/^ask fallback languages: //p' \
      | head -1
  )"
  POLICY_PATH="$(
    printf '%s\n' "$POLICY_OUTPUT" \
      | sed -n 's/^config: //p' \
      | head -1
  )"
  if [ -n "$POLICY_LANGUAGES" ]; then
    ok "semantic Ask fallback: $POLICY_LANGUAGES"
  else
    warn "the binary returned an unreadable agent policy"
    info "$POLICY_OUTPUT"
  fi
  [ -n "$POLICY_PATH" ] && info "config: $POLICY_PATH"
  info "temporal intent bypasses Ask and navigates time first"
  info "only the query may be translated; stored evidence stays byte-for-byte"
else
  warn "this binary predates configurable agent routing"
  info "$POLICY_OUTPUT"
  info "update the engine and plugin together; no language fallback is assumed"
fi

# ----------------------------------------------------------- host wiring ----
section "Hosts"

FOUND_HOST=0

if command -v claude >/dev/null 2>&1 || [ "${KMP_DOCTOR_CLAUDE_MCP_LIST+x}" = x ]; then
  FOUND_HOST=1
  # `claude mcp list` proves a registration by starting the server, and a
  # server that starts prepares its data dir — so asking Claude Code whether
  # KMP is wired used to leave a `.kernel/` behind in whatever project the
  # user happened to be standing in. Point that start at the throwaway dir
  # the tool probe already uses: the answer is the same, the footprint is
  # none.
  if [ "${KMP_DOCTOR_CLAUDE_MCP_LIST+x}" = x ]; then
    # Deterministic seam for the plugin smoke: no host config or process is
    # needed to prove every supported registration shape.
    CLAUDE_MCP_LIST="$KMP_DOCTOR_CLAUDE_MCP_LIST"
  else
    CLAUDE_MCP_LIST="$(env KMP_MCP_DATA_DIR="$PROBE_DIR" claude mcp list 2>/dev/null)"
  fi
  # The plugin ships the server as `memory`, so Claude Code composes the id
  # `plugin:kmp:memory` — the plugin segment carries the identity and the
  # server segment says what it is. A hand-registered server is a flat
  # `kmp`. Matching the `kmp` segment recognises both shapes.
  if host_list_has_server "$CLAUDE_MCP_LIST" kmp; then
    ok "Claude Code — kmp registered"
  elif host_list_has_server "$CLAUDE_MCP_LIST" kernel-memory; then
    warn "Claude Code — registered under the former kernel-memory id"
    offer "/kmp:setup" "Claude Code still uses the former server id"
    info "replace it with:"
    info "  claude mcp remove kernel-memory"
    info "  claude mcp add kmp --scope user \\"
    info "    --env KMP_MCP_BACKEND=embedded -- $BIN"
  else
    warn "Claude Code — kmp not in 'claude mcp list'"
    offer "/kmp:setup" "Claude Code has no kmp server registered"
    info "installing the kmp plugin wires it automatically:"
    info "  /plugin marketplace add underpass-ai/plugins"
    info "  /plugin install kmp@underpass"
    info "or register the server directly:"
    info "  claude mcp add kmp --scope user \\"
    info "    --env KMP_MCP_BACKEND=embedded -- $BIN"
  fi
fi

# What a complete Codex install looks like. This list is the same one
# install-kmp-plugin.sh copies, and scripts/ci/kmp-plugin-voice.sh fails the
# build if the two ever drift from the prompts the plugin actually ships —
# the drift that left a Codex user with three of eight.
CODEX_PROMPT_NAMES="kmp-setup kmp-doctor kmp-info kmp-moves kmp-demo kmp-catchup kmp-save kmp-restore kmp-revert kmp-uninstall"
CODEX_EXPECTED_PROMPTS=10

CODEX_CONFIG="$HOME/.codex/config.toml"
if command -v codex >/dev/null 2>&1 \
    || [ -f "$CODEX_CONFIG" ] \
    || [ "${KMP_DOCTOR_CODEX_PLUGIN_LIST+x}" = x ]; then
  FOUND_HOST=1

  if [ "${KMP_DOCTOR_CODEX_PLUGIN_LIST+x}" = x ]; then
    CODEX_PLUGIN_LIST="$KMP_DOCTOR_CODEX_PLUGIN_LIST"
  elif command -v codex >/dev/null 2>&1; then
    CODEX_PLUGIN_LIST="$(codex plugin list 2>/dev/null || true)"
  else
    CODEX_PLUGIN_LIST=""
  fi
  if printf '%s\n' "$CODEX_PLUGIN_LIST" \
      | grep -Eq '^kmp@[^[:space:]]+[[:space:]]+installed, enabled([[:space:]]|$)'; then
    CODEX_PLUGIN_ENABLED=1
  else
    CODEX_PLUGIN_ENABLED=0
  fi

  CODEX_GLOBAL_CURRENT=0
  CODEX_GLOBAL_FORMER=0
  CODEX_STALE_TOOL_POLICIES=0
  if [ -f "$CODEX_CONFIG" ]; then
    grep -q '^\[mcp_servers\.kmp\]' "$CODEX_CONFIG" && CODEX_GLOBAL_CURRENT=1
    grep -q '^\[mcp_servers\.kernel-memory' "$CODEX_CONFIG" && CODEX_GLOBAL_FORMER=1
    grep -Eq '^\[mcp_servers\.(kmp|kernel-memory)\.tools\.kernel_(ingest|write_memory|wake|ask|goto|near|rewind|forward|trace|inspect)\]$' \
      "$CODEX_CONFIG" && CODEX_STALE_TOOL_POLICIES=1
  fi

  if [ "$CODEX_STALE_TOOL_POLICIES" -eq 1 ]; then
    warn "Codex CLI — approval policy still names retired kernel_* tools"
    if [ "$CODEX_PLUGIN_ENABLED" -eq 1 ]; then
      offer "codex mcp remove kmp" "the plugin owns MCP, so stale global tool policies belong to the duplicate owner"
      info "remove the global owner; the enabled plugin keeps the live kmp_* tools"
    else
      offer "bash scripts/mcp/install-kmp-plugin.sh --codex --standalone" "standalone tool policies need the atomic kmp_* migration"
      info "the installer migrates all ten policy table names and preserves their values"
    fi
  elif [ "$CODEX_GLOBAL_FORMER" -eq 1 ]; then
    warn "Codex CLI — config still contains former kernel-memory tables"
    if [ "$CODEX_PLUGIN_ENABLED" -eq 1 ]; then
      offer "codex mcp remove kernel-memory" "the native plugin already owns the KMP server"
    else
      offer "bash scripts/mcp/install-kmp-plugin.sh --codex --standalone" "Codex cannot safely load a partial server-id migration"
    fi
    info "the former registration and every child tool policy must move together"
  fi

  if [ "$CODEX_PLUGIN_ENABLED" -eq 1 ] && { [ "$CODEX_GLOBAL_CURRENT" -eq 1 ] || [ "$CODEX_GLOBAL_FORMER" -eq 1 ]; }; then
    warn "Codex CLI — both plugin and global config claim the KMP MCP server"
    offer "codex mcp remove kmp" "one server must have exactly one owner"
    info "plugin owner: kmp@underpass -> kmp-mcp"
    if [ -f "$CODEX_CONFIG" ]; then
      while IFS= read -r owner_line; do
        [ -n "$owner_line" ] && info "$owner_line"
      done < <(python3 - "$CODEX_CONFIG" <<'PY'
import pathlib
import sys
import tomllib

try:
    body = tomllib.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
except Exception:
    raise SystemExit(0)
servers = body.get("mcp_servers", {})
entry = servers.get("kmp") or servers.get("kernel-memory") or {}
print(f"global owner: {entry.get('command', '<no command>')}")
env = entry.get("env", {})
for name in ("KMP_MCP_BACKEND", "KMP_MCP_DATA_DIR", "KMP_MCP_ENGINE"):
    if name in env:
        print(f"global {name}={env[name]}")
PY
)
    fi
    info "compare the global environment above with the Data directory selected from this working directory"
  elif [ "$CODEX_PLUGIN_ENABLED" -eq 1 ]; then
    ok "Codex CLI — plugin-managed, one MCP owner"
  elif [ "$CODEX_GLOBAL_CURRENT" -eq 1 ]; then
    # Standalone installs deliberately keep prompts and doctrine outside a
    # plugin. Count them only in this mode; native Codex skills are checked by
    # the capability contract and the installed-plugin smoke.
    CODEX_PROMPT_DIR="$HOME/.codex/prompts"
    CODEX_HAVE="$(ls "$CODEX_PROMPT_DIR"/kmp-*.md 2>/dev/null | wc -l | tr -d ' ')"
    if [ "$CODEX_HAVE" -ge "$CODEX_EXPECTED_PROMPTS" ]; then
      ok "Codex CLI — standalone, $CODEX_HAVE commands"
    elif [ "$CODEX_HAVE" -eq 0 ]; then
      warn "Codex CLI — standalone MCP, but no /kmp- commands installed"
      info "complete it with:  bash scripts/mcp/install-kmp-plugin.sh --codex --standalone"
    else
      warn "Codex CLI — standalone, $CODEX_HAVE of $CODEX_EXPECTED_PROMPTS commands"
      CODEX_MISSING=""
      for name in $CODEX_PROMPT_NAMES; do
        [ -f "$CODEX_PROMPT_DIR/$name.md" ] || CODEX_MISSING="$CODEX_MISSING /$name"
      done
      info "missing:$CODEX_MISSING"
      info "re-run:  bash scripts/mcp/install-kmp-plugin.sh --codex --standalone"
    fi
  elif [ "$CODEX_GLOBAL_FORMER" -eq 0 ]; then
    warn "Codex CLI — neither an enabled KMP plugin nor standalone wiring was found"
    offer "codex plugin add kmp@underpass" "Codex CLI is not wired"
    info "advanced alternative: bash scripts/mcp/install-kmp-plugin.sh --codex --standalone"
  fi

  # config.toml is only one input. Enabled Codex plugins can contribute MCP
  # servers too, which is how a pre-rename kmp plugin kept starting a second
  # `kernel-memory` after the user config had been migrated correctly.
  if [ -n "${KMP_DOCTOR_CODEX_MCP_LIST+x}" ]; then
    CODEX_MCP_LIST="$KMP_DOCTOR_CODEX_MCP_LIST"
  else
    CODEX_MCP_LIST="$(codex mcp list 2>/dev/null || true)"
  fi
  if host_list_has_server "$CODEX_MCP_LIST" kernel-memory; then
    warn "Codex CLI — effective MCP list still contains kernel-memory"
    offer "codex plugin add kmp@underpass" "an enabled pre-rename plugin is still injecting the former server"
    info "reinstall the KMP plugin, then restart Codex"
  fi
fi

# With no host on the machine there is no check to pass, and an area whose
# only lines are info has no headline — it printed as a tick with nothing
# beside it. Say what was found instead of showing a blank verdict.
if [ "$FOUND_HOST" -eq 0 ]; then
  brief "no Claude Code or Codex CLI found on this machine"
  info "nothing is wired here yet; install one of them and re-run this doctor"
fi

info ""
info "A host session started before a registration change keeps the old MCP"
info "inventory. If the wiring looks right but the tools are missing, restart"
info "the session."

# ---------------------------------------------------------------- viewer ----
section "Viewer"

# The viewer is compiled into every kmp-mcp and mounts over a live embedded
# session, so there is nothing to install. Its capability belongs to that
# process: this separate doctor must never advertise the bare address because
# it cannot know the capability link and following that address returns 401.
VIEWER_ADDR="${KMP_VIEWER_ADDR-127.0.0.1:7317}"
case "$(printf '%s' "$VIEWER_ADDR" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')" in
  ''|off|none)
    brief "declined with KMP_VIEWER_ADDR"
    info "unset that variable and restart the session to see your memory again"
    ;;
  *)
    ok "ChronoLoom comes with the session — ask the agent to open it"
    info "only that session knows its capability link"
    ;;
esac

# --------------------------------------------------------------- verdict ----
render_area
AREA=""

printf '\n'
if [ "$SESSION_USABLE" -eq 0 ]; then
  printf '%sYour memory is not answering in this session.%s %s\n' "$R" "$Z" "$SESSION_REASON"
  [ -n "$NEXT_COMMAND" ] && printf 'Run: %s%s%s\n' "$B" "$NEXT_COMMAND" "$Z"
  exit 1
fi

ISSUES=$((FAILURES + WARNINGS))
if [ "$FAILURES" -gt 0 ]; then
  printf '%sDoctor found %d issue(s).%s Your memory will not work until they are fixed.\n' \
    "$R" "$ISSUES" "$Z"
elif [ "$WARNINGS" -gt 0 ]; then
  printf '%sDoctor found %d issue(s).%s Your memory works; none of them stop it today.\n' \
    "$Y" "$ISSUES" "$Z"
else
  printf '%sNo issues found. Your memory is wired and answering.%s\n' "$G" "$Z"
fi

if [ -n "$NEXT_COMMAND" ]; then
  printf '\nNext: %s%s%s' "$B" "$NEXT_COMMAND" "$Z"
  [ -n "$NEXT_REASON" ] && printf '   %s(%s)%s' "$D" "$NEXT_REASON" "$Z"
  printf '\n'
fi
[ "$VERBOSE" -eq 0 ] && [ "$ISSUES" -gt 0 ] \
  && printf '%sRun with --verbose to see every check.%s\n' "$D" "$Z"

[ "$FAILURES" -gt 0 ] && exit 1
exit 0
