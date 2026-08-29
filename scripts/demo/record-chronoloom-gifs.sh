#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

for command in node npm sha256sum; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "record-chronoloom-gifs: ${command} is required" >&2
    exit 1
  }
done

CAPTURE_CHROME="${KMP_CAPTURE_CHROME:-$(command -v google-chrome || true)}"
if [ -z "${CAPTURE_CHROME}" ]; then
  echo "record-chronoloom-gifs: set KMP_CAPTURE_CHROME to a Chrome/Chromium binary" >&2
  exit 1
fi

if [ -z "${KMP_MCP_BIN:-}" ]; then
  cargo build -p kmp-mcp --locked
  BIN="${ROOT}/target/debug/kmp-mcp"
else
  BIN="${KMP_MCP_BIN}"
  if [ ! -x "${BIN}" ]; then
    echo "record-chronoloom-gifs: KMP_MCP_BIN is not executable: ${BIN}" >&2
    exit 1
  fi
fi

mkdir -p "${ROOT}/tmp" "${ROOT}/docs/assets"
CAPTURE_ROOT="$(mktemp -d "${ROOT}/tmp/chronoloom-gifs.XXXXXX")"
cleanup() {
  if [ "${KMP_KEEP_CHRONOLOOM_FRAMES:-0}" = "1" ]; then
    echo "record-chronoloom-gifs: kept capture states in ${CAPTURE_ROOT}"
  else
    rm -rf "${CAPTURE_ROOT}"
  fi
}
trap cleanup EXIT

echo "record-chronoloom-gifs: installing pinned Playwright into disposable scratch"
PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm install \
  --prefix "${CAPTURE_ROOT}/playwright" \
  --no-audit --no-fund --no-save --ignore-scripts \
  playwright@1.62.1 >/dev/null

export NODE_PATH="${CAPTURE_ROOT}/playwright/node_modules"
export KMP_CHRONOLOOM_CAPTURE_ROOT="${CAPTURE_ROOT}"
export KMP_CAPTURE_CHROME="${CAPTURE_CHROME}"
export KMP_MCP_BIN="${BIN}"
export KMP_CAMPAIGN_COMMIT="$(git rev-parse HEAD)"
if [ -n "$(git status --short)" ]; then
  export KMP_CAMPAIGN_WORKTREE_DIRTY=true
else
  export KMP_CAMPAIGN_WORKTREE_DIRTY=false
fi
node scripts/demo/record-chronoloom-gifs.js

EVIDENCE="${ROOT}/campaign/embedded-launch/evidence-pack/capture/product-probe"
PRODUCT="${ROOT}/campaign/embedded-launch/evidence-pack/product"
mkdir -p "${EVIDENCE}/states" "${PRODUCT}"
cp "${CAPTURE_ROOT}/capture-evidence.json" "${EVIDENCE}/capture-evidence.json"
cp "${CAPTURE_ROOT}/tool-calls.jsonl" "${EVIDENCE}/tool-calls.jsonl"
cp "${CAPTURE_ROOT}/tools-list.json" "${EVIDENCE}/tools-list.json"
cp "${CAPTURE_ROOT}"/states/*.png "${EVIDENCE}/states/"
cp "${CAPTURE_ROOT}/tools-list.json" "${PRODUCT}/tools-list.json"
sha256sum "${BIN}" | cut -d' ' -f1 > "${PRODUCT}/binary.sha256"
"${BIN}" --version > "${PRODUCT}/version.txt"
(
  cd "${EVIDENCE}"
  find . -type f ! -name SHA256SUMS -print0 \
    | sort -z \
    | xargs -0 sha256sum > SHA256SUMS
)

echo "record-chronoloom-gifs: wrote browser/product evidence only"
echo "record-chronoloom-gifs: final campaign picture must come from the OBS capture adapter"
du -sh "${EVIDENCE}"
