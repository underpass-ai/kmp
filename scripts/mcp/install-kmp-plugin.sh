#!/usr/bin/env bash
#
# install-kmp-plugin.sh — wire KMP agent memory into the coding agents on this
# machine.
#
#   bash scripts/mcp/install-kmp-plugin.sh            # detect and wire both
#   bash scripts/mcp/install-kmp-plugin.sh --codex    # Codex CLI only
#   bash scripts/mcp/install-kmp-plugin.sh --claude   # Claude Code only
#   bash scripts/mcp/install-kmp-plugin.sh --dry-run  # show, change nothing
#
# Idempotent: re-running it is safe and reports "already wired" rather than
# duplicating configuration. Works from a checkout or standalone, in which
# case the assets it needs are fetched from the repository.
#
# Claude Code installs the plugin natively from the marketplace, which brings
# the MCP server with it, so there is nothing to write into its config here.
# Codex has no plugin system, so this script does the wiring by hand.

set -euo pipefail

REPO_RAW="${KMP_PLUGIN_RAW_BASE:-https://raw.githubusercontent.com/underpass-ai/kmp/main}"
DO_CODEX=0
DO_CLAUDE=0
DRY_RUN=0

while [ $# -gt 0 ]; do
  case "$1" in
    --codex)   DO_CODEX=1 ;;
    --claude)  DO_CLAUDE=1 ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

if [ "$DO_CODEX" -eq 0 ] && [ "$DO_CLAUDE" -eq 0 ]; then
  command -v codex  >/dev/null 2>&1 && DO_CODEX=1
  command -v claude >/dev/null 2>&1 && DO_CLAUDE=1
  if [ "$DO_CODEX" -eq 0 ] && [ "$DO_CLAUDE" -eq 0 ]; then
    echo "Neither Codex CLI nor Claude Code found on PATH."
    echo "Name a host explicitly with --codex or --claude if it is installed elsewhere."
    exit 1
  fi
fi

say()  { printf '%s\n' "$*"; }
step() { printf '\n== %s\n' "$*"; }
act()  { if [ "$DRY_RUN" -eq 1 ]; then printf '   would: %s\n' "$*"; else printf '   %s\n' "$*"; fi; }

# Resolve plugin assets from the checkout when we are in one, otherwise fetch.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLUGIN_DIR="$(cd "$SCRIPT_DIR/../../plugins/kmp" 2>/dev/null && pwd || true)"

fetch_asset() {
  # fetch_asset <path-relative-to-plugin-dir> <destination>
  local rel="$1" dest="$2"
  if [ -n "$PLUGIN_DIR" ] && [ -f "$PLUGIN_DIR/$rel" ]; then
    cp "$PLUGIN_DIR/$rel" "$dest"
  else
    curl -fsSL "$REPO_RAW/plugins/kmp/$rel" -o "$dest"
  fi
}

if [ -n "$PLUGIN_DIR" ]; then
  say "Using plugin assets from the checkout at $PLUGIN_DIR"
else
  say "Not in a checkout — fetching plugin assets from $REPO_RAW"
fi

# ---------------------------------------------------------------- binary ----
step "Binary"

BIN="${KMP_MCP_BIN:-}"
if [ -n "$BIN" ] && [ ! -x "$BIN" ]; then
  echo "   KMP_MCP_BIN is set to $BIN but that is not executable" >&2
  exit 1
fi
if [ -z "$BIN" ]; then
  if command -v kmp-mcp >/dev/null 2>&1; then
    BIN="$(command -v kmp-mcp)"
  elif [ -x "$HOME/.cargo/bin/kmp-mcp" ]; then
    BIN="$HOME/.cargo/bin/kmp-mcp"
  fi
fi

if [ -n "$BIN" ]; then
  say "   already installed: $BIN"
else
  if [ "$DRY_RUN" -eq 1 ]; then
    act "cargo install --git https://github.com/underpass-ai/kmp kmp-mcp --locked"
    BIN="$HOME/.cargo/bin/kmp-mcp"
  else
    command -v cargo >/dev/null 2>&1 || {
      echo "   kmp-mcp is missing and cargo is not available to build it." >&2
      echo "   Install Rust, or take a prebuilt binary from docs/operations/embedded-release.md" >&2
      exit 1
    }
    say "   installing kmp-mcp (this compiles, give it a few minutes)"
    cargo install --git https://github.com/underpass-ai/kmp kmp-mcp --locked
    BIN="$HOME/.cargo/bin/kmp-mcp"
  fi
fi

# Shared doctor, used by both hosts' prompts.
DOCTOR_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/kmp/bin"
DOCTOR="$DOCTOR_DIR/kmp-doctor.sh"
if [ "$DRY_RUN" -eq 1 ]; then
  act "install the doctor at $DOCTOR"
else
  mkdir -p "$DOCTOR_DIR"
  fetch_asset "scripts/kmp-doctor.sh" "$DOCTOR"
  chmod +x "$DOCTOR"
  say "   doctor installed at $DOCTOR"
fi

# ----------------------------------------------------------------- codex ----
if [ "$DO_CODEX" -eq 1 ]; then
  step "Codex CLI"

  CODEX_HOME="$HOME/.codex"
  CODEX_CONFIG="$CODEX_HOME/config.toml"

  if [ -f "$CODEX_CONFIG" ] && grep -q '^\[mcp_servers\.kernel-memory\]' "$CODEX_CONFIG"; then
    say "   config.toml — kernel-memory already registered, left untouched"
  elif [ "$DRY_RUN" -eq 1 ]; then
    act "append [mcp_servers.kernel-memory] to $CODEX_CONFIG"
  else
    mkdir -p "$CODEX_HOME"
    [ -f "$CODEX_CONFIG" ] && cp "$CODEX_CONFIG" "$CODEX_CONFIG.kmp-backup"
    cat >>"$CODEX_CONFIG" <<EOF

[mcp_servers.kernel-memory]
command = "$BIN"
env = { KMP_MCP_BACKEND = "embedded" }
EOF
    say "   config.toml — registered kernel-memory (embedded)"
    [ -f "$CODEX_CONFIG.kmp-backup" ] && say "   previous config saved as $CODEX_CONFIG.kmp-backup"
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    act "install /kmp-doctor and /kmp-moves into $CODEX_HOME/prompts"
  else
    mkdir -p "$CODEX_HOME/prompts"
    for p in kmp-doctor kmp-moves; do
      fetch_asset "codex/prompts/$p.md" "$CODEX_HOME/prompts/$p.md"
      # The Codex prompts have no plugin-root variable to lean on.
      sed -i.bak "s#@@DOCTOR@@#$DOCTOR#g" "$CODEX_HOME/prompts/$p.md"
      rm -f "$CODEX_HOME/prompts/$p.md.bak"
    done
    say "   prompts — /kmp-doctor and /kmp-moves installed"
  fi

  # The memory doctrine. Codex has no skills, so it lives in AGENTS.md,
  # fenced by markers so re-running replaces it instead of stacking copies.
  AGENTS="$CODEX_HOME/AGENTS.md"
  if [ "$DRY_RUN" -eq 1 ]; then
    act "add the KMP section to $AGENTS"
  else
    SNIPPET="$(mktemp)"
    fetch_asset "codex/AGENTS.kmp.md" "$SNIPPET"
    if [ -f "$AGENTS" ] && grep -q '<!-- kmp:begin -->' "$AGENTS"; then
      python3 - "$AGENTS" "$SNIPPET" <<'PY'
import re, sys
target, snippet = sys.argv[1], sys.argv[2]
with open(target) as fh:
    body = fh.read()
with open(snippet) as fh:
    new = fh.read().strip()
body = re.sub(
    r"<!-- kmp:begin -->.*?<!-- kmp:end -->",
    lambda _: new,
    body,
    flags=re.DOTALL,
)
with open(target, "w") as fh:
    fh.write(body)
PY
      say "   AGENTS.md — KMP section refreshed in place"
    else
      { [ -f "$AGENTS" ] && printf '\n'; cat "$SNIPPET"; } >>"$AGENTS"
      say "   AGENTS.md — KMP section added"
    fi
    rm -f "$SNIPPET"
  fi

  say "   restart any running Codex session: it keeps the MCP inventory it started with"
fi

# ---------------------------------------------------------------- claude ----
if [ "$DO_CLAUDE" -eq 1 ]; then
  step "Claude Code"
  say "   Claude Code installs this natively, and the plugin ships the MCP"
  say "   server with it. Inside a session, run:"
  say ""
  say "     /plugin marketplace add underpass-ai/plugins"
  say "     /plugin install kmp@underpass"
  say ""
  say "   That gives you /kmp:doctor, /kmp:moves and /kmp:setup, the"
  say "   kmp-memory skill, and the kernel-memory server — no extra wiring."
  say ""
  say "   To register the server on its own, without the plugin:"
  say "     claude mcp add kernel-memory --scope user \\"
  say "       --env KMP_MCP_BACKEND=embedded -- $BIN"
fi

step "Check"
if [ "$DRY_RUN" -eq 1 ]; then
  say "   dry run: nothing was changed"
else
  say "   bash $DOCTOR"
fi
