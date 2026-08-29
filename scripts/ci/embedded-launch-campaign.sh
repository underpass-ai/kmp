#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
campaign="${root}/campaign/embedded-launch"
manifest="${campaign}/evidence-pack/manifest.json"
final_hook="${campaign}/scripts/final-regeneration-gate.sh"
python="${PYTHON:-python3}"

note() {
  printf '%s\n' "$1"
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '%s\n' "$1" >>"${GITHUB_STEP_SUMMARY}"
  fi
}

cd "${root}"

"${python}" campaign/embedded-launch/scripts/validate-campaign.py
"${python}" campaign/embedded-launch/scripts/test_panel_contract.py
"${python}" campaign/embedded-launch/scripts/test_capture_portability.py
"${python}" campaign/embedded-launch/scripts/test_final_media_contract.py
node campaign/embedded-launch/obs-harness/scripts/test-obs-websocket-auth.mjs
"${python}" campaign/embedded-launch/scripts/freeze-product-evidence.py check

for scenario in campaign/embedded-launch/obs-harness/scenarios/*.json; do
  "${python}" campaign/embedded-launch/obs-harness/scripts/validate-scenario.py "${scenario}"
done

mkdir -p "${root}/tmp"
audio_first="$(mktemp -d "${root}/tmp/embedded-launch-audio-source-a.XXXXXX")"
audio_repeat="$(mktemp -d "${root}/tmp/embedded-launch-audio-source-b.XXXXXX")"
cleanup_audio() {
  rm -rf -- "${audio_first}" "${audio_repeat}"
}
trap cleanup_audio EXIT
"${python}" campaign/embedded-launch/scripts/render-campaign.py --audio-only "${audio_first}"
"${python}" campaign/embedded-launch/scripts/render-campaign.py --audio-only "${audio_repeat}"
"${python}" campaign/embedded-launch/scripts/test_audio_contract.py \
  "${audio_first}" "${audio_repeat}"
cleanup_audio
trap - EXIT

if [[ ! -f "${manifest}" ]]; then
  note "Campaign CI: SOURCE VERIFIED; FINAL EVIDENCE NOT RUN (manifest absent)."
  exit 0
fi

note "Campaign CI: final evidence manifest detected; entering fail-closed release gate."
"${python}" campaign/embedded-launch/scripts/build-evidence-manifest.py check
"${python}" campaign/embedded-launch/scripts/panel_contract.py check
"${python}" campaign/embedded-launch/scripts/verify-final-media.py

# This hook is intentionally absent until the clean-room command is
# encapsulated. Its contract is: render all procedural audio and all three MP4
# masters into the supplied repository-local scratch directory, compare the
# regenerated canonical PCM and distribution artifacts with the evidence pack,
# and return non-zero on any drift. A final manifest without that executable is
# a hard failure; CI must never describe a static-only check as launch GO.
if [[ ! -x "${final_hook}" ]]; then
  note "Campaign CI: FINAL BLOCKED — scripts/final-regeneration-gate.sh is not defined."
  exit 1
fi

mkdir -p "${root}/tmp"
scratch="$(mktemp -d "${root}/tmp/embedded-launch-final-ci.XXXXXX")"
cleanup() {
  rm -rf -- "${scratch}"
}
trap cleanup EXIT
"${final_hook}" --scratch "${scratch}"
note "Campaign CI: FINAL EVIDENCE VERIFIED, including deterministic regeneration."
