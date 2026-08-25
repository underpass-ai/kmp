#!/usr/bin/env bash
#
# install-kmp-plugin.sh — wire KMP agent memory into the coding agents on this
# machine.
#
#   bash scripts/mcp/install-kmp-plugin.sh            # detect and wire both
#   bash scripts/mcp/install-kmp-plugin.sh --codex    # Codex CLI only
#   bash scripts/mcp/install-kmp-plugin.sh --claude   # Claude Code only
#   bash scripts/mcp/install-kmp-plugin.sh --dry-run  # show, change nothing
#   bash scripts/mcp/install-kmp-plugin.sh --version 0.1.14 --codex
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
TARGET_VERSION=""

while [ $# -gt 0 ]; do
  case "$1" in
    --codex)   DO_CODEX=1 ;;
    --claude)  DO_CLAUDE=1 ;;
    --dry-run) DRY_RUN=1 ;;
    --version) TARGET_VERSION="${2:?--version needs X.Y.Z}"; shift ;;
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

if [ -n "$TARGET_VERSION" ]; then
  TARGET_VERSION="${TARGET_VERSION#v}"
  [[ "$TARGET_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?$ ]] || {
    echo "   invalid target version: $TARGET_VERSION" >&2
    exit 1
  }
fi

INSTALLED_VERSION=""
if [ -n "$BIN" ]; then
  INSTALLED_VERSION="$("$BIN" --version 2>/dev/null | head -1 | sed -E 's/^kmp-mcp ([^ ]+).*/\1/' || true)"
fi

if [ -n "$BIN" ] && { [ -z "$TARGET_VERSION" ] || [ "$INSTALLED_VERSION" = "$TARGET_VERSION" ]; }; then
  say "   already installed: $BIN${INSTALLED_VERSION:+ ($INSTALLED_VERSION)}"
elif [ -n "$TARGET_VERSION" ]; then
  INSTALL_DIR="${KMP_INSTALL_DIR:-$HOME/.local/bin}"
  if [ "$DRY_RUN" -eq 1 ]; then
    act "install checksummed kmp-mcp ${TARGET_VERSION} into ${INSTALL_DIR}"
    BIN="${INSTALL_DIR}/kmp-mcp"
  else
    ENGINE_INSTALLER="$(mktemp)"
    fetch_asset "scripts/kmp-install-binary.sh" "$ENGINE_INSTALLER"
    chmod +x "$ENGINE_INSTALLER"
    bash "$ENGINE_INSTALLER" --version "$TARGET_VERSION" --dir "$INSTALL_DIR"
    rm -f "$ENGINE_INSTALLER"
    BIN="${INSTALL_DIR}/kmp-mcp"
  fi
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

# The engine installer, so /kmp-setup can offer it the same way Claude Code's
# /kmp:setup does. Codex has no plugin-root variable to reach it by.
SETUP="$DOCTOR_DIR/kmp-install-binary.sh"
if [ "$DRY_RUN" -eq 1 ]; then
  act "install the engine installer at $SETUP"
else
  fetch_asset "scripts/kmp-install-binary.sh" "$SETUP"
  chmod +x "$SETUP"
  say "   engine installer at $SETUP"
fi

# One-command update path used by /kmp-setup. It refreshes Codex's copied
# assets from a versioned release and installs the matching checksummed engine.
UPDATE="$DOCTOR_DIR/kmp-update.sh"
if [ "$DRY_RUN" -eq 1 ]; then
  act "install the updater at $UPDATE"
else
  fetch_asset "scripts/kmp-update.sh" "$UPDATE"
  chmod +x "$UPDATE"
  say "   updater at $UPDATE"
fi

# Shared demo. The script resolves its own plugin root from its location, so
# installing it beside the doctor with the bundle one directory over is all it
# needs — no plugin-root variable, which Codex does not have.
DEMO_HOME="${XDG_DATA_HOME:-$HOME/.local/share}/kmp"
DEMO="$DEMO_HOME/bin/kmp-demo.sh"
if [ "$DRY_RUN" -eq 1 ]; then
  act "install the demo at $DEMO"
else
  mkdir -p "$DEMO_HOME/bin" "$DEMO_HOME/demo"
  fetch_asset "scripts/kmp-demo.sh" "$DEMO"
  chmod +x "$DEMO"
  fetch_asset "demo/checkout-latency.jsonl" "$DEMO_HOME/demo/checkout-latency.jsonl"
  say "   demo installed at $DEMO"
fi

# ----------------------------------------------------------------- codex ----
if [ "$DO_CODEX" -eq 1 ]; then
  step "Codex CLI"

  CODEX_HOME="$HOME/.codex"
  CODEX_CONFIG="$CODEX_HOME/config.toml"

  # Rename the whole table prefix, not just the transport table. Codex keeps
  # tool policy in child tables such as
  # [mcp_servers.kernel-memory.tools.kernel_wake]; leaving one behind creates
  # a second server with no command or URL, and Codex then refuses to start
  # with "invalid transport in mcp_servers.kernel-memory".
  if [ -f "$CODEX_CONFIG" ] && grep -q '^\[mcp_servers\.kernel-memory' "$CODEX_CONFIG"; then
    if [ "$DRY_RUN" -eq 1 ]; then
      act "rename every [mcp_servers.kernel-memory...] table to [mcp_servers.kmp...] in $CODEX_CONFIG"
    else
      MIGRATED_CONFIG="$(mktemp)"
      sed 's/^\[mcp_servers\.kernel-memory/[mcp_servers.kmp/' \
        "$CODEX_CONFIG" > "$MIGRATED_CONFIG"
      if ! python3 - "$MIGRATED_CONFIG" <<'PY'
import pathlib
import sys
import tomllib

tomllib.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
PY
      then
        rm -f "$MIGRATED_CONFIG"
        echo "   cannot migrate Codex config: kmp and kernel-memory tables conflict" >&2
        echo "   left $CODEX_CONFIG untouched" >&2
        exit 1
      fi
      cp "$CODEX_CONFIG" "$CODEX_CONFIG.kmp-backup"
      cp "$MIGRATED_CONFIG" "$CODEX_CONFIG"
      rm -f "$MIGRATED_CONFIG"
      say "   config.toml — migrated the kernel-memory registration and tool policies to kmp"
      say "   previous config saved as $CODEX_CONFIG.kmp-backup"
    fi
  elif [ -f "$CODEX_CONFIG" ] && grep -q '^\[mcp_servers\.kmp\]' "$CODEX_CONFIG"; then
    say "   config.toml — kmp already registered, left untouched"
  elif [ "$DRY_RUN" -eq 1 ]; then
    act "append [mcp_servers.kmp] to $CODEX_CONFIG"
  else
    mkdir -p "$CODEX_HOME"
    [ -f "$CODEX_CONFIG" ] && cp "$CODEX_CONFIG" "$CODEX_CONFIG.kmp-backup"
    cat >>"$CODEX_CONFIG" <<EOF

[mcp_servers.kmp]
command = "$BIN"
env = { KMP_MCP_BACKEND = "embedded" }
EOF
    say "   config.toml — registered kmp (embedded)"
    [ -f "$CODEX_CONFIG.kmp-backup" ] && say "   previous config saved as $CODEX_CONFIG.kmp-backup"
  fi

  # Every prompt the plugin ships, not a hand-picked three. The repository
  # carried nine and installed three, so /kmp-save and /kmp-catchup existed
  # for Claude Code users and silently did not for Codex ones.
  CODEX_PROMPTS="kmp-setup kmp-doctor kmp-info kmp-moves kmp-demo kmp-catchup kmp-save kmp-restore kmp-revert kmp-uninstall"
  if [ "$DRY_RUN" -eq 1 ]; then
    act "install $(printf '/%s ' $CODEX_PROMPTS)into $CODEX_HOME/prompts"
  else
    mkdir -p "$CODEX_HOME/prompts"
    for p in $CODEX_PROMPTS; do
      fetch_asset "codex/prompts/$p.md" "$CODEX_HOME/prompts/$p.md"
      # The Codex prompts have no plugin-root variable to lean on.
      sed -i.bak -e "s#@@DOCTOR@@#$DOCTOR#g" -e "s#@@DEMO@@#$DEMO#g" \
        -e "s#@@SETUP@@#$SETUP#g" -e "s#@@UPDATE@@#$UPDATE#g" \
        "$CODEX_HOME/prompts/$p.md"
      rm -f "$CODEX_HOME/prompts/$p.md.bak"
    done
    say "   prompts — $(printf '/%s ' $CODEX_PROMPTS)installed"
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
  say "   kmp-memory skill, and the kmp server — no extra wiring."
  say ""
  say "   To register the server on its own, without the plugin:"
  say "     claude mcp add kmp --scope user \\"
  say "       --env KMP_MCP_BACKEND=embedded -- $BIN"
fi

step "Check"
if [ "$DRY_RUN" -eq 1 ]; then
  say "   dry run: nothing was changed"
else
  say "   bash $DOCTOR"
fi
