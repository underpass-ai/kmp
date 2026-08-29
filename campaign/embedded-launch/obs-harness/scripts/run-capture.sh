#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/../../../.." && pwd)"

promote=0
if [ "${1:-}" = "--promote" ]; then
  promote=1
  shift
fi
if [ "$#" -ne 1 ]; then
  echo 'usage: run-capture.sh [--promote] SCENARIO.json' >&2
  exit 2
fi

scenario="$(realpath "$1")"
python3 "${SCRIPT_DIR}/validate-scenario.py" "${scenario}"
scenario_id="$(jq -r '.id' "${scenario}")"
duration_ms="$(jq -r '.duration_ms' "${scenario}")"
process_count="$(jq -r '(.processes // [{id:"process"}]) | length' "${scenario}")"
edl="${KMP_CAPTURE_EDL:-${ROOT}/campaign/embedded-launch/edl.json}"
[ -f "${edl}" ] || { echo "run-capture: EDL does not exist: ${edl}" >&2; exit 1; }
audio_contract="${ROOT}/campaign/embedded-launch/audio/contract.json"
[ -f "${audio_contract}" ] || { echo "run-capture: audio contract does not exist: ${audio_contract}" >&2; exit 1; }

binary="${KMP_MCP_BIN:-${ROOT}/target/debug/kmp-mcp}"
if [ ! -x "${binary}" ]; then
  echo "run-capture: build kmp-mcp or set KMP_MCP_BIN; not executable: ${binary}" >&2
  exit 1
fi

evidence_root="${ROOT}/campaign/embedded-launch/evidence-pack/capture"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-${scenario_id}"
run_dir="${evidence_root}/runs/${scenario_id}/${run_id}"
mkdir -p "${run_dir}"
chmod 700 "${run_dir}"

display_number=""
viewer_ports_available() {
  local base="$1"
  local offset
  for offset in $(seq 0 $((process_count - 1))); do
    if ss -H -ltn "sport = :$((base + offset))" | grep -q .; then return 1; fi
  done
  return 0
}
for candidate in $(seq 120 180); do
  obs_candidate=$((45000 + candidate))
  viewer_candidate=$((17000 + candidate))
  cdp_candidate=$((9200 + candidate))
  if [ ! -S "/tmp/.X11-unix/X${candidate}" ] \
    && ! ss -H -ltn "sport = :${obs_candidate}" | grep -q . \
    && viewer_ports_available "${viewer_candidate}" \
    && ! ss -H -ltn "sport = :${cdp_candidate}" | grep -q .; then
    display_number="${candidate}"
    obs_port="${obs_candidate}"
    viewer_port="${viewer_candidate}"
    cdp_port="${cdp_candidate}"
    break
  fi
done
[ -n "${display_number}" ] || { echo 'run-capture: no isolated X11/port tuple available' >&2; exit 1; }

node "${SCRIPT_DIR}/prepare-run.mjs" "${run_dir}" "${obs_port}" >"${run_dir}/prepare-run.json"

xauth_file="${run_dir}/xauthority.private"
xauth_cookie="$(mcookie)"
xauth -f "${xauth_file}" add ":${display_number}" . "${xauth_cookie}"
chmod 600 "${xauth_file}"

xvfb_pid=""
terminal_pid=""
chrome_pid=""
cdp_pid=""
obs_pid=""
schedule_pid=""

stop_group() {
  local pid="${1:-}"
  if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
    kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
  fi
}

cleanup() {
  set +e
  stop_group "${cdp_pid}"
  stop_group "${schedule_pid}"
  stop_group "${chrome_pid}"
  stop_group "${terminal_pid}"
  stop_group "${obs_pid}"
  if [ -n "${xvfb_pid}" ]; then kill "${xvfb_pid}" 2>/dev/null || true; fi
  wait "${cdp_pid}" 2>/dev/null || true
  wait "${schedule_pid}" 2>/dev/null || true
  wait "${chrome_pid}" 2>/dev/null || true
  wait "${terminal_pid}" 2>/dev/null || true
  wait "${obs_pid}" 2>/dev/null || true
  wait "${xvfb_pid}" 2>/dev/null || true
  case "${run_dir}" in
    "${evidence_root}"/runs/*)
      node "${SCRIPT_DIR}/sanitize-run.mjs" "${run_dir}" || true
      ;;
  esac
}
trap cleanup EXIT

Xvfb ":${display_number}" \
  -screen 0 1920x1080x24 \
  -screen 1 1280x720x24 \
  -nolisten tcp \
  -auth "${xauth_file}" \
  >"${run_dir}/xvfb.log" 2>&1 &
xvfb_pid=$!
for _ in $(seq 1 100); do
  [ -S "/tmp/.X11-unix/X${display_number}" ] && break
  sleep 0.05
done
[ -S "/tmp/.X11-unix/X${display_number}" ] || { echo 'run-capture: Xvfb failed to start' >&2; exit 1; }

DISPLAY=":${display_number}.0" XAUTHORITY="${xauth_file}" xdpyinfo >"${run_dir}/xdpyinfo-screen0.txt"
DISPLAY=":${display_number}.1" XAUTHORITY="${xauth_file}" xdpyinfo >"${run_dir}/xdpyinfo-screen1.txt"
runtime_json="$(jq -cn \
  --arg scenario_id "${scenario_id}" \
  --arg run_id "${run_id}" \
  --arg display ":${display_number}" \
  --argjson obs_port "${obs_port}" \
  --argjson viewer_port "${viewer_port}" \
  --argjson cdp_port "${cdp_port}" \
  '{scenario_id:$scenario_id,run_id:$run_id,x11:{server:$display,capture_screen:0,obs_screen:1,tcp_listener:false},ports:{obs_websocket:$obs_port,viewer:$viewer_port,chrome_devtools:$cdp_port}}')"
node "${SCRIPT_DIR}/signal.mjs" "${run_dir}/runtime.json" "${runtime_json}"
install -m 0644 "${edl}" "${run_dir}/edl.json"
sha256sum "${edl}" >"${run_dir}/edl.sha256"
install -m 0644 "${audio_contract}" "${run_dir}/audio-contract.json"

setsid env \
  DISPLAY=":${display_number}.0" \
  XAUTHORITY="${xauth_file}" \
  GSETTINGS_BACKEND=memory \
  GTK_USE_PORTAL=0 \
  GIO_USE_VFS=local \
  NO_AT_BRIDGE=1 \
  dbus-run-session -- \
  gnome-terminal --disable-factory --wait \
    --title=KMP_CAPTURE_TERMINAL \
    --geometry=32x26+0+0 \
    --zoom=2.0 -- \
    "${SCRIPT_DIR}/terminal-entry.sh" \
    "${scenario}" "${run_dir}" "${binary}" "${ROOT}" "${viewer_port}" \
  >"${run_dir}/terminal.stdout.log" 2>"${run_dir}/terminal.stderr.log" &
terminal_pid=$!

client_deadline=$((SECONDS + 30))
while [ ! -f "${run_dir}/control/client-ready" ]; do
  if [ -f "${run_dir}/control/client-failed" ] || [ "${SECONDS}" -ge "${client_deadline}" ]; then
    echo "run-capture: MCP PTY client did not become ready" >&2
    [ -f "${run_dir}/control/client-failed" ] && sed -n '1,200p' "${run_dir}/control/client-failed" >&2
    exit 1
  fi
  sleep 0.1
done

setsid env \
  DISPLAY=":${display_number}.0" \
  XAUTHORITY="${xauth_file}" \
  google-chrome \
    --user-data-dir="${run_dir}/browser-profile.private" \
    --remote-debugging-port="${cdp_port}" \
    --remote-allow-origins='*' \
    --no-first-run \
    --no-default-browser-check \
    --disable-sync \
    --disable-extensions \
    --disable-component-update \
    --disable-background-networking \
    --disable-breakpad \
    --disable-features=MediaRouter,Translate,OptimizationGuideModelDownloading \
    --force-device-scale-factor=1 \
    --window-position=672,0 \
    --window-size=1248,1080 \
    --app=about:blank \
  >"${run_dir}/chrome.stdout.log" 2>"${run_dir}/chrome.stderr.log" &
chrome_pid=$!

setsid node "${SCRIPT_DIR}/cdp-audit.mjs" \
  "${cdp_port}" "${run_dir}/control/viewer-url.private" "${run_dir}" \
  >"${run_dir}/cdp.stdout.log" 2>"${run_dir}/cdp.stderr.log" &
cdp_pid=$!

cdp_deadline=$((SECONDS + 30))
while [ ! -f "${run_dir}/control/cdp-ready" ]; do
  if ! kill -0 "${cdp_pid}" 2>/dev/null || [ "${SECONDS}" -ge "${cdp_deadline}" ]; then
    echo 'run-capture: Chromium/CDP did not become ready' >&2
    sed -n '1,200p' "${run_dir}/cdp.stderr.log" >&2
    exit 1
  fi
  sleep 0.1
done

sleep 1
DISPLAY=":${display_number}.0" XAUTHORITY="${xauth_file}" xwininfo -root -tree >"${run_dir}/window-tree.txt"

setsid env \
  DISPLAY=":${display_number}.1" \
  XAUTHORITY="${xauth_file}" \
  XDG_CONFIG_HOME="${run_dir}/obs-config" \
  XDG_CACHE_HOME="${run_dir}/obs-cache.private" \
  obs --multi \
    --collection 'KMP Capture' \
    --profile 'KMP Capture' \
    --scene 'KMP Capture' \
    --websocket_port "${obs_port}" \
    --disable-shutdown-check \
    --disable-missing-files-check \
    --only-bundled-plugins \
  >"${run_dir}/obs.stdout.log" 2>"${run_dir}/obs.stderr.log" &
obs_pid=$!

if ! node "${SCRIPT_DIR}/obs-control.mjs" arm \
  "${obs_port}" "${run_dir}/control/obs-password.private" "${run_dir}/obs-websocket.jsonl" \
  >"${run_dir}/obs-arm.json" 2>"${run_dir}/obs-arm.stderr.log"; then
  XAUTHORITY="${xauth_file}" ffmpeg -y -hide_banner -loglevel error \
    -f x11grab -video_size 1280x720 -i ":${display_number}.1+0,0" \
    -frames:v 1 "${run_dir}/obs-control-screen.png" || true
  XAUTHORITY="${xauth_file}" ffmpeg -y -hide_banner -loglevel error \
    -f x11grab -video_size 1920x1080 -i ":${display_number}.0+0,0" \
    -frames:v 1 "${run_dir}/capture-screen.png" || true
  echo 'run-capture: OBS failed to arm; retained sanitized control/capture screenshots' >&2
  sed -n '1,200p' "${run_dir}/obs-arm.stderr.log" >&2
  exit 1
fi

setsid node "${SCRIPT_DIR}/obs-schedule.mjs" \
  "${edl}" "${scenario_id}" "${duration_ms}" "${obs_port}" \
  "${run_dir}/control/obs-password.private" "${run_dir}/obs-websocket.jsonl" "${run_dir}" \
  >"${run_dir}/obs-schedule.stdout.log" 2>"${run_dir}/obs-schedule.stderr.log" &
schedule_pid=$!

schedule_deadline=$((SECONDS + 20))
while [ ! -f "${run_dir}/control/obs-schedule-ready" ]; do
  if ! kill -0 "${schedule_pid}" 2>/dev/null || [ "${SECONDS}" -ge "${schedule_deadline}" ]; then
    echo 'run-capture: OBS scene scheduler did not become ready' >&2
    sed -n '1,200p' "${run_dir}/obs-schedule.stderr.log" >&2
    exit 1
  fi
  sleep 0.05
done

node "${SCRIPT_DIR}/signal.mjs" "${run_dir}/control/go" "$(jq -c '{obs_arm_monotonic_ns:.monotonic_ns}' "${run_dir}/obs-arm.json")"

scenario_deadline=$((SECONDS + duration_ms / 1000 + 45))
while [ ! -f "${run_dir}/control/scenario-complete" ]; do
  if [ -f "${run_dir}/control/client-failed" ]; then
    echo 'run-capture: scenario client failed' >&2
    sed -n '1,240p' "${run_dir}/control/client-failed" >&2
    exit 1
  fi
  if [ "${SECONDS}" -ge "${scenario_deadline}" ]; then
    echo 'run-capture: scenario timed out' >&2
    exit 1
  fi
  sleep 0.1
done

if ! wait "${schedule_pid}"; then
  echo 'run-capture: OBS scene scheduler failed' >&2
  sed -n '1,200p' "${run_dir}/obs-schedule.stderr.log" >&2
  exit 1
fi
schedule_pid=""

node "${SCRIPT_DIR}/signal.mjs" "${run_dir}/control/stop"
node "${SCRIPT_DIR}/signal.mjs" "${run_dir}/control/cdp-stop"

wait "${cdp_pid}"
cdp_pid=""
wait "${terminal_pid}"
terminal_pid=""

output_path="$(jq -r '.stopped.outputPath // empty' "${run_dir}/obs-stop.json")"
if [ -z "${output_path}" ] || [ ! -f "${output_path}" ]; then
  echo "run-capture: OBS did not return an existing recording path: ${output_path}" >&2
  exit 1
fi
mv -- "${output_path}" "${run_dir}/obs-recording.mkv"

ffprobe -v error -show_format -show_streams -of json \
  "${run_dir}/obs-recording.mkv" >"${run_dir}/ffprobe.json"
python3 "${SCRIPT_DIR}/build-anchors.py" "${run_dir}" "${scenario}"
python3 "${SCRIPT_DIR}/prepare-review.py" "${run_dir}" "${scenario}" "${run_dir}/edl.json"

stop_group "${obs_pid}"
wait "${obs_pid}" 2>/dev/null || true
obs_pid=""
stop_group "${chrome_pid}"
wait "${chrome_pid}" 2>/dev/null || true
chrome_pid=""
if [ -n "${xvfb_pid}" ]; then
  kill "${xvfb_pid}" 2>/dev/null || true
  wait "${xvfb_pid}" 2>/dev/null || true
  xvfb_pid=""
fi

case "${run_dir}" in
  "${evidence_root}"/runs/*)
    node "${SCRIPT_DIR}/sanitize-run.mjs" "${run_dir}"
    ;;
  *) echo "run-capture: refusing private-data cleanup outside evidence runs: ${run_dir}" >&2; exit 1 ;;
esac

python3 "${SCRIPT_DIR}/verify-run.py" "${run_dir}" "${scenario}" "${run_dir}/edl.json"

if [ "${promote}" -eq 1 ]; then
  mkdir -p "${evidence_root}/raw"
  install -m 0644 "${run_dir}/obs-recording.mkv" "${evidence_root}/raw/${scenario_id}.mkv"
  raw_relative="campaign/embedded-launch/evidence-pack/capture/raw/${scenario_id}.mkv"
  (cd "${ROOT}" && sha256sum "${raw_relative}") >"${evidence_root}/raw/${scenario_id}.mkv.sha256"
  python3 "${SCRIPT_DIR}/promote-run.py" \
    "${run_dir}" \
    "${evidence_root}/raw/${scenario_id}.mkv" \
    "${evidence_root}/promoted/${scenario_id}.json"
  echo "promoted: ${evidence_root}/raw/${scenario_id}.mkv"
fi

echo "run: ${run_dir}"
