#!/usr/bin/env node
"use strict";

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const [runDir, portText] = process.argv.slice(2);
if (!runDir || !portText) {
  throw new Error("usage: prepare-run.mjs RUN_DIR OBS_WEBSOCKET_PORT");
}
const port = Number(portText);
if (!Number.isInteger(port) || port < 1024 || port > 65535) {
  throw new Error(`invalid OBS WebSocket port: ${portText}`);
}

const obsRoot = path.join(runDir, "obs-config", "obs-studio");
const profile = path.join(obsRoot, "basic", "profiles", "KMPCapture");
const scenes = path.join(obsRoot, "basic", "scenes");
const control = path.join(runDir, "control");
const raw = path.join(runDir, "obs-output");
const chrome = path.join(runDir, "browser-profile.private");
for (const dir of [profile, scenes, control, raw, chrome]) {
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
}
fs.chmodSync(runDir, 0o700);

const password = crypto.randomBytes(24).toString("base64url");
const passwordPath = path.join(control, "obs-password.private");
fs.writeFileSync(passwordPath, `${password}\n`, { mode: 0o600 });
fs.writeFileSync(
  path.join(runDir, "obs-auth.json"),
  `${JSON.stringify({
    port,
    auth_required: true,
    password_sha256: crypto.createHash("sha256").update(password).digest("hex"),
    cleartext_retained: false,
  }, null, 2)}\n`
);

const globalIni = `[General]
Pre19Defaults=false
Pre21Defaults=false
Pre23Defaults=false
Pre24.1Defaults=false
MaxLogs=10
InfoIncrement=-1
ProcessPriority=Normal
EnableAutoUpdates=false
ConfirmOnExit=false
HotkeyFocusType=NeverDisableHotkeys
FirstRun=true

[Video]
Renderer=OpenGL

[BasicWindow]
PreviewEnabled=false
PreviewProgramMode=false
RecordWhenStreaming=false
KeepRecordingWhenStreamStops=false
SysTrayEnabled=false
SaveProjectors=false
ShowStatusBar=true

[Basic]
Profile=KMP Capture
ProfileDir=KMPCapture
SceneCollection=KMP Capture
SceneCollectionFile=KMPCapture
ConfigOnNewProfile=false

[OBSWebSocket]
FirstLoad=false
ServerEnabled=true
ServerPort=${port}
AlertsEnabled=false
AuthRequired=true
ServerPassword=${password}
`;
fs.writeFileSync(path.join(obsRoot, "global.ini"), globalIni);

const basicIni = `[General]
Name=KMP Capture

[Output]
Mode=Advanced
FilenameFormatting=%CCYY-%MM-%DD_%hh-%mm-%ss
OverwriteIfExists=false

[AdvOut]
RecType=Standard
RecFilePath=${raw}
RecFormat2=mkv
RecEncoder=obs_x264
RecAudioEncoder=ffmpeg_aac
RecTracks=1
RecUseRescale=false
FFOutputToFile=true
FFFilePath=${raw}
FFExtension=mkv
FFVEncoderId=27
FFAEncoderId=86018

[Video]
BaseCX=1920
BaseCY=1080
OutputCX=1920
OutputCY=1080
FPSType=0
FPSCommon=30
ScaleType=bicubic
ColorFormat=NV12
ColorSpace=709
ColorRange=Partial

[Audio]
SampleRate=48000
ChannelSetup=Stereo
`;
fs.writeFileSync(path.join(profile, "basic.ini"), basicIni);

// Pin a true low-latency software encoder. OBS otherwise inherits x264's
// reorder/look-ahead defaults, making final PTS drift after StopRecord.
const recordEncoder = {
  rate_control: "CBR",
  bitrate: 2500,
  use_bufsize: true,
  buffer_size: 2500,
  keyint_sec: 0,
  crf: 23,
  preset: "veryfast",
  profile: "",
  tune: "zerolatency",
  x264opts: "bframes=0 rc-lookahead=0 sync-lookahead=0",
  bf: 0,
};
fs.writeFileSync(path.join(profile, "recordEncoder.json"), `${JSON.stringify(recordEncoder)}\n`);

const sceneUuid = crypto.randomUUID();
const scene = {
  current_scene: "KMP Capture",
  current_program_scene: "KMP Capture",
  scene_order: [{ name: "KMP Capture" }],
  name: "KMP Capture",
  sources: [
    {
      prev_ver: 503316482,
      name: "KMP Capture",
      uuid: sceneUuid,
      id: "scene",
      versioned_id: "scene",
      settings: { id_counter: 0, custom_size: false, items: [] },
      mixers: 0,
      sync: 0,
      flags: 0,
      volume: 1.0,
      balance: 0.5,
      enabled: true,
      muted: false,
      "push-to-mute": false,
      "push-to-mute-delay": 0,
      "push-to-talk": false,
      "push-to-talk-delay": 0,
      hotkeys: { "OBSBasic.SelectScene": [] },
      deinterlace_mode: 0,
      deinterlace_field_order: 0,
      monitoring_type: 0,
      private_settings: {},
    },
  ],
  groups: [],
  quick_transitions: [
    { name: "Cut", duration: 0, hotkeys: [], id: 1, fade_to_black: false },
  ],
  transitions: [],
  saved_projectors: [],
  current_transition: "Cut",
  transition_duration: 0,
  preview_locked: true,
  scaling_enabled: false,
  scaling_level: 0,
  scaling_off_x: 0.0,
  scaling_off_y: 0.0,
  modules: { "scripts-tool": [] },
};
fs.writeFileSync(path.join(scenes, "KMPCapture.json"), `${JSON.stringify(scene)}\n`);

console.log(JSON.stringify({ obsRoot, profile, scenes, control, raw, chrome, port, recordEncoder }));
