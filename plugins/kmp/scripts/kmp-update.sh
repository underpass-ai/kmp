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
CODEX_PLUGIN_ROOT=""

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
  # Each host owns an independent plugin cache. Detect each one independently:
  # finding Codex on disk must not suppress the PATH probe that discovers
  # Claude Code (or vice versa) when this updater is run from a plain shell.
  command -v claude >/dev/null 2>&1 && DO_CLAUDE=1
  command -v codex >/dev/null 2>&1 && DO_CODEX=1
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
TAGGED_RAW="https://raw.githubusercontent.com/${REPO}/v${VERSION}"

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

# A native plugin update may replace the cache directory this script is
# running from. Resolve and stage the sibling engine installer before asking
# either host to mutate that directory, so the second half of the update
# remains available afterwards.
if [ -f "${SCRIPT_DIR}/kmp-install-binary.sh" ]; then
  # Codex installs both helpers beside each other under its data directory.
  INSTALLER_SOURCE="${SCRIPT_DIR}/kmp-install-binary.sh"
else
  INSTALLER_SOURCE="${PLUGIN_ROOT}/scripts/kmp-install-binary.sh"
fi
[ -f "$INSTALLER_SOURCE" ] || {
  echo "kmp-update: engine installer is missing at ${INSTALLER_SOURCE}" >&2
  exit 1
}

INSTALLER="$INSTALLER_SOURCE"
UPDATE_WORK=""
if [ "$DRY_RUN" -eq 0 ]; then
  UPDATE_WORK="$(mktemp -d)"
  trap 'rm -rf "${UPDATE_WORK}"' EXIT
  cp "$INSTALLER_SOURCE" "${UPDATE_WORK}/kmp-install-binary.sh"
  INSTALLER="${UPDATE_WORK}/kmp-install-binary.sh"

  # Stage the target release's guide independently of either host cache. A
  # plugin manager may replace the directory this updater started from.
  mkdir -p "${UPDATE_WORK}/guide-plugin/scripts" "${UPDATE_WORK}/guide-plugin/guide"
  if [ -n "${KMP_UPDATE_GUIDE_SOURCE_DIR:-}" ]; then
    cp "${KMP_UPDATE_GUIDE_SOURCE_DIR}/capabilities.json" \
      "${UPDATE_WORK}/guide-plugin/capabilities.json"
    cp "${KMP_UPDATE_GUIDE_SOURCE_DIR}/scripts/kmp-guide-sync.sh" \
      "${UPDATE_WORK}/guide-plugin/scripts/kmp-guide-sync.sh"
    for asset in build-guide.py editorial.json guide.requests.json memory.jsonl; do
      cp "${KMP_UPDATE_GUIDE_SOURCE_DIR}/guide/${asset}" \
        "${UPDATE_WORK}/guide-plugin/guide/${asset}"
    done
  else
    curl --proto '=https' --tlsv1.2 -fsSL \
      "${TAGGED_RAW}/plugins/kmp/capabilities.json" \
      -o "${UPDATE_WORK}/guide-plugin/capabilities.json"
    curl --proto '=https' --tlsv1.2 -fsSL \
      "${TAGGED_RAW}/plugins/kmp/scripts/kmp-guide-sync.sh" \
      -o "${UPDATE_WORK}/guide-plugin/scripts/kmp-guide-sync.sh"
    for asset in build-guide.py editorial.json guide.requests.json memory.jsonl; do
      curl --proto '=https' --tlsv1.2 -fsSL \
        "${TAGGED_RAW}/plugins/kmp/guide/${asset}" \
        -o "${UPDATE_WORK}/guide-plugin/guide/${asset}"
    done
  fi
  chmod +x "${UPDATE_WORK}/guide-plugin/scripts/kmp-guide-sync.sh" \
    "${UPDATE_WORK}/guide-plugin/guide/build-guide.py"
fi

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
  if [ "$DRY_RUN" -eq 1 ]; then
    run codex plugin marketplace upgrade underpass --json
    run codex plugin add kmp@underpass --json
  else
    # `plugin add` installs from the configured snapshot; it does not promise
    # that a Git-backed marketplace is current. Refresh first when possible.
    # A local development marketplace is intentionally not upgradeable, so
    # continue with its current files and let the exact-version check below
    # decide whether they are usable.
    if ! codex plugin marketplace upgrade underpass --json >/dev/null 2>&1; then
      say "Codex marketplace 'underpass' could not be refreshed automatically; checking its configured snapshot"
    fi

    CODEX_INSTALL="$(codex plugin add kmp@underpass --json)"
    if ! CODEX_PLUGIN="$(printf '%s' "$CODEX_INSTALL" | python3 -c '
import json
import sys

body = json.load(sys.stdin)
print(body["version"] + "\t" + body["installedPath"])
')"; then
      echo "kmp-update: Codex did not return a valid plugin install result" >&2
      exit 1
    fi
    CODEX_PLUGIN_VERSION="${CODEX_PLUGIN%%$'\t'*}"
    CODEX_PLUGIN_ROOT="${CODEX_PLUGIN#*$'\t'}"
    case "$CODEX_PLUGIN_ROOT" in
      /)
        echo "kmp-update: Codex returned an unsafe installedPath '/'" >&2
        exit 1
        ;;
      /*) ;;
      *)
        echo "kmp-update: Codex returned a non-absolute installedPath '${CODEX_PLUGIN_ROOT}'" >&2
        exit 1
        ;;
    esac
    case "$CODEX_PLUGIN_VERSION" in
      "$VERSION"|"$VERSION"+*) ;;
      *)
        echo "kmp-update: Codex marketplace installed plugin ${CODEX_PLUGIN_VERSION}, but release ${VERSION} was requested" >&2
        echo "kmp-update: the engine was not changed; publish or refresh kmp@underpass, then retry" >&2
        exit 1
        ;;
    esac
    say "Codex plugin ${CODEX_PLUGIN_VERSION} installed at ${CODEX_PLUGIN_ROOT}"
  fi
fi

if [ "$DO_CODEX" -eq 1 ] && [ "$STANDALONE" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
  # PLUGIN_ROOT names the cache this updater started from. `plugin add` may
  # install the requested release into a different cache, so only the exact
  # installedPath returned by Codex is authoritative after that call.
  if [ -n "${CLAUDE_PLUGIN_ROOT:-}" ] \
      && [ -x "${PLUGIN_ROOT}/bin/kmp-mcp" ] \
      && [ "$PLUGIN_ROOT" != "$CODEX_PLUGIN_ROOT" ]; then
    run bash "$INSTALLER" --version "$VERSION" \
      --also-dir "${CODEX_PLUGIN_ROOT}/bin" --also-dir "${PLUGIN_ROOT}/bin"
  else
    run bash "$INSTALLER" --version "$VERSION" --also-dir "${CODEX_PLUGIN_ROOT}/bin"
  fi
elif [ "$DO_CODEX" -eq 1 ] && [ "$STANDALONE" -eq 0 ] && [ "$DRY_RUN" -eq 1 ]; then
  run bash "$INSTALLER" --version "$VERSION" --also-dir '<Codex installedPath>/bin'
elif [ -x "${PLUGIN_ROOT}/bin/kmp-mcp" ]; then
  run bash "$INSTALLER" --version "$VERSION" --dir "${PLUGIN_ROOT}/bin"
else
  run bash "$INSTALLER" --version "$VERSION"
fi

if [ "$DO_CODEX" -eq 1 ]; then
  if [ "$STANDALONE" -eq 0 ]; then
    say "Codex plugin owns MCP and skills; no standalone prompts or registration were changed"
  else
    if [ "$DRY_RUN" -eq 1 ]; then
      say "would: fetch ${TAGGED_RAW}/scripts/mcp/install-kmp-plugin.sh"
      say "would: refresh the standalone Codex prompts, doctrine and registration from v${VERSION}"
    else
      curl --proto '=https' --tlsv1.2 -fsSL \
        "${TAGGED_RAW}/scripts/mcp/install-kmp-plugin.sh" \
        -o "${UPDATE_WORK}/install-kmp-plugin.sh"
      KMP_PLUGIN_RAW_BASE="$TAGGED_RAW" \
        bash "${UPDATE_WORK}/install-kmp-plugin.sh" \
          --codex --standalone --version "$VERSION"
    fi
  fi
fi

if [ "$DRY_RUN" -eq 1 ]; then
  say "would: converge guide:kmp-agent and guide:kmp from v${VERSION} without duplicating events"
else
  if [ -n "${KMP_MCP_BIN:-}" ] && [ -x "$KMP_MCP_BIN" ]; then
    GUIDE_BIN="$KMP_MCP_BIN"
  elif [ -n "$CODEX_PLUGIN_ROOT" ] && [ -x "${CODEX_PLUGIN_ROOT}/bin/kmp-mcp" ]; then
    GUIDE_BIN="${CODEX_PLUGIN_ROOT}/bin/kmp-mcp"
  elif [ -x "${PLUGIN_ROOT}/bin/kmp-mcp" ]; then
    GUIDE_BIN="${PLUGIN_ROOT}/bin/kmp-mcp"
  elif command -v kmp-mcp >/dev/null 2>&1; then
    GUIDE_BIN="$(command -v kmp-mcp)"
  else
    echo 'kmp-update: updated engine is not reachable for guide sync' >&2
    exit 1
  fi
  KMP_GUIDE_PLUGIN_ROOT="${UPDATE_WORK}/guide-plugin" \
    bash "${UPDATE_WORK}/guide-plugin/scripts/kmp-guide-sync.sh" \
      sync --binary "$GUIDE_BIN"
fi

say "KMP ${VERSION} is in place. Restart the host so it loads the new plugin and tools."
