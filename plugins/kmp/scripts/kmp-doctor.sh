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

FAILURES=0
WARNINGS=0

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  B=$'\033[1m'; R=$'\033[31m'; G=$'\033[32m'; Y=$'\033[33m'; D=$'\033[2m'; Z=$'\033[0m'
else
  B=''; R=''; G=''; Y=''; D=''; Z=''
fi

section() { printf '\n%s%s%s\n' "$B" "$1" "$Z"; }
ok()      { printf '  %sok%s    %s\n' "$G" "$Z" "$1"; }
warn()    { printf '  %swarn%s  %s\n' "$Y" "$Z" "$1"; WARNINGS=$((WARNINGS + 1)); }
fail()    { printf '  %sFAIL%s  %s\n' "$R" "$Z" "$1"; FAILURES=$((FAILURES + 1)); }
info()    { printf '        %s%s%s\n' "$D" "$1" "$Z"; }

printf '%sKMP doctor%s  %sagent memory, end to end%s\n' "$B" "$Z" "$D" "$Z"

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
  info "install it with:"
  info "  cargo install kmp-mcp"
  info "or, in a checkout:  bash scripts/mcp/install-kmp-mcp.sh"
  printf '\n%sNothing else can be checked without the binary.%s\n' "$R" "$Z"
  exit 1
fi

ok "kmp-mcp at $BIN"
VERSION="$("$BIN" --version 2>/dev/null | head -1)"
[ -n "$VERSION" ] && info "$VERSION"

# --------------------------------------------------------------- backend ----
section "Backend"

BACKEND="${KMP_MCP_BACKEND:-}"
ENDPOINT="${KMP_KERNEL_GRPC_ENDPOINT:-}"

if [ -n "$BACKEND" ]; then
  case "$BACKEND" in
    embedded) ok "embedded — kernel runs in-process, storage is local" ;;
    grpc)     ok "grpc — talks to a deployed kernel"
              [ -z "$ENDPOINT" ] && fail "KMP_MCP_BACKEND=grpc but KMP_KERNEL_GRPC_ENDPOINT is unset" ;;
    fixture)  warn "fixture — canned responses, test-only. Memory is not real."
              info "unset KMP_MCP_BACKEND or set it to embedded for real memory" ;;
    *)        fail "unknown KMP_MCP_BACKEND=$BACKEND (expected embedded, grpc or fixture)" ;;
  esac
elif [ -n "$ENDPOINT" ]; then
  ok "grpc (implied by KMP_KERNEL_GRPC_ENDPOINT=$ENDPOINT)"
else
  warn "no backend selected in this shell"
  info "the binary is fail-fast: with no configuration it exits with guidance"
  info "rather than guessing. Hosts usually set KMP_MCP_BACKEND=embedded in"
  info "their MCP registration, so this can be fine — the tool surface check"
  info "below runs with embedded to prove the binary works."
fi

# -------------------------------------------------------------- data dir ----
section "Data directory"

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
    if [ -f "$DATA_DIR/FORMAT_VERSION" ]; then
      info "store format: $(cat "$DATA_DIR/FORMAT_VERSION" 2>/dev/null)"
    fi
    STORE_FILE="$DATA_DIR/store/kernel.redb"
    if [ -f "$STORE_FILE" ]; then
      info "store size: $(du -h "$STORE_FILE" 2>/dev/null | cut -f1)"
      # Single-writer contract (ADR-011). Checked by looking, never by
      # opening: acquiring the lock to test it would be the very conflict
      # this is meant to report.
      HOLDER=""
      if command -v fuser >/dev/null 2>&1; then
        HOLDER="$(fuser "$STORE_FILE" 2>/dev/null | tr -s ' ')"
      elif command -v lsof >/dev/null 2>&1; then
        HOLDER="$(lsof -t "$STORE_FILE" 2>/dev/null | tr '\n' ' ')"
      fi
      if [ -n "$(printf '%s' "$HOLDER" | tr -d ' ')" ]; then
        warn "another process holds this store (pid$HOLDER)"
        info "the embedded store is single-writer (ADR-011): a second host"
        info "session on the same data dir gets no tools at all. Close that"
        info "session, or point this one at a different KMP_MCP_DATA_DIR."
      else
        ok "store is free — no other process holds it"
      fi
    else
      info "no store yet — it is created on first write"
    fi
  else
    info "does not exist yet — it is created on first write"
  fi
fi

# --------------------------------------------------------- tool surface ----
section "Tool surface"

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
RESPONSE="$(printf '%s\n' "$REQUEST" \
  | env KMP_MCP_BACKEND="${BACKEND:-embedded}" KMP_MCP_DATA_DIR="$PROBE_DIR" \
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
  ok "$COUNT tools answered tools/list"
  printf '        %s%s%s\n' "$D" "$TOOLS" "$Z"
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
section "Host registration"

FOUND_HOST=0

if command -v claude >/dev/null 2>&1; then
  FOUND_HOST=1
  if claude mcp list 2>/dev/null | grep -qi 'kernel-memory'; then
    ok "Claude Code — kernel-memory registered"
  else
    warn "Claude Code — kernel-memory not in 'claude mcp list'"
    info "installing the kmp plugin wires it automatically:"
    info "  /plugin marketplace add underpass-ai/kmp"
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
    info "wire it with:  bash scripts/mcp/install-kmp-plugin.sh --codex"
  fi
fi

[ "$FOUND_HOST" -eq 0 ] && info "no Claude Code or Codex CLI found on this machine"

info ""
info "A host session started before a registration change keeps the old MCP"
info "inventory. If the wiring looks right but the tools are missing, restart"
info "the session."

# --------------------------------------------------------------- verdict ----
printf '\n'
if [ "$FAILURES" -gt 0 ]; then
  printf '%s%d check(s) failed%s, %d warning(s).\n' "$R" "$FAILURES" "$Z" "$WARNINGS"
  exit 1
fi
if [ "$WARNINGS" -gt 0 ]; then
  printf '%sUsable%s, with %d warning(s) above.\n' "$Y" "$Z" "$WARNINGS"
  exit 0
fi
printf '%sKMP memory is wired and answering.%s\n' "$G" "$Z"
exit 0
