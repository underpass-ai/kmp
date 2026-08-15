#!/usr/bin/env bash
# Package the KMP plugin bundle for Codex and Claude Code.
#
# Single source of truth for the version: the workspace Cargo.toml. On a
# `v*` tag (CI release) the tag must match the workspace version exactly —
# a release never lies about what it contains. Outside a tag the package
# gets `+<short-sha>` build metadata so a dev tarball can never pass for
# the release it merely resembles.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PLUGIN_DIR="${ROOT_DIR}/plugins/kmp"
DIST_DIR="${ROOT_DIR}/dist/plugin"
STAGE_DIR="${DIST_DIR}/stage"

cd "${ROOT_DIR}"

WORKSPACE_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
if [[ -z "${WORKSPACE_VERSION}" ]]; then
  echo "KMP plugin package: could not read the workspace version" >&2
  exit 1
fi

TAG_NAME="${GITHUB_REF_NAME:-}"
if [[ "${TAG_NAME}" == v* ]]; then
  TAG_VERSION="${TAG_NAME#v}"
  if [[ "${TAG_VERSION}" != "${WORKSPACE_VERSION}" ]]; then
    echo "KMP plugin package: tag ${TAG_NAME} does not match workspace version ${WORKSPACE_VERSION}" >&2
    exit 1
  fi
  PACKAGE_VERSION="${WORKSPACE_VERSION}"
else
  SHORT_SHA="$(git rev-parse --short HEAD)"
  PACKAGE_VERSION="${WORKSPACE_VERSION}+${SHORT_SHA}"
fi

# Build the MCP binary and place it at bin/kmp-mcp.
bash scripts/plugin/build-local-kmp-plugin.sh

# Stamp the resolved version into both host manifests.
python3 - "${PACKAGE_VERSION}" <<'EOF'
import json
import pathlib
import sys

version = sys.argv[1]
plugin_dir = pathlib.Path("plugins/kmp")
for manifest in (".codex-plugin/plugin.json", ".claude-plugin/plugin.json"):
    path = plugin_dir / manifest
    data = json.loads(path.read_text())
    data["version"] = version
    path.write_text(json.dumps(data, indent=2) + "\n")
EOF

# Stage a clean copy named after the plugin so the tarball unpacks as
# `kmp/` on any host. `bin/` is gitignored, so it is copied explicitly.
rm -rf "${STAGE_DIR}"
mkdir -p "${STAGE_DIR}/kmp/bin"
cp -R "${PLUGIN_DIR}/.codex-plugin" "${STAGE_DIR}/kmp/.codex-plugin"
cp -R "${PLUGIN_DIR}/.claude-plugin" "${STAGE_DIR}/kmp/.claude-plugin"
cp -R "${PLUGIN_DIR}/.mcp.json" "${STAGE_DIR}/kmp/.mcp.json"
cp -R "${PLUGIN_DIR}/README.md" "${STAGE_DIR}/kmp/README.md"
cp -R "${PLUGIN_DIR}/skills" "${STAGE_DIR}/kmp/skills"
cp -R "${PLUGIN_DIR}/commands" "${STAGE_DIR}/kmp/commands"
cp -R "${PLUGIN_DIR}/codex" "${STAGE_DIR}/kmp/codex"
cp -R "${PLUGIN_DIR}/scripts" "${STAGE_DIR}/kmp/scripts"
cp "${PLUGIN_DIR}/bin/kmp-mcp"* "${STAGE_DIR}/kmp/bin/"
chmod +x "${STAGE_DIR}/kmp/scripts/run-embedded-mcp.sh" "${STAGE_DIR}/kmp/scripts/kmp-doctor.sh"
[[ -f "${STAGE_DIR}/kmp/bin/kmp-mcp" ]] && chmod +x "${STAGE_DIR}/kmp/bin/kmp-mcp"

OS_NAME="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "${OS_NAME}" in
  linux)                OS_LABEL="linux" ;;
  darwin)               OS_LABEL="macos" ;;
  mingw*|msys*|cygwin*) OS_LABEL="windows" ;;
  *)                    OS_LABEL="${OS_NAME}" ;;
esac
ARCH_NAME="$(uname -m)"
case "${ARCH_NAME}" in
  aarch64) ARCH_LABEL="arm64" ;;
  *)       ARCH_LABEL="${ARCH_NAME}" ;;
esac

ARCHIVE_NAME="kmp-plugin-${PACKAGE_VERSION}-${OS_LABEL}-${ARCH_LABEL}.tar.gz"
mkdir -p "${DIST_DIR}"
tar -czf "${DIST_DIR}/${ARCHIVE_NAME}" -C "${STAGE_DIR}" kmp
rm -rf "${STAGE_DIR}"

if command -v sha256sum >/dev/null 2>&1; then
  (cd "${DIST_DIR}" && sha256sum "${ARCHIVE_NAME}" > "${ARCHIVE_NAME}.sha256")
else
  (cd "${DIST_DIR}" && shasum -a 256 "${ARCHIVE_NAME}" > "${ARCHIVE_NAME}.sha256")
fi

echo "KMP plugin package: ${DIST_DIR}/${ARCHIVE_NAME}"
echo "KMP plugin package version: ${PACKAGE_VERSION}"
