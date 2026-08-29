#!/usr/bin/env bash
set -euo pipefail

CAMPAIGN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${1:?usage: render-audio.sh BUILD_DIR}"
SOURCE="${CAMPAIGN_DIR}/audio/evidence-knot.csd"
CUES="${CAMPAIGN_DIR}/audio/cues.tsv"
CONTRACT="${CAMPAIGN_DIR}/audio/contract.json"
GATE="${CAMPAIGN_DIR}/scripts/audio_contract.py"
mkdir -p "$BUILD_DIR/cues"
cp "$SOURCE" "$BUILD_DIR/evidence-knot.csd"

(
  cd "$BUILD_DIR"
  csound evidence-knot.csd >/dev/null
)

tail -n +2 "$CUES" | while IFS=$'\t' read -r cue start end; do
  ffmpeg -nostdin -hide_banner -loglevel error -y \
    -i "$BUILD_DIR/evidence-knot-palette.wav" \
    -af "atrim=start=${start}:end=${end},asetpts=PTS-STARTPTS" \
    -ar 48000 -ac 2 -c:a pcm_s24le "$BUILD_DIR/cues/${cue}.wav"
done

python3 "$GATE" palette-provenance "$BUILD_DIR"
cp "$SOURCE" "$CUES" "$CONTRACT" "$BUILD_DIR/"
echo "KMP campaign audio: rendered 48 kHz/24-bit procedural Evidence Knot palette"
