#!/usr/bin/env bash
#
# kmp-demo — load a populated example memory so KMP can be seen before it is
# used.
#
# Imports the shipped incident bundle into a data directory of its own. It
# never touches the memory of the project you are in: a demo that could write
# into real memory would be a demo nobody should run.
#
# Standalone by design: Claude Code runs it from /kmp:demo, and a human can
# run it directly. No arguments required.

set -uo pipefail

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  B=$'\033[1m'; R=$'\033[31m'; G=$'\033[32m'; Y=$'\033[33m'; D=$'\033[2m'; Z=$'\033[0m'
else
  B=''; R=''; G=''; Y=''; D=''; Z=''
fi

say()  { printf '%s\n' "$*"; }
ok()   { printf '  %sok%s    %s\n' "$G" "$Z" "$*"; }
warn() { printf '  %swarn%s  %s\n' "$Y" "$Z" "$*"; }
fail() { printf '  %sfail%s  %s\n' "$R" "$Z" "$*"; }
info() { printf '        %s%s%s\n' "$D" "$*" "$Z"; }

PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
BUNDLE="${PLUGIN_ROOT}/demo/checkout-latency.jsonl"
DEMO_DIR="${KMP_DEMO_DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/kmp/demo}"

say ""
say "${B}KMP demo${Z}  an example memory, in its own data directory"
say ""

# ---------------------------------------------------------------- binary ----
BIN=""
if [ -x "${PLUGIN_ROOT}/bin/kmp-mcp" ]; then
  BIN="${PLUGIN_ROOT}/bin/kmp-mcp"
elif command -v kmp-mcp >/dev/null 2>&1; then
  BIN="$(command -v kmp-mcp)"
else
  fail "no kmp-mcp binary found"
  info "install it with:  cargo install kmp-mcp"
  info "or run /kmp:setup, which does that and the host wiring"
  exit 1
fi
ok "binary $BIN"

if [ ! -f "$BUNDLE" ]; then
  fail "the demo bundle is missing from this plugin install"
  info "expected at $BUNDLE"
  exit 1
fi

# ------------------------------------------------------------ data dir ----
#
# A separate directory, always. The import refuses a non-empty store anyway,
# but refusing is not the point: the point is that a demo must not be able to
# put example data anywhere near real memory.
if [ -e "${DEMO_DIR}/FORMAT_VERSION" ]; then
  ok "demo memory already loaded at ${DEMO_DIR}"
  info "nothing re-imported — the store is left exactly as it is"
else
  say ""
  say "${B}Importing${Z}"
  mkdir -p "$DEMO_DIR" || { fail "could not create ${DEMO_DIR}"; exit 1; }
  OUT="$(KMP_MCP_DATA_DIR="$DEMO_DIR" "$BIN" import "$BUNDLE" 2>&1)"
  STATUS=$?
  if [ $STATUS -ne 0 ]; then
    fail "import failed"
    printf '%s\n' "$OUT" | sed 's/^/        /'
    exit 1
  fi
  ok "imported into ${DEMO_DIR}"
  info "$(printf '%s' "$OUT" | tail -1)"
fi

# ------------------------------------------------------------- what now ----
say ""
say "${B}What is in it${Z}"
info "An incident: checkout p99 tripled after a deploy, the obvious cause was"
info "rolled back, the rollback did not help, and the real cause turned out to"
info "be a client timeout change that turned every slow request into six."
info ""
info "The wrong turn is deliberate. It is what makes \"what did we believe at"
info "15:05\" a question worth asking."

say ""
say "${B}Try it${Z}"
info "Point a session at it and use the ten moves:"
say ""
say "    export KMP_MCP_DATA_DIR=${DEMO_DIR}"
say ""
info "Then, from an agent with KMP wired:"
info "  kernel_wake    about: incident:checkout-latency"
info "  kernel_ask     \"why did the rollback not fix the latency\""
info "  kernel_trace   from the first symptom to the constraint that closed it"
say ""
info "Or look at it directly, no agent involved:"
say ""
say "    KMP_MCP_DATA_DIR=${DEMO_DIR} kmp-mcp viewer 127.0.0.1:7317"
say ""
info "Loopback only, read-only, no auth."

say ""
say "${B}When you are done${Z}"
info "It is one directory and nothing else: rm -rf ${DEMO_DIR}"
say ""
