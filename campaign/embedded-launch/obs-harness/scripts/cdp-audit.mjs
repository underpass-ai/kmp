#!/usr/bin/env node
"use strict";

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const [portText, viewerUrlFile, runDir] = process.argv.slice(2);
if (!portText || !viewerUrlFile || !runDir) {
  throw new Error("usage: cdp-audit.mjs DEBUG_PORT VIEWER_URL_FILE RUN_DIR");
}
const port = Number(portText);
const networkFile = path.join(runDir, "browser-network.jsonl");
const revisionsFile = path.join(runDir, "viewer-revisions.jsonl");
const control = path.join(runDir, "control");

function stamp() {
  return { wall_time: new Date().toISOString(), monotonic_ns: process.hrtime.bigint().toString() };
}

function append(file, value) {
  fs.appendFileSync(file, `${JSON.stringify({ ...stamp(), ...value })}\n`);
}

function sha256(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

function sanitizeUrl(raw) {
  const url = new URL(raw);
  const redactions = [];
  if (url.searchParams.has("k")) {
    const value = url.searchParams.get("k");
    redactions.push({ parameter: "k", reason: "viewer_capability", sha256: sha256(value) });
    url.searchParams.set("k", `<redacted:sha256:${sha256(value)}>`);
  }
  return { url: url.toString(), redactions };
}

async function fetchJson(relative, timeoutMs = 20000) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}${relative}`);
      if (response.ok) return await response.json();
      last = new Error(`HTTP ${response.status}`);
    } catch (error) { last = error; }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Chrome DevTools endpoint did not become ready: ${last}`);
}

let viewerUrl = fs.readFileSync(viewerUrlFile, "utf8").trim();
const targets = await fetchJson("/json/list");
const target = targets.find((item) => item.type === "page");
if (!target) throw new Error("Chrome DevTools has no page target");

const ws = new WebSocket(target.webSocketDebuggerUrl);
const pending = new Map();
const requests = new Map();
let commandId = 0;
let stopping = false;

function command(method, params = {}) {
  const id = ++commandId;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });
}

const opened = new Promise((resolve, reject) => {
  ws.addEventListener("open", resolve, { once: true });
  ws.addEventListener("error", () => reject(new Error("CDP WebSocket failed")), { once: true });
});

ws.addEventListener("message", async (event) => {
  const packet = JSON.parse(String(event.data));
  if (packet.id) {
    const waiter = pending.get(packet.id);
    if (!waiter) return;
    pending.delete(packet.id);
    if (packet.error) waiter.reject(new Error(JSON.stringify(packet.error)));
    else waiter.resolve(packet.result || {});
    return;
  }
  if (packet.method === "Network.requestWillBeSent") {
    const rawUrl = packet.params.request.url;
    let parsed;
    try { parsed = new URL(rawUrl); } catch (_) { return; }
    if (parsed.hostname !== "127.0.0.1") return;
    const safe = sanitizeUrl(rawUrl);
    const record = {
      request_id: packet.params.requestId,
      method: packet.params.request.method,
      origin: parsed.origin,
      path: parsed.pathname,
      query: Object.fromEntries(parsed.searchParams.entries()),
      sanitized_url: safe.url,
      redactions: safe.redactions,
      request_monotonic_time: packet.params.timestamp,
    };
    if (record.query.k) record.query.k = `<redacted:sha256:${safe.redactions[0]?.sha256}>`;
    requests.set(packet.params.requestId, record);
    append(networkFile, { phase: "request", ...record });
  } else if (packet.method === "Network.responseReceived") {
    const record = requests.get(packet.params.requestId);
    if (!record) return;
    record.status = packet.params.response.status;
    record.mime_type = packet.params.response.mimeType;
    record.response_monotonic_time = packet.params.timestamp;
    append(networkFile, { phase: "response", ...record });
  } else if (packet.method === "Network.loadingFinished") {
    const record = requests.get(packet.params.requestId);
    if (!record || record.path !== "/api/view") return;
    try {
      const bodyResult = await command("Network.getResponseBody", { requestId: packet.params.requestId });
      const body = bodyResult.base64Encoded
        ? Buffer.from(bodyResult.body, "base64").toString("utf8")
        : bodyResult.body;
      const state = JSON.parse(body);
      append(revisionsFile, {
        request_id: packet.params.requestId,
        view_revision: state.view_revision,
        actor: state.last_change?.actor || null,
        explanation: state.last_change?.explanation || null,
        about: state.about || null,
        origin: record.origin,
        long_poll: Object.hasOwn(record.query, "since"),
        since: record.query.since === undefined ? null : Number(record.query.since),
        encoded_bytes: packet.params.encodedDataLength,
        body_sha256: sha256(body),
      });
      const explanation = state.last_change?.explanation || "";
      if (explanation === "light up the proof path" || explanation.startsWith("proof hop ")) {
        await new Promise((resolve) => setTimeout(resolve, 500));
        const hop = /^proof hop (\d+)/.exec(explanation)?.[1];
        const selector = hop ? `#trace-hops > li:nth-child(${hop})` : "#trace-box";
        const scroll = await command("Runtime.evaluate", {
          expression: `document.querySelector(${JSON.stringify(selector)})?.scrollIntoView({block:'start',behavior:'instant'})`,
          returnByValue: true,
        });
        append(networkFile, {
          phase: "viewer_ui_action",
          action: "scroll_real_chronoloom_audit_path_into_view",
          selector,
          trigger_view_revision: state.view_revision,
          trigger_body_sha256: sha256(body),
          runtime_result_type: scroll.result?.type || null,
        });
      }
    } catch (error) {
      append(revisionsFile, {
        request_id: packet.params.requestId,
        observation_error: error.message,
        long_poll: Object.hasOwn(record.query, "since"),
      });
    }
  }
});

await opened;
await command("Network.enable");
await command("Page.enable");
await command("Page.navigate", { url: viewerUrl });
append(networkFile, { phase: "viewer_navigation", ...sanitizeUrl(viewerUrl) });
await new Promise((resolve) => setTimeout(resolve, 800));
fs.writeFileSync(path.join(control, "cdp-ready"), `${JSON.stringify(stamp())}\n`, { mode: 0o600 });

while (!stopping) {
  if (fs.existsSync(path.join(control, "cdp-stop"))) {
    stopping = true;
  } else {
    const candidate = fs.readFileSync(viewerUrlFile, "utf8").trim();
    if (candidate && candidate !== viewerUrl) {
      const previous = sanitizeUrl(viewerUrl);
      viewerUrl = candidate;
      await command("Page.navigate", { url: viewerUrl });
      append(networkFile, {
        phase: "viewer_navigation",
        previous_url: previous.url,
        previous_redactions: previous.redactions,
        ...sanitizeUrl(viewerUrl),
      });
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}
ws.close();
