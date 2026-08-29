#!/usr/bin/env bash
set -euo pipefail

required=(
  obs Xvfb xauth xdpyinfo xwininfo gnome-terminal google-chrome
  node python3 script ffmpeg ffprobe jq sha256sum dbus-run-session
  mcookie setsid ss realpath install
)

missing=0
for tool in "${required[@]}"; do
  if command -v "${tool}" >/dev/null 2>&1; then
    printf 'ok      %-20s %s\n' "${tool}" "$(command -v "${tool}")"
  else
    printf 'missing %-20s\n' "${tool}" >&2
    missing=1
  fi
done
[ "${missing}" -eq 0 ] || exit 1

obs_version="$(obs --version)"
case "${obs_version}" in
  *" 30."*) ;;
  *) echo "fail    expected OBS 30, found: ${obs_version}" >&2; exit 1 ;;
esac

node_major="$(node -p 'Number(process.versions.node.split(".")[0])')"
[ "${node_major}" -ge 22 ] || {
  echo "fail    Node 22+ is required for the built-in WebSocket client" >&2
  exit 1
}

obs_plugin_dir="$(find /usr/lib -path '*/obs-plugins/obs-websocket.so' -print -quit 2>/dev/null)"
[ -n "${obs_plugin_dir}" ] || {
  echo "fail    the bundled obs-websocket plugin was not found" >&2
  exit 1
}

printf 'ok      %-20s %s\n' OBS "${obs_version}"
printf 'ok      %-20s %s\n' obs-websocket "${obs_plugin_dir}"
printf 'ok      %-20s %s\n' X11 "${DISPLAY:-not inherited; harness uses Xvfb}"
python3 -c 'from PIL import Image' || {
  echo 'fail    Python Pillow is required for review-frame preflight' >&2
  exit 1
}
printf 'ok      %-20s %s\n' Pillow "$(python3 -c 'import PIL; print(PIL.__version__)')"
node "$(dirname "${BASH_SOURCE[0]}")/test-obs-websocket-auth.mjs"
node "$(dirname "${BASH_SOURCE[0]}")/test-obs-schedule.mjs"
echo 'KMP OBS capture harness: ready'
