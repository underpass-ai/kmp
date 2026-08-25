#!/usr/bin/env bash
#
# Update the plugin-side files and engine as one user action. This is called by
# /kmp:setup after the session-start notice finds a newer release.

set -euo pipefail

REPO="underpass-ai/kmp"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:-$(cd "${SCRIPT_DIR}/.." && pwd)}"
VERSION=""
DO_CLAUDE=0
DO_CODEX=0
DRY_RUN=0
STANDALONE=0

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:?--version needs X.Y.Z}"; shift 2 ;;
    --claude) DO_CLAUDE=1; shift ;;
    --codex) DO_CODEX=1; shift ;;
    --standalone) STANDALONE=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "kmp-update: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

if [ "$DO_CLAUDE" -eq 0 ] && [ "$DO_CODEX" -eq 0 ]; then
  [ -n "${CLAUDE_PLUGIN_ROOT:-}" ] && DO_CLAUDE=1
  [ -f "$HOME/.codex/config.toml" ] && DO_CODEX=1
  if [ "$DO_CLAUDE" -eq 0 ] && [ "$DO_CODEX" -eq 0 ]; then
    command -v claude >/dev/null 2>&1 && DO_CLAUDE=1
    command -v codex >/dev/null 2>&1 && DO_CODEX=1
  fi
fi

if [ "$STANDALONE" -eq 1 ] && [ "$DO_CODEX" -eq 0 ]; then
  echo "kmp-update: --standalone is a Codex mode" >&2
  exit 2
fi

if [ -z "$VERSION" ]; then
  response="$(curl --proto '=https' --tlsv1.2 --connect-timeout 2 --max-time 10 \
    -fsSL -H 'Accept: application/vnd.github+json' -H 'User-Agent: kmp-update' \
    "https://api.github.com/repos/${REPO}/releases/latest")"
  VERSION="$(printf '%s' "$response" | python3 -c \
    'import json,sys; print(json.load(sys.stdin)["tag_name"].removeprefix("v"))')"
fi
VERSION="${VERSION#v}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?$ ]] || {
  echo "kmp-update: invalid release version '${VERSION}'" >&2
  exit 1
}

say() { printf '%s\n' "$*"; }
run() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf 'would: '
    printf '%q ' "$@"
    printf '\n'
  else
    "$@"
  fi
}

say "KMP ${VERSION} — updating the installed halves together"

if [ "$DO_CLAUDE" -eq 1 ]; then
  if [ "$DRY_RUN" -eq 0 ]; then
    command -v claude >/dev/null 2>&1 || {
      echo "kmp-update: Claude Code was requested but 'claude' is not on PATH" >&2
      exit 1
    }
  fi
  run claude plugin update kmp@underpass --yes
fi

if [ "$DO_CODEX" -eq 1 ] && [ "$STANDALONE" -eq 0 ]; then
  if [ "$DRY_RUN" -eq 0 ]; then
    command -v codex >/dev/null 2>&1 || {
      echo "kmp-update: plugin-managed Codex was requested but 'codex' is not on PATH" >&2
      exit 1
    }
  fi
  run codex plugin add kmp@underpass
fi

if [ -f "${SCRIPT_DIR}/kmp-install-binary.sh" ]; then
  # Codex installs both helpers beside each other under its data directory.
  INSTALLER="${SCRIPT_DIR}/kmp-install-binary.sh"
else
  INSTALLER="${PLUGIN_ROOT}/scripts/kmp-install-binary.sh"
fi
[ -f "$INSTALLER" ] || {
  echo "kmp-update: engine installer is missing at ${INSTALLER}" >&2
  exit 1
}

if [ -x "${PLUGIN_ROOT}/bin/kmp-mcp" ]; then
  run bash "$INSTALLER" --version "$VERSION" --dir "${PLUGIN_ROOT}/bin"
else
  run bash "$INSTALLER" --version "$VERSION"
fi

if [ "$DO_CODEX" -eq 1 ]; then
  if [ "$STANDALONE" -eq 0 ]; then
    say "Codex plugin owns MCP and skills; no standalone prompts or registration were changed"
  else
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    tagged_raw="https://raw.githubusercontent.com/${REPO}/v${VERSION}"
    if [ "$DRY_RUN" -eq 1 ]; then
      say "would: fetch ${tagged_raw}/scripts/mcp/install-kmp-plugin.sh"
      say "would: refresh the standalone Codex prompts, doctrine and registration from v${VERSION}"
    else
      curl --proto '=https' --tlsv1.2 -fsSL \
        "${tagged_raw}/scripts/mcp/install-kmp-plugin.sh" -o "${work}/install-kmp-plugin.sh"
      KMP_PLUGIN_RAW_BASE="$tagged_raw" \
        bash "${work}/install-kmp-plugin.sh" --codex --standalone --version "$VERSION"
    fi
  fi
fi

say "KMP ${VERSION} is in place. Restart the host so it loads the new plugin and tools."
