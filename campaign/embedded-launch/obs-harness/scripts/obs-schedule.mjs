#!/usr/bin/env node
"use strict";

import fs from "node:fs";
import path from "node:path";
import { obsWebSocketAuthentication } from "./obs-websocket-auth.mjs";

const [edlPath, scenarioId, durationText, portText, credentialFile, traceFile, runDir] = process.argv.slice(2);
if (!edlPath || !scenarioId || !durationText || !portText || !credentialFile || !traceFile || !runDir) {
  throw new Error("usage: obs-schedule.mjs EDL SCENARIO_ID DURATION_MS PORT PASSWORD_FILE OBS_TRACE RUN_DIR");
}

function stamp() {
  return { wall_time: new Date().toISOString(), monotonic_ns: process.hrtime.bigint().toString() };
}
function append(file, value) {
  fs.appendFileSync(file, `${JSON.stringify({ ...stamp(), ...value })}\n`);
}
async function waitForFile(file, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (fs.existsSync(file)) return JSON.parse(fs.readFileSync(file, "utf8"));
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`timed out waiting for ${file}`);
}
async function waitUntil(targetNs) {
  for (;;) {
    const remaining = targetNs - process.hrtime.bigint();
    if (remaining <= 0n) return;
    await new Promise((resolve) => setTimeout(resolve, Math.min(Number(remaining / 1000000n), 50)));
  }
}

const edl = JSON.parse(fs.readFileSync(edlPath, "utf8"));
const master = edl.masters?.find((item) => item.id === scenarioId);
const schedule = master?.obs_schedule || [{ at_ms: 0, scene: "KMP/Wide" }];
const durationMs = Number(durationText);
if (!Number.isInteger(durationMs) || durationMs <= 0) throw new Error(`missing positive EDL duration for ${scenarioId}`);
if (master && Math.round(Number(master.duration_seconds) * 1000) !== durationMs) {
  throw new Error(`scenario/EDL duration mismatch for ${scenarioId}`);
}
const allowedScenes = new Set(["KMP/Wide", "KMP/TerminalFocus", "KMP/ChronoFocus", "KMP/ProofFocus", "KMP/CTAFocus"]);
for (let index = 0; index < schedule.length; index += 1) {
  const event = schedule[index];
  if (!Number.isInteger(event.at_ms) || event.at_ms < 0 || !allowedScenes.has(event.scene)) {
    throw new Error(`invalid obs_schedule[${index}]: ${JSON.stringify(event)}`);
  }
  if (index > 0 && event.at_ms <= schedule[index - 1].at_ms) throw new Error("obs_schedule must be strictly ordered");
}

const credential = fs.readFileSync(credentialFile, "utf8").trim();
const ws = new WebSocket(`ws://127.0.0.1:${Number(portText)}`);
const pending = new Map();
let requestNumber = 0;

function request(requestType, requestData = {}) {
  const requestId = `kmp-schedule-${++requestNumber}`;
  const packet = { op: 6, d: { requestType, requestId, requestData } };
  append(traceFile, { direction: "scheduler_to_obs", ...packet });
  return new Promise((resolve, reject) => {
    pending.set(requestId, { resolve, reject });
    ws.send(JSON.stringify(packet));
  });
}

const ready = new Promise((resolve, reject) => {
  const timer = setTimeout(() => reject(new Error("OBS scheduler handshake timed out")), 15000);
  ws.addEventListener("message", (event) => {
    const packet = JSON.parse(String(event.data));
    if (packet.op === 0) {
      const authentication = obsWebSocketAuthentication(
        credential,
        packet.d.authentication.salt,
        packet.d.authentication.challenge,
      );
      ws.send(JSON.stringify({ op: 1, d: { rpcVersion: 1, eventSubscriptions: 4, authentication } }));
      append(traceFile, { direction: "scheduler_to_obs", op: 1, authentication: "redacted" });
    } else if (packet.op === 2) {
      clearTimeout(timer);
      resolve();
    } else if (packet.op === 7) {
      append(traceFile, { direction: "obs_to_scheduler", op: 7, response: packet.d });
      const waiter = pending.get(packet.d.requestId);
      if (!waiter) return;
      pending.delete(packet.d.requestId);
      if (packet.d.requestStatus.result) waiter.resolve(packet.d.responseData || {});
      else waiter.reject(new Error(`${packet.d.requestType}: ${packet.d.requestStatus.comment || packet.d.requestStatus.code}`));
    } else if (packet.op === 5) {
      append(traceFile, { direction: "obs_to_scheduler", op: 5, event: packet.d });
    }
  });
  ws.addEventListener("error", () => reject(new Error("OBS scheduler WebSocket failed")));
});
await ready;
const control = path.join(runDir, "control");
const scheduleEvidence = path.join(runDir, "obs-scene-schedule.jsonl");
fs.writeFileSync(path.join(control, "obs-schedule-ready"), `${JSON.stringify(stamp())}\n`, { mode: 0o600 });
const go = await waitForFile(path.join(control, "go"), 60000);
const baseNs = BigInt(go.monotonic_ns);
const startEvent = fs.readFileSync(traceFile, "utf8").trim().split("\n")
  .map((line) => JSON.parse(line))
  .find((row) => row.op === 5
    && row.event?.eventType === "RecordStateChanged"
    && row.event?.eventData?.outputState === "OBS_WEBSOCKET_OUTPUT_STARTED");
if (!startEvent) throw new Error("OBS STARTED event is missing from the evidence trace");
const recordStartNs = BigInt(startEvent.monotonic_ns);
for (const event of schedule) {
  await waitUntil(baseNs + BigInt(event.at_ms) * 1000000n);
  const before = process.hrtime.bigint();
  await request("SetCurrentProgramScene", { sceneName: event.scene });
  const after = process.hrtime.bigint();
  append(scheduleEvidence, {
    scenario_id: scenarioId,
    requested_at_ms: event.at_ms,
    scene: event.scene,
    target_monotonic_ns: (baseNs + BigInt(event.at_ms) * 1000000n).toString(),
    request_monotonic_ns: before.toString(),
    response_monotonic_ns: after.toString(),
    lateness_ms: Number(after - (baseNs + BigInt(event.at_ms) * 1000000n)) / 1e6,
  });
}
// Recording duration is a picture contract, not MCP-client cleanup time.
// The pinned x264 zerolatency profile has no reordered/look-ahead frames, so
// StopRecord is scheduled directly against the observed OBS STARTED clock.
const stopTargetNs = recordStartNs + BigInt(durationMs) * 1000000n;
await waitUntil(stopTargetNs);
const beforeStop = await request("GetRecordStatus");
const stopped = beforeStop.outputActive ? await request("StopRecord") : { outputPath: beforeStop.outputPath };
fs.writeFileSync(path.join(runDir, "obs-stop.json"), `${JSON.stringify({
  ...stamp(),
  command: "scheduled-stop",
  target_monotonic_ns: stopTargetNs.toString(),
  record_start_monotonic_ns: recordStartNs.toString(),
  duration_ms: durationMs,
  encoder_tail_compensation_ms: 0,
  before: beforeStop,
  stopped,
})}\n`);
fs.writeFileSync(path.join(control, "obs-schedule-complete"), `${JSON.stringify(stamp())}\n`, { mode: 0o600 });
ws.close();
await new Promise((resolve) => setTimeout(resolve, 50));
process.exit(0);
