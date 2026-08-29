#!/usr/bin/env node
"use strict";

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";
import { spawn, spawnSync } from "node:child_process";

import {
  consumeTerminalRows,
  guardsTerminalRows,
  opensSemanticViewport,
  resetSemanticViewport,
  sceneAt,
} from "./terminal-presentation-contract.mjs";

const [scenarioPath, runDir, binary, repoRoot] = process.argv.slice(2);
if (!scenarioPath || !runDir || !binary || !repoRoot) {
  throw new Error("usage: mcp-pty-client.mjs SCENARIO RUN_DIR KMP_MCP_BIN REPO_ROOT");
}

const scenario = JSON.parse(fs.readFileSync(scenarioPath, "utf8"));
const capturedEdl = JSON.parse(fs.readFileSync(path.join(runDir, "edl.json"), "utf8"));
const master = capturedEdl.masters?.find((item) => item.id === scenario.id);
const obsSchedule = master?.obs_schedule || [{ at_ms: 0, scene: "KMP/Wide" }];
const controlDir = path.join(runDir, "control");
const wireFile = path.join(runDir, "tool-calls.jsonl");
const terminalFile = path.join(runDir, "terminal-events.jsonl");
const lifecycleFile = path.join(runDir, "process-lifecycle.json");
const storeFile = path.join(runDir, "stores.json");
const viewerHistoryFile = path.join(runDir, "viewer-history.jsonl");
const viewerFile = path.join(runDir, "viewer.json");
const baseViewerPort = Number(process.env.KMP_CAPTURE_VIEWER_PORT);
if (!Number.isInteger(baseViewerPort)) throw new Error("KMP_CAPTURE_VIEWER_PORT must be an integer");

function stamp() {
  return { wall_time: new Date().toISOString(), monotonic_ns: process.hrtime.bigint().toString() };
}

function sha256(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

function appendJsonl(file, value) {
  fs.appendFileSync(file, `${JSON.stringify({ ...stamp(), ...value })}\n`);
}

function sanitized(value, redactions = [], pointer = "$") {
  if (typeof value === "string") {
    return value.replace(/([?&]k=)([0-9a-f]{64})/gi, (_, prefix, capability) => {
      redactions.push({ path: pointer, reason: "viewer_capability", sha256: sha256(capability) });
      return `${prefix}<redacted:sha256:${sha256(capability)}>`;
    });
  }
  if (Array.isArray(value)) return value.map((item, index) => sanitized(item, redactions, `${pointer}[${index}]`));
  if (value && typeof value === "object") {
    const output = {};
    for (const [key, child] of Object.entries(value)) {
      const childPath = `${pointer}.${key}`;
      if (/(?:authorization|cookie|password|secret|token|api[_-]?key)$/i.test(key)) {
        const wire = typeof child === "string" ? child : JSON.stringify(child);
        redactions.push({ path: childPath, reason: "secret_shaped_key", sha256: sha256(wire) });
        output[key] = `<redacted:sha256:${sha256(wire)}>`;
      } else {
        output[key] = sanitized(child, redactions, childPath);
      }
    }
    return output;
  }
  return value;
}

function logWire(processId, direction, line) {
  let parsed;
  try { parsed = JSON.parse(line); } catch (_) { parsed = { unparsable_line: true }; }
  const redactions = [];
  const message = sanitized(parsed, redactions);
  appendJsonl(wireFile, {
    process_id: processId,
    direction,
    wire_sha256: sha256(line),
    exact_payload: redactions.length === 0,
    redactions,
    message,
  });
}

let semanticViewportActive = false;
let semanticRowsUsed = 0;
let presentationScene = "KMP/Wide";
let presentationAtMs = 0;

function terminal(speaker, text, style = "") {
  const palettes = {
    user: "\x1b[38;5;117m",
    process: "\x1b[38;5;151m",
    kmp: "\x1b[38;5;183m",
    tool: "\x1b[38;5;147m",
    neutral: "\x1b[38;5;252m",
  };
  const color = palettes[style || speaker] || palettes.neutral;
  const prefix = speaker ? `${speaker.toUpperCase()}  ` : "";
  const guarded = semanticViewportActive && guardsTerminalRows(presentationScene);
  const viewport = guarded
    ? consumeTerminalRows(semanticRowsUsed, speaker, text)
    : null;
  if (viewport) semanticRowsUsed = viewport.used;
  process.stdout.write(`${color}\x1b[1m${prefix}\x1b[0m${color}${text}\x1b[0m\n\n`);
  appendJsonl(terminalFile, {
    event_type: "line",
    speaker,
    text,
    style: style || speaker,
    ...(semanticViewportActive ? {
      presentation_scene: presentationScene,
      scheduled_at_ms: presentationAtMs,
      viewport_guarded: guarded,
    } : {}),
    ...(viewport ? {
      viewport_rows: viewport.rows,
      viewport_rows_used: viewport.used,
      viewport_row_budget: viewport.budget,
    } : {}),
  });
}

function getPath(value, dotted) {
  return dotted.split(".").reduce((cursor, key) => cursor?.[key], value);
}

function cloneReplacing(value, replacement) {
  if (value === "$CURRENT_VIEW_REVISION") return replacement;
  if (Array.isArray(value)) return value.map((item) => cloneReplacing(item, replacement));
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, cloneReplacing(item, replacement)]));
  }
  return value;
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

function fileInventory(root) {
  const entries = [];
  const visit = (dir) => {
    for (const item of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const absolute = path.join(dir, item.name);
      if (item.isDirectory()) visit(absolute);
      else if (item.isFile()) {
        const buffer = fs.readFileSync(absolute);
        entries.push({ path: path.relative(root, absolute), bytes: buffer.length, sha256: sha256(buffer) });
      }
    }
  };
  visit(root);
  return entries;
}

const storeSpecs = scenario.stores || [{ id: "default" }];
const processSpecs = scenario.processes || [
  { id: "process", store_id: storeSpecs[0].id, autostart: true, browser: true },
];
const stores = new Map();
for (const spec of storeSpecs) {
  const dataDir = path.join(runDir, "stores", spec.id);
  fs.mkdirSync(dataDir, { recursive: true, mode: 0o700 });
  const fingerprint = sha256(`kmp-capture-store-v1\0${dataDir}`).slice(0, 12);
  stores.set(spec.id, { spec, dataDir, fingerprint });
}
const declaredProcesses = new Map();
for (const [index, raw] of processSpecs.entries()) {
  const spec = { autostart: true, browser: false, ...raw, index };
  if (!stores.has(spec.store_id)) throw new Error(`process ${spec.id} references unknown store ${spec.store_id}`);
  declaredProcesses.set(spec.id, spec);
}
const defaultProcessId = processSpecs[0].id;

class McpClient {
  constructor(spec, instance) {
    this.spec = spec;
    this.processId = spec.id;
    this.instance = instance;
    this.nextId = 1;
    this.pending = new Map();
    this.stderr = "";
    this.viewerPort = baseViewerPort + spec.index;
    this.dataDir = stores.get(spec.store_id).dataDir;
    this.xdgDataDir = path.join(runDir, "process-state", spec.id, `instance-${instance}`, "xdg-data");
    this.xdgConfigDir = path.join(runDir, "process-state", spec.id, `instance-${instance}`, "xdg-config");
    for (const dir of [this.xdgDataDir, this.xdgConfigDir]) fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
    this.stderrFile = path.join(runDir, `mcp.${spec.id}.${instance}.stderr.log`);
    const env = {
      PATH: process.env.PATH,
      LANG: "C.UTF-8",
      LC_ALL: "C.UTF-8",
      KMP_MCP_BACKEND: "embedded",
      KMP_MCP_ENGINE: "sqlite",
      KMP_MCP_DATA_DIR: this.dataDir,
      KMP_VIEWER_ADDR: `127.0.0.1:${this.viewerPort}`,
      RUST_LOG: "info",
      XDG_DATA_HOME: this.xdgDataDir,
      XDG_CONFIG_HOME: this.xdgConfigDir,
    };
    this.child = spawn(binary, [], { cwd: repoRoot, env, stdio: ["pipe", "pipe", "pipe"] });
    this.child.stdin.on("error", (error) => {
      for (const waiter of this.pending.values()) waiter.reject(error);
      this.pending.clear();
    });
    this.child.stderr.setEncoding("utf8");
    this.child.stderr.on("data", (chunk) => {
      this.stderr += chunk;
      fs.appendFileSync(this.stderrFile, chunk);
    });
    readline.createInterface({ input: this.child.stdout }).on("line", (line) => {
      logWire(this.processId, "server_to_client", line);
      let message;
      try { message = JSON.parse(line); } catch (_) { return; }
      const waiter = this.pending.get(message.id);
      if (!waiter) return;
      this.pending.delete(message.id);
      if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
      else waiter.resolve(message.result);
    });
    this.exit = new Promise((resolve) => this.child.once("exit", (code, signal) => resolve({ code, signal })));
  }

  rpc(method, params) {
    const id = this.nextId++;
    const message = JSON.stringify({ jsonrpc: "2.0", id, method, params });
    logWire(this.processId, "client_to_server", message);
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.child.stdin.write(`${message}\n`);
    });
  }

  notify(method, params = {}) {
    const message = JSON.stringify({ jsonrpc: "2.0", method, params });
    logWire(this.processId, "client_to_server", message);
    this.child.stdin.write(`${message}\n`);
  }

  async initialize() {
    const result = await this.rpc("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: `kmp-obs-capture-pty:${this.processId}`, version: "1.0.0" },
    });
    this.notify("notifications/initialized");
    return result;
  }

  async tool(name, args) {
    const result = await this.rpc("tools/call", { name, arguments: args });
    if (result?.isError) throw new Error(`${name} returned isError: ${JSON.stringify(result.content)}`);
    return result;
  }

  async viewerUrl() {
    if (this.cachedViewerUrl) return this.cachedViewerUrl;
    const deadline = Date.now() + 20000;
    while (Date.now() < deadline) {
      const match = this.stderr.match(/memory viewer at (http:\/\/127\.0\.0\.1:\d+\/\?k=[0-9a-f]{64})/);
      if (match) {
        this.cachedViewerUrl = match[1];
        return match[1];
      }
      if (this.child.exitCode !== null) throw new Error(`kmp-mcp ${this.processId} exited before viewer startup: ${this.stderr}`);
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`kmp-mcp ${this.processId} did not publish a ChronoLoom URL`);
  }

  async stop() {
    if (this.child.exitCode === null) this.child.stdin.end();
    const timeout = new Promise((resolve) => setTimeout(() => resolve(null), 3000));
    let exit = await Promise.race([this.exit, timeout]);
    if (!exit) {
      this.child.kill("SIGTERM");
      exit = await this.exit;
    }
    return exit;
  }
}

const binaryBuffer = fs.readFileSync(binary);
const scenarioBuffer = fs.readFileSync(scenarioPath);
const commit = spawnSync("git", ["-C", repoRoot, "rev-parse", "HEAD"], { encoding: "utf8" }).stdout.trim();
const statusResult = spawnSync(
  "git",
  ["-C", repoRoot, "status", "--porcelain=v1", "-z", "--untracked-files=all"],
  { encoding: "utf8" },
);
if (statusResult.status !== 0) throw new Error(`git status failed: ${statusResult.stderr}`);
const ignoredGeneratedPrefixes = ["campaign/embedded-launch/evidence-pack/capture/runs/"];
const statusRecords = statusResult.stdout.split("\0").filter(Boolean);
const changedPaths = [];
for (let index = 0; index < statusRecords.length; index += 1) {
  const record = statusRecords[index];
  const status = record.slice(0, 2);
  let changedPath = record.slice(3);
  if (/[RC]/.test(status) && index + 1 < statusRecords.length) changedPath = `${statusRecords[++index]} -> ${changedPath}`;
  if (ignoredGeneratedPrefixes.some((prefix) => changedPath.startsWith(prefix))) continue;
  changedPaths.push({ status, path: changedPath, path_sha256: sha256(changedPath) });
}
const changedPathsDigest = sha256(changedPaths.map((item) => `${item.status}\0${item.path}`).sort().join("\0"));
const lifecycle = {
  contract: "kmp.capture.process-lifecycle.v2",
  scenario_id: scenario.id,
  fixture_label: scenario.fixture_label || null,
  client_pid: process.pid,
  start: stamp(),
  binary: { path: binary, bytes: binaryBuffer.length, sha256: sha256(binaryBuffer) },
  repository: {
    path: repoRoot,
    commit,
    worktree_dirty: changedPaths.length > 0,
    changed_paths: changedPaths,
    changed_paths_sha256: changedPathsDigest,
    ignored_generated_prefixes: ignoredGeneratedPrefixes,
  },
  scenario: { path: scenarioPath, sha256: sha256(scenarioBuffer) },
  declared_processes: [...declaredProcesses.values()].map(({ index, ...spec }) => spec),
  process_runs: [],
  browser_switches: [],
  environment_allowlist: [
    "PATH", "LANG", "LC_ALL", "KMP_MCP_BACKEND", "KMP_MCP_ENGINE",
    "KMP_MCP_DATA_DIR", "KMP_VIEWER_ADDR", "RUST_LOG", "XDG_DATA_HOME", "XDG_CONFIG_HOME",
  ],
};
function persistLifecycle() {
  fs.writeFileSync(lifecycleFile, `${JSON.stringify(lifecycle, null, 2)}\n`);
}
persistLifecycle();

const activeClients = new Map();
const instanceCounts = new Map();
let currentBrowserProcess = null;

async function startClient(processId) {
  if (activeClients.has(processId)) throw new Error(`process ${processId} is already running`);
  const spec = declaredProcesses.get(processId);
  if (!spec) throw new Error(`unknown process ${processId}`);
  const instance = (instanceCounts.get(processId) || 0) + 1;
  instanceCounts.set(processId, instance);
  const client = new McpClient(spec, instance);
  activeClients.set(processId, client);
  const record = {
    process_id: processId,
    store_id: spec.store_id,
    instance,
    pid: client.child.pid,
    viewer_port: client.viewerPort,
    data_dir: client.dataDir,
    store_fingerprint: stores.get(spec.store_id).fingerprint,
    start: stamp(),
  };
  lifecycle.process_runs.push(record);
  persistLifecycle();
  try {
    await client.initialize();
    const viewerUrl = await client.viewerUrl();
    const parsed = new URL(viewerUrl);
    const capability = parsed.searchParams.get("k");
    record.viewer = { origin: parsed.origin, capability_sha256: sha256(capability), capability_retained: false };
    persistLifecycle();
    appendJsonl(viewerHistoryFile, { process_id: processId, instance, ...record.viewer });
    return client;
  } catch (error) {
    const exit = await client.stop();
    activeClients.delete(processId);
    record.end = stamp();
    record.exit = exit;
    record.start_failure = error.message;
    persistLifecycle();
    throw error;
  }
}

async function stopClient(processId) {
  const client = activeClients.get(processId);
  if (!client) throw new Error(`process ${processId} is not running`);
  const exit = await client.stop();
  activeClients.delete(processId);
  const record = [...lifecycle.process_runs].reverse().find(
    (item) => item.process_id === processId && item.instance === client.instance,
  );
  record.end = stamp();
  record.exit = exit;
  persistLifecycle();
  return exit;
}

async function switchBrowser(processId) {
  const client = activeClients.get(processId);
  if (!client) throw new Error(`cannot switch browser to stopped process ${processId}`);
  const viewerUrl = new URL(await client.viewerUrl());
  viewerUrl.hash = `capture-switch-${lifecycle.browser_switches.length + 1}`;
  fs.writeFileSync(path.join(controlDir, "viewer-url.private"), `${viewerUrl.toString()}\n`, { mode: 0o600 });
  const event = { process_id: processId, instance: client.instance, origin: new URL(viewerUrl).origin, at: stamp() };
  currentBrowserProcess = processId;
  lifecycle.browser_switches.push(event);
  persistLifecycle();
  fs.writeFileSync(viewerFile, `${JSON.stringify({ current_process_id: processId, switches: lifecycle.browser_switches }, null, 2)}\n`);
}

function clientForStep(step) {
  const processId = step.process_id || defaultProcessId;
  const client = activeClients.get(processId);
  if (!client) throw new Error(`tool step targets stopped process ${processId}`);
  return { processId, client };
}

let failure = null;
try {
  process.stdout.write("\x1b]0;KMP_CAPTURE_TERMINAL\x07\x1b]10;#edf1ff\x07\x1b]11;#080b14\x07\x1b[2J\x1b[H");
  terminal("process", "KMP EMBEDDED · REAL MCP / REAL PTY", "process");
  if (scenario.fixture_label) terminal("process", scenario.fixture_label, "neutral");
  terminal("process", scenario.title, "neutral");

  for (const spec of declaredProcesses.values()) {
    if (spec.autostart !== false) {
      const client = await startClient(spec.id);
      terminal("process", `${spec.id} · pid ${client.child.pid} · store ${spec.store_id}`, "process");
    }
  }
  const initialBrowser = [...declaredProcesses.values()].find((spec) => spec.browser && activeClients.has(spec.id))
    || [...declaredProcesses.values()].find((spec) => activeClients.has(spec.id));
  if (!initialBrowser) throw new Error("scenario has no running process for the initial ChronoLoom target");
  await switchBrowser(initialBrowser.id);

  for (const step of scenario.bootstrap) {
    const { processId, client } = clientForStep(step);
    await client.tool(step.name, step.arguments);
    if (step.name === "kmp_view_open" && currentBrowserProcess === processId) await switchBrowser(processId);
  }
  fs.writeFileSync(path.join(controlDir, "client-ready"), `${JSON.stringify(stamp())}\n`, { mode: 0o600 });
  terminal("process", "Isolated SQLite store set ready", "process");
  terminal("process", "Waiting for verified OBS record start…", "neutral");

  const go = await waitForFile(path.join(controlDir, "go"), 60000);
  const baseNs = BigInt(go.monotonic_ns);
  terminal("process", "OBS RECORDING · evidence clock locked", "process");

  for (const step of scenario.steps) {
    await waitUntil(baseNs + BigInt(step.at_ms) * 1000000n);
    presentationAtMs = step.at_ms;
    presentationScene = sceneAt(obsSchedule, step.at_ms);
    if (opensSemanticViewport(step)) {
      resetSemanticViewport(step, {
        write: (value) => process.stdout.write(value),
        audit: (value) => appendJsonl(terminalFile, value),
      });
      semanticViewportActive = true;
      semanticRowsUsed = 0;
    }
    if (step.type === "say") {
      terminal(step.speaker, step.text);
    } else if (step.type === "process") {
      if (step.action === "start") {
        const client = await startClient(step.process_id);
        terminal("process", `${step.process_id} START · pid ${client.child.pid} · store ${client.spec.store_id}`, "process");
      } else if (step.action === "stop") {
        const client = activeClients.get(step.process_id);
        const pid = client?.child.pid;
        const exit = await stopClient(step.process_id);
        terminal("process", `${step.process_id} STOP · pid ${pid} · exit ${exit.code}`, "process");
      } else if (step.action === "switch_browser") {
        await switchBrowser(step.process_id);
        terminal("process", `ChronoLoom → ${step.process_id}`, "process");
      }
    } else if (step.type === "tool") {
      const { processId, client } = clientForStep(step);
      let args = step.arguments;
      if (JSON.stringify(args).includes("$CURRENT_VIEW_REVISION")) {
        const current = await client.tool("kmp_view_get_state", { view_id: "default" });
        args = cloneReplacing(args, current.structuredContent?.view_revision);
      }
      terminal("kmp", `${processId} · ${step.name}`, "tool");
      const result = await client.tool(step.name, args);
      if (step.name === "kmp_view_open" && currentBrowserProcess === processId) await switchBrowser(processId);
      if (step.display_paths?.length) {
        for (const item of step.display_paths) {
          const value = getPath(result, item.path);
          if (value === undefined) throw new Error(`${step.name} response has no ${item.path}`);
          terminal("kmp", `${item.label}: ${typeof value === "string" ? value : JSON.stringify(value)}`, "kmp");
        }
      } else {
        terminal("kmp", JSON.stringify(result.structuredContent), "kmp");
      }
    }
  }
  await waitUntil(baseNs + BigInt(scenario.duration_ms) * 1000000n);
  fs.writeFileSync(path.join(controlDir, "scenario-complete"), `${JSON.stringify(stamp())}\n`, { mode: 0o600 });
  await waitForFile(path.join(controlDir, "stop"), 30000);
} catch (error) {
  failure = { message: error.message, stack: error.stack };
  fs.writeFileSync(path.join(controlDir, "client-failed"), `${JSON.stringify({ ...stamp(), failure }, null, 2)}\n`, { mode: 0o600 });
  semanticViewportActive = false;
  terminal("process", `CAPTURE FAILED · ${error.message}`, "neutral");
} finally {
  for (const processId of [...activeClients.keys()]) {
    try { await stopClient(processId); } catch (error) {
      if (!failure) failure = { message: error.message, stack: error.stack };
    }
  }
  lifecycle.end = stamp();
  lifecycle.failure = failure;
  lifecycle.mcp_pid = lifecycle.process_runs[0]?.pid || null;
  lifecycle.mcp_exit = lifecycle.process_runs[0]?.exit || null;
  persistLifecycle();
  const storeRecords = [...stores].map(([id, store]) => ({
    id,
    engine: "sqlite",
    durability: "wal",
    selected_data_dir: store.dataDir,
    fingerprint: store.fingerprint,
    isolated_from_user_store: true,
    files: fileInventory(store.dataDir),
  }));
  fs.writeFileSync(storeFile, `${JSON.stringify({
    contract: "kmp.capture.stores.v2",
    engine: "sqlite",
    durability: "wal",
    isolated_from_user_store: true,
    selected_data_dir: storeRecords[0].selected_data_dir,
    files: storeRecords[0].files,
    stores: storeRecords,
  }, null, 2)}\n`);
}

if (failure) process.exitCode = 1;
