#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

for command in node npm ffmpeg; do
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
node scripts/demo/record-chronoloom-gifs.js

encode_six_states() {
  local ready="$1" question="$2" selection="$3" followup="$4" trace="$5" nice="$6" output="$7"
  ffmpeg -hide_banner -loglevel error -y \
    -loop 1 -t 2.5 -i "${ready}" \
    -loop 1 -t 3.8 -i "${question}" \
    -loop 1 -t 6.0 -i "${selection}" \
    -loop 1 -t 3.8 -i "${followup}" \
    -loop 1 -t 4.8 -i "${trace}" \
    -loop 1 -t 7.0 -i "${nice}" \
    -filter_complex \
      "[0:v]fps=10,format=rgba,settb=AVTB[a]; \
       [1:v]fps=10,format=rgba,settb=AVTB[b]; \
       [2:v]fps=10,format=rgba,settb=AVTB[c]; \
       [3:v]fps=10,format=rgba,settb=AVTB[d]; \
       [4:v]fps=10,format=rgba,settb=AVTB[e]; \
       [5:v]fps=10,format=rgba,settb=AVTB[f]; \
       [a][b]xfade=transition=fade:duration=0.30:offset=2.20[ab]; \
       [ab][c]xfade=transition=fade:duration=0.35:offset=5.65[abc]; \
       [abc][d]xfade=transition=fade:duration=0.35:offset=11.30[abcd]; \
       [abcd][e]xfade=transition=fade:duration=0.35:offset=14.75[abcde]; \
       [abcde][f]xfade=transition=fade:duration=0.40:offset=19.15[scene]; \
       [scene]fps=10,scale=1600:640:flags=lanczos,split[palette_source][pixels]; \
       [palette_source]palettegen=max_colors=112:stats_mode=diff[palette]; \
       [pixels][palette]paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle[out]" \
    -map "[out]" -loop 0 "${output}"
}

encode_six_states \
  "${CAPTURE_ROOT}/states/agent-01-ready.png" \
  "${CAPTURE_ROOT}/states/agent-02-question.png" \
  "${CAPTURE_ROOT}/states/agent-03-selection.png" \
  "${CAPTURE_ROOT}/states/agent-04-followup.png" \
  "${CAPTURE_ROOT}/states/agent-05-trace.png" \
  "${CAPTURE_ROOT}/states/agent-06-nice.png" \
  "${ROOT}/docs/assets/kmp-agent-loom.gif"

echo "record-chronoloom-gifs: wrote"
du -h docs/assets/kmp-agent-loom.gif
