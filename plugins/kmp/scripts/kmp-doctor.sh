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

# Asks a loopback port whether anything is behind it. Configuration says what
# was intended; only the port says what is true. Loopback answers or refuses
# at once, so there is nothing to time out.
port_answers() {
  local hostport="$1" host port
  port="${hostport##*:}"
  host="${hostport%:*}"
  host="${host#[}"; host="${host%]}"
  [ -n "$host" ] && [ -n "$port" ] || return 1
  ( exec 3<>"/dev/tcp/$host/$port" ) >/dev/null 2>&1
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
# A stale plugin with a fresh binary keeps working by luck: the launcher
# falls through to PATH when a marketplace install has no bin/, so the engine
# updates silently while the launcher, this doctor and the skills stay old.
# Nothing announced that, so this does.
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
    info "a stale plugin still starts, because the launcher falls back to the"
    info "binary on PATH — so fixes that live in the launcher, this doctor or"
    info "the skills are the ones you are missing."
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

    # FORMAT_VERSION names the layout, which names the engine (ADR-018):
    # 1 is redb, 2 is sqlite. The engine decides whether a second host can
    # share this store, so say which one it is.
    ENGINE="redb"
    STORE_FILE="$DATA_DIR/store/kernel.redb"
    if [ -f "$DATA_DIR/FORMAT_VERSION" ]; then
      FORMAT="$(cat "$DATA_DIR/FORMAT_VERSION" 2>/dev/null | tr -d '[:space:]')"
      case "$FORMAT" in
        1) ENGINE="redb" ;;
        2) ENGINE="sqlite"; STORE_FILE="$DATA_DIR/store/kernel.sqlite3" ;;
        *) ENGINE="unknown" ;;
      esac
      info "store format: $FORMAT ($ENGINE engine)"
    fi
    if [ -f "$STORE_FILE" ]; then
      info "store size: $(du -h "$STORE_FILE" 2>/dev/null | cut -f1)"
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
          info "share the store between hosts by moving it to the sqlite engine."
          # Say the one thing that is true of *this* machine, rather than a
          # command that will fail on it. A binary without the engine cannot
          # run the migration at all, and finding that out afterwards is the
          # worst moment.
          if printf '%s' "$VERSION" | grep -q "sqlite"; then
            info ""
            info "  kmp-mcp share-memory"
            info ""
            info "one command: snapshots the store (yours is locked, so it"
            info "cannot be migrated in place), migrates, verifies the event"
            info "log matches, keeps the original under a -redb-before-share"
            info "name, and installs the result. Restart both hosts after."
          else
            info ""
            info "this binary has no sqlite engine, so it cannot do that yet:"
            info "  cargo install kmp-mcp"
            info "  kmp-mcp share-memory"
            info ""
            info "through a release-bundle plugin, also point the launcher at"
            info "the binary you built: KMP_MCP_BIN=\$HOME/.cargo/bin/kmp-mcp"
          fi
        fi
      else
        ok "store is free — no other process holds it"
      fi
      STORE_SIZE="$(du -h "$STORE_FILE" 2>/dev/null | cut -f1)"
      STORE_WHEN="$(date -r "$STORE_FILE" '+%Y-%m-%d %H:%M' 2>/dev/null)"
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
ERR_LOG="$(mktemp)"
PROBE_DIR="$(mktemp -d)"
trap 'rm -rf "$ERR_LOG" "$PROBE_DIR"' EXIT

RUNNER=""
command -v timeout >/dev/null 2>&1 && RUNNER="timeout 30"

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
RESPONSE="$(printf '%s\n' "$REQUEST" \
  | env KMP_MCP_BACKEND="${BACKEND:-embedded}" KMP_MCP_DATA_DIR="$PROBE_DIR" \
    KMP_VIEWER_ADDR= \
    $RUNNER "$BIN" 2>"$ERR_LOG")"

TOOLS=""
if [ -n "$RESPONSE" ]; then
  if command -v python3 >/dev/null 2>&1; then
    TOOLS="$(printf '%s' "$RESPONSE" | python3 -c '
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
' 2>/dev/null)"
  else
    TOOLS="$(printf '%s' "$RESPONSE" | grep -o '"name":"kernel_[a-z_]*"' \
      | sed 's/.*"kernel_/kernel_/; s/"$//' | tr '\n' ' ')"
  fi
fi

COUNT="$(printf '%s' "$TOOLS" | wc -w | tr -d ' ')"

if [ "$COUNT" -gt 0 ]; then
  ok "$COUNT tools answered"
  info "$TOOLS"
  [ "$COUNT" -lt 10 ] && warn "expected 10 moves; this build exposes $COUNT"
else
  fail "the binary did not return a usable tool list"
  info "the probe ran against a scratch store, so this is the binary itself"
  info "failing rather than anything to do with your project's memory"
  if [ -s "$ERR_LOG" ]; then
    info "stderr said:"
    sed 's/^/        /' "$ERR_LOG" | head -8
  fi
fi

# ----------------------------------------------------------- host wiring ----
section "Hosts"

FOUND_HOST=0

if command -v claude >/dev/null 2>&1; then
  FOUND_HOST=1
  # `claude mcp list` proves a registration by starting the server, and a
  # server that starts prepares its data dir — so asking Claude Code whether
  # KMP is wired used to leave a `.kernel/` behind in whatever project the
  # user happened to be standing in. Point that start at the throwaway dir
  # the tool probe already uses: the answer is the same, the footprint is
  # none.
  if env KMP_MCP_DATA_DIR="$PROBE_DIR" claude mcp list 2>/dev/null | grep -qi 'kernel-memory'; then
    ok "Claude Code — kernel-memory registered"
  else
    warn "Claude Code — kernel-memory not in 'claude mcp list'"
    offer "/kmp:setup" "Claude Code has no kernel-memory server registered"
    info "installing the kmp plugin wires it automatically:"
    info "  /plugin marketplace add underpass-ai/plugins"
    info "  /plugin install kmp@underpass"
    info "or register the server directly:"
    info "  claude mcp add kernel-memory --scope user \\"
    info "    --env KMP_MCP_BACKEND=embedded -- $BIN"
  fi
fi

CODEX_CONFIG="$HOME/.codex/config.toml"
if command -v codex >/dev/null 2>&1 || [ -f "$CODEX_CONFIG" ]; then
  FOUND_HOST=1
  if [ -f "$CODEX_CONFIG" ] && grep -q 'mcp_servers.kernel-memory' "$CODEX_CONFIG"; then
    ok "Codex CLI — kernel-memory registered"
  else
    warn "Codex CLI — kernel-memory not in $CODEX_CONFIG"
    offer "bash scripts/mcp/install-kmp-plugin.sh --codex" "Codex CLI is not wired"
    info "wire it with:  bash scripts/mcp/install-kmp-plugin.sh --codex"
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
# session, so there is nothing to install. Resolve the address exactly as the
# binary does — unset means the default, an empty value means declined — then
# ask the port instead of trusting the configuration.
VIEWER_ADDR="${KMP_VIEWER_ADDR-127.0.0.1:7317}"
case "$(printf '%s' "$VIEWER_ADDR" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')" in
  ''|off|none)
    brief "declined with KMP_VIEWER_ADDR"
    info "unset that variable and restart the session to see your memory again"
    ;;
  *)
    if port_answers "$VIEWER_ADDR"; then
      ok "your memory, as a graph: http://$VIEWER_ADDR/"
    else
      brief "serves at http://$VIEWER_ADDR/ once a session is running"
      info "nothing is listening there yet. A session on the embedded backend"
      info "mounts it at startup; one already running from an older version"
      info "needs restarting before it does."
    fi
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
