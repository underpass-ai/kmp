#!/usr/bin/env node
"use strict";

import crypto from "node:crypto";
import fs from "node:fs";

function now() {
  return { wall_time: new Date().toISOString(), monotonic_ns: process.hrtime.bigint().toString() };
}

function appendJsonl(file, value) {
  fs.appendFileSync(file, `${JSON.stringify({ ...now(), ...value })}\n`);
}

class ObsConnection {
  constructor({ port, password, trace }) {
    this.port = port;
    this.password = password;
    this.trace = trace;
    this.pending = new Map();
    this.requestNumber = 0;
  }

  async open() {
    this.ws = new WebSocket(`ws://127.0.0.1:${this.port}`);
    const ready = new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("OBS WebSocket handshake timed out")), 15000);
      this.ws.addEventListener("error", (event) => reject(new Error(`OBS WebSocket error: ${event.message || "unknown"}`)));
      this.ws.addEventListener("message", (event) => {
        const packet = JSON.parse(String(event.data));
        if (packet.op === 0) {
          const identify = { rpcVersion: 1, eventSubscriptions: 64 };
          if (packet.d.authentication) {
            const secret = crypto
              .createHash("sha256")
              .update(this.password + packet.d.authentication.salt)
              .digest("base64");
            identify.authentication = crypto
              .createHash("sha256")
              .update(secret + packet.d.authentication.challenge)
              .digest("base64");
          }
          this.ws.send(JSON.stringify({ op: 1, d: identify }));
          appendJsonl(this.trace, { direction: "client_to_obs", op: 1, authentication: "redacted" });
        } else if (packet.op === 2) {
          clearTimeout(timer);
          appendJsonl(this.trace, { direction: "obs_to_client", op: 2, negotiated_rpc_version: packet.d.negotiatedRpcVersion });
          resolve();
        } else if (packet.op === 5) {
          appendJsonl(this.trace, { direction: "obs_to_client", op: 5, event: packet.d });
        } else if (packet.op === 7) {
          appendJsonl(this.trace, { direction: "obs_to_client", op: 7, response: packet.d });
          const waiter = this.pending.get(packet.d.requestId);
          if (waiter) {
            this.pending.delete(packet.d.requestId);
            if (packet.d.requestStatus.result) waiter.resolve(packet.d.responseData || {});
            else waiter.reject(new Error(`${packet.d.requestType}: ${packet.d.requestStatus.code} ${packet.d.requestStatus.comment || ""}`));
          }
        }
      });
    });
    await ready;
  }

  request(requestType, requestData = {}) {
    const requestId = `kmp-capture-${++this.requestNumber}`;
    const packet = { op: 6, d: { requestType, requestId, requestData } };
    appendJsonl(this.trace, { direction: "client_to_obs", ...packet });
    return new Promise((resolve, reject) => {
      this.pending.set(requestId, { resolve, reject });
      this.ws.send(JSON.stringify(packet));
    });
  }

  close() {
    this.ws.close();
  }
}

const sceneDefinitions = [
  {
    name: "KMP/Wide",
    sources: [{ role: "wide", crop: [0, 0, 1920, 1080], target: [0, 0, 1920, 1080] }],
  },
  {
    name: "KMP/TerminalFocus",
    sources: [
      { role: "primary-terminal", crop: [0, 160, 672, 378], target: [0, 0, 1920, 1080] },
      { role: "secondary-chronoloom", crop: [672, 189, 1248, 702], target: [1390, 748, 500, 281] },
    ],
  },
  {
    name: "KMP/ChronoFocus",
    sources: [
      { role: "primary-chronoloom", crop: [1350, 160, 570, 321], target: [0, 0, 1920, 1080] },
      { role: "secondary-terminal", crop: [0, 160, 672, 378], target: [40, 748, 520, 292] },
    ],
  },
  {
    name: "KMP/ProofFocus",
    sources: [
      { role: "primary-proof", crop: [1350, 430, 570, 321], target: [0, 0, 1920, 1080] },
      { role: "secondary-terminal", crop: [0, 160, 672, 378], target: [40, 748, 520, 292] },
    ],
  },
  {
    name: "KMP/CTAFocus",
    sources: [
      { role: "primary-cta", crop: [0, 160, 672, 378], target: [0, 0, 1920, 1080] },
      { role: "secondary-chronoloom", crop: [672, 189, 1248, 702], target: [1390, 748, 500, 281] },
    ],
  },
];

async function connectWithRetry(options) {
  const deadline = Date.now() + 30000;
  let last;
  while (Date.now() < deadline) {
    const connection = new ObsConnection(options);
    try {
      await connection.open();
      return connection;
    } catch (error) {
      last = error;
      try { connection.close(); } catch (_) {}
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }
  throw new Error(`OBS WebSocket did not become ready: ${last}`);
}

const [command, portText, passwordFile, traceFile] = process.argv.slice(2);
if (!command || !portText || !passwordFile || !traceFile) {
  throw new Error("usage: obs-control.mjs arm|stop|status PORT PASSWORD_FILE TRACE_JSONL");
}
const password = fs.readFileSync(passwordFile, "utf8").trim();
const obs = await connectWithRetry({ port: Number(portText), password, trace: traceFile });
async function requestWhenObsReady(requestType, requestData = {}) {
  const deadline = Date.now() + 30000;
  let last;
  while (Date.now() < deadline) {
    try {
      return await obs.request(requestType, requestData);
    } catch (error) {
      last = error;
      if (!error.message.includes("OBS is not ready")) throw error;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  throw new Error(`OBS core did not become ready: ${last}`);
}
try {
  if (command === "arm") {
    const version = await requestWhenObsReady("GetVersion");
    const video = await obs.request("GetVideoSettings");
    const recordDirectory = await obs.request("GetRecordDirectory");
    const outputMode = await obs.request("GetProfileParameter", { parameterCategory: "Output", parameterName: "Mode" });
    const recordPath = await obs.request("GetProfileParameter", { parameterCategory: "AdvOut", parameterName: "RecFilePath" });
    const recordEncoder = await obs.request("GetProfileParameter", { parameterCategory: "AdvOut", parameterName: "RecEncoder" });
    const scenes = await obs.request("GetSceneList");
    const existingScenes = new Set(scenes.scenes.map((scene) => scene.sceneName));
    for (const definition of sceneDefinitions) {
      if (!existingScenes.has(definition.name)) await obs.request("CreateScene", { sceneName: definition.name });
    }
    const inputs = await obs.request("GetInputList");
    const defaults = await obs.request("GetInputDefaultSettings", { inputKind: "xshm_input" });
    const createdInputs = [];
    for (const definition of sceneDefinitions) {
      for (const source of definition.sources) {
        const inputName = `${definition.name} · ${source.role} · isolated X11 screen 0`;
        const prior = inputs.inputs.find((input) => input.inputName === inputName);
        if (prior) await obs.request("RemoveInput", { inputName });
        await obs.request("CreateInput", {
          sceneName: definition.name,
          inputName,
          inputKind: "xshm_input",
          inputSettings: { screen: 0, show_cursor: false },
          sceneItemEnabled: true,
        });
        const created = await obs.request("GetInputSettings", { inputName });
        const item = await obs.request("GetSceneItemId", { sceneName: definition.name, sourceName: inputName });
        const [cropX, cropY, cropWidth, cropHeight] = source.crop;
        const [targetX, targetY, targetWidth, targetHeight] = source.target;
        await obs.request("SetSceneItemTransform", {
          sceneName: definition.name,
          sceneItemId: item.sceneItemId,
          sceneItemTransform: {
            positionX: targetX,
            positionY: targetY,
            rotation: 0,
            scaleX: targetWidth / cropWidth,
            scaleY: targetHeight / cropHeight,
            cropLeft: cropX,
            cropTop: cropY,
            cropRight: 1920 - cropX - cropWidth,
            cropBottom: 1080 - cropY - cropHeight,
            alignment: 5,
            boundsType: "OBS_BOUNDS_NONE",
          },
        });
        createdInputs.push({
          scene: definition.name,
          role: source.role,
          input_name: inputName,
          crop: source.crop,
          target: source.target,
          input: created,
        });
      }
    }
    await obs.request("SetCurrentProgramScene", { sceneName: "KMP/Wide" });
    await obs.request("StartRecord");
    let status;
    for (let attempt = 0; attempt < 40; attempt += 1) {
      status = await obs.request("GetRecordStatus");
      if (status.outputActive) break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    if (!status?.outputActive) throw new Error("OBS recording did not enter active state");
    console.log(JSON.stringify({
      ...now(),
      command,
      version,
      video,
      record_directory: recordDirectory,
      output_mode: outputMode,
      record_path: recordPath,
      record_encoder: recordEncoder,
      scene_name: "KMP/Wide",
      scenes: sceneDefinitions,
      input_defaults: defaults,
      inputs: createdInputs,
      recording: status,
    }));
  } else if (command === "stop") {
    const status = await obs.request("GetRecordStatus");
    const stopped = status.outputActive ? await obs.request("StopRecord") : { outputPath: status.outputPath };
    console.log(JSON.stringify({ ...now(), command, before: status, stopped }));
  } else if (command === "status") {
    console.log(JSON.stringify({
      ...now(),
      command,
      version: await obs.request("GetVersion"),
      video: await obs.request("GetVideoSettings"),
      recording: await obs.request("GetRecordStatus"),
      scenes: await obs.request("GetSceneList"),
      inputs: await obs.request("GetInputList"),
    }));
  } else {
    throw new Error(`unknown command: ${command}`);
  }
} finally {
  obs.close();
}
await new Promise((resolve) => setTimeout(resolve, 50));
process.exit(0);
