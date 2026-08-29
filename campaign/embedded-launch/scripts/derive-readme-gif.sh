#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
MASTER="${ROOT}/docs/assets/campaign/kmp-embedded/fresh-process-same-why.mp4"
OUTPUT="${ROOT}/docs/assets/kmp-agent-loom.gif"
MANIFEST="${ROOT}/campaign/embedded-launch/evidence-pack/manifest.json"
CRITIC="${ROOT}/campaign/embedded-launch/evidence-pack/signoffs/launch-critic.json"
[ -f "$MASTER" ] || { echo "missing master: $MASTER" >&2; exit 1; }
[ -f "$MANIFEST" ] || { echo "missing evidence manifest: $MANIFEST" >&2; exit 1; }
[ -f "$CRITIC" ] || { echo "README GIF refused: missing independent critic result" >&2; exit 1; }

[ "$(jq -r '.status' "$MANIFEST")" = complete ] \
  || { echo "README GIF refused: campaign evidence is incomplete" >&2; exit 1; }
EXPECTED="$(jq -r '.artifacts[] | select(.path == "docs/assets/campaign/kmp-embedded/fresh-process-same-why.mp4") | .sha256' "$MANIFEST")"
[ -n "$EXPECTED" ] && [ "$EXPECTED" != null ] \
  || { echo "README GIF refused: campaign master 1 is not bound by the manifest" >&2; exit 1; }
ACTUAL="$(sha256sum "$MASTER" | cut -d' ' -f1)"
[ "$EXPECTED" = "$ACTUAL" ] \
  || { echo "README GIF refused: campaign master 1 hash does not match the manifest" >&2; exit 1; }
MANIFEST_SHA="$(sha256sum "$MANIFEST" | cut -d' ' -f1)"
[ "$(jq -r '.decision' "$CRITIC")" = GO ] \
  || { echo "README GIF refused: independent critic did not return GO" >&2; exit 1; }
[ "$(jq -r '.input_manifest_sha256' "$CRITIC")" = "$MANIFEST_SHA" ] \
  || { echo "README GIF refused: independent critic result is stale" >&2; exit 1; }
python3 "${ROOT}/campaign/embedded-launch/scripts/build-publication-manifest.py" --preflight

WORK="$(mktemp -d "${ROOT}/tmp/readme-gif.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
ffmpeg -hide_banner -loglevel error -y -i "$MASTER" \
  -vf "fps=8,scale=1200:-2:flags=lanczos,palettegen=max_colors=96:stats_mode=diff" \
  "$WORK/palette.png"
ffmpeg -hide_banner -loglevel error -y -i "$MASTER" -i "$WORK/palette.png" \
  -lavfi "fps=8,scale=1200:-2:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" \
  -loop 0 "$OUTPUT"
python3 "${ROOT}/campaign/embedded-launch/scripts/build-publication-manifest.py"
echo "KMP campaign: derived README GIF from fresh-process-same-why.mp4"
