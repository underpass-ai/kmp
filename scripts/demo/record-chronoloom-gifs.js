#!/usr/bin/env node
"use strict";

// Reproducible README capture for ChronoLoom. This script does not drive the
// product through DOM shortcuts: it writes the fixture and moves the shared
// view through the public MCP tools, then waits for the browser's long poll to
// render those semantic intents. Browser automation after that only frames
// the resulting product state for the recording.

const fs = require("node:fs");
const path = require("node:path");
const readline = require("node:readline");
const crypto = require("node:crypto");
const { spawn, spawnSync } = require("node:child_process");
const { chromium } = require("playwright");

const root = path.resolve(__dirname, "../..");
const workRoot = process.env.KMP_CHRONOLOOM_CAPTURE_ROOT;
if (!workRoot) throw new Error("KMP_CHRONOLOOM_CAPTURE_ROOT is required");

const binary = process.env.KMP_MCP_BIN || path.join(root, "target/debug/kmp-mcp");
const chrome = process.env.KMP_CAPTURE_CHROME || "/usr/bin/google-chrome";
const frames = path.join(workRoot, "states");
fs.mkdirSync(frames, { recursive: true });
const wirePath = process.env.KMP_CAPTURE_WIRE_JSONL || path.join(workRoot, "tool-calls.jsonl");
const captureEvidence = {
  schema_version: "1",
  capture_kind: "browser_product_evidence_only",
  product_binary: binary,
  scenarios: {},
  processes: {},
  editorial_copy: [],
};

function sanitizeWire(value, redactions) {
  if (Array.isArray(value)) return value.map((item) => sanitizeWire(item, redactions));
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, sanitizeWire(item, redactions)])
    );
  }
  if (typeof value === "string" && /[?&]k=[0-9a-f]{64}/.test(value)) {
    redactions.push("expired viewer capability query");
    return value.replace(/([?&]k=)[0-9a-f]{64}/g, "$1<redacted>");
  }
  return value;
}

function recordWire(session, processId, direction, wire) {
  const redactions = [];
  let payload;
  try {
    payload = sanitizeWire(JSON.parse(wire), redactions);
  } catch (_) {
    payload = { unparsed: true };
  }
  const record = {
    session,
    process_id: processId,
    direction,
    wire_sha256: crypto.createHash("sha256").update(wire).digest("hex"),
    payload,
    redactions: [...new Set(redactions)],
  };
  fs.appendFileSync(wirePath, `${JSON.stringify(record)}\n`);
}

const ABOUT = "incident:pool-saturation";
const CFG_CHANGE = `${ABOUT}:cfg-change`;
const SATURATION = `${ABOUT}:saturation`;
const HYPO_TRAFFIC = `${ABOUT}:hypo-traffic`;
const VOLUME_FLAT = `${ABOUT}:volume-flat`;
const ROOT_CAUSE = `${ABOUT}:root-cause`;
const DEPLOY_FREEZE = `${ABOUT}:deploy-freeze`;
const VERIFIED = `${ABOUT}:verified`;
function coordinate(dimension, scopeId, sequence, occurred, observed, ingested, validity) {
  const value = {
    dimension,
    scope_id: scopeId,
    sequence,
    occurred_at: occurred,
    observed_at: observed,
    ingested_at: ingested,
  };
  if (validity) {
    value.valid_from = validity[0];
    value.valid_until = validity[1];
  }
  return value;
}

function memoryFixture() {
  const timeline = "timeline:pool-saturation";
  const decisions = "decision:pool-saturation";
  const inTimeline = (sequence, occurred, observed, ingested, validity) =>
    coordinate("timeline", timeline, sequence, occurred, observed, ingested, validity);
  const inDecisions = (sequence, occurred, observed, ingested, validity) =>
    coordinate("decision", decisions, sequence, occurred, observed, ingested, validity);

  return {
    about: ABOUT,
    idempotency_key: "ingest:chronoloom-readme-pool-saturation-v1",
    provenance: {
      source_kind: "agent",
      source_agent: "chronoloom-readme-capture",
      observed_at: "2026-08-27T10:22:00Z",
    },
    memory: {
      dimensions: [
        { id: timeline, kind: "timeline", title: "Incident timeline" },
        { id: decisions, kind: "decision", title: "Decision thread" },
      ],
      entries: [
        {
          id: CFG_CHANGE,
          kind: "decision",
          text: "The deploy lowered max_connections from 200 to 20.",
          coordinates: [
            inTimeline(1, "2026-08-27T03:12:00Z", "2026-08-27T09:40:00Z", "2026-08-27T09:41:00Z"),
            inDecisions(1, "2026-08-27T03:12:00Z", "2026-08-27T09:40:00Z", "2026-08-27T09:41:00Z"),
          ],
        },
        {
          id: SATURATION,
          kind: "observation",
          text: "Connection checkout latency crossed two seconds.",
          coordinates: [
            inTimeline(2, "2026-08-27T09:38:00Z", "2026-08-27T09:38:20Z", "2026-08-27T09:39:00Z"),
          ],
        },
        {
          id: HYPO_TRAFFIC,
          kind: "decision",
          text: "This looks like a traffic spike; scale the service.",
          coordinates: [
            inTimeline(3, "2026-08-27T09:41:00Z", "2026-08-27T09:41:00Z", "2026-08-27T09:41:20Z"),
            inDecisions(2, "2026-08-27T09:41:00Z", "2026-08-27T09:41:00Z", "2026-08-27T09:41:20Z"),
          ],
        },
        {
          id: VOLUME_FLAT,
          kind: "observation",
          text: "Request volume had stayed flat all morning.",
          coordinates: [
            inTimeline(4, "2026-08-27T09:52:00Z", "2026-08-27T09:52:00Z", "2026-08-27T09:52:20Z"),
          ],
        },
        {
          id: ROOT_CAUSE,
          kind: "decision",
          text: "The pool ceiling caused the incident; restore max_connections to 200.",
          coordinates: [
            inTimeline(5, "2026-08-27T10:05:00Z", "2026-08-27T10:05:00Z", "2026-08-27T10:05:20Z"),
            inDecisions(3, "2026-08-27T10:05:00Z", "2026-08-27T10:05:00Z", "2026-08-27T10:05:20Z"),
          ],
        },
        {
          id: DEPLOY_FREEZE,
          kind: "constraint",
          text: "Deployments are frozen while the restored pool is verified.",
          coordinates: [
            inTimeline(
              6,
              "2026-08-27T10:07:00Z",
              "2026-08-27T10:07:00Z",
              "2026-08-27T10:07:20Z",
              ["2026-08-27T10:07:00Z", "2026-08-27T14:00:00Z"]
            ),
            inDecisions(
              4,
              "2026-08-27T10:07:00Z",
              "2026-08-27T10:07:00Z",
              "2026-08-27T10:07:20Z",
              ["2026-08-27T10:07:00Z", "2026-08-27T14:00:00Z"]
            ),
          ],
        },
        {
          id: VERIFIED,
          kind: "success_path",
          text: "Checkout latency is back to 180 ms and the pool is healthy.",
          coordinates: [
            inTimeline(7, "2026-08-27T10:22:00Z", "2026-08-27T10:22:00Z", "2026-08-27T10:22:20Z"),
          ],
        },
      ],
      relations: [
        {
          from: SATURATION,
          to: CFG_CHANGE,
          rel: "depends_on",
          class: "causal",
          why: "A pool ceiling of 20 is what saturated under ordinary load.",
          evidence: "The ceiling was the only capacity change before checkout latency crossed two seconds.",
          confidence: "high",
        },
        {
          from: HYPO_TRAFFIC,
          to: SATURATION,
          rel: "chosen_because",
          class: "motivational",
          why: "The saturation was read as demand before request volume was checked.",
          evidence: "The first mitigation proposed scaling while the dashboard still showed a flat request rate.",
          confidence: "high",
        },
        {
          from: VOLUME_FLAT,
          to: HYPO_TRAFFIC,
          rel: "contradicts",
          class: "evidential",
          why: "Flat request volume contradicts the traffic-spike hypothesis.",
          evidence: "The request-rate series stayed inside its morning baseline through the incident.",
          confidence: "high",
        },
        {
          from: ROOT_CAUSE,
          to: HYPO_TRAFFIC,
          rel: "supersedes",
          class: "evidential",
          why: "The config change replaces the traffic-spike reading.",
          evidence: "The pool limit changed from 200 to 20 while traffic remained flat.",
          confidence: "high",
        },
        {
          from: DEPLOY_FREEZE,
          to: ROOT_CAUSE,
          rel: "chosen_because",
          class: "motivational",
          why: "The team froze unrelated deploys so the pool restoration could be verified cleanly.",
          evidence: "The freeze began two minutes after the root cause was recorded.",
          confidence: "high",
        },
        {
          from: VERIFIED,
          to: ROOT_CAUSE,
          rel: "verified_by",
          class: "evidential",
          why: "Restoring the pool returned latency to baseline.",
          evidence: "Checkout latency fell to 180 ms after max_connections returned to 200.",
          confidence: "high",
        },
        {
          from: VOLUME_FLAT,
          to: ROOT_CAUSE,
          rel: "supports",
          class: "evidential",
          why: "Stable demand isolates the configuration change as the relevant capacity shift.",
          evidence: "Request volume did not rise while pool wait time did.",
          confidence: "high",
        },
      ],
      evidence: [
        {
          id: `evidence:${ABOUT}:pool-config-diff`,
          supports: [CFG_CHANGE, ROOT_CAUSE],
          text: "The deployment diff changed max_connections from 200 to 20.",
          source: "deployment diff",
          time: "2026-08-27T03:12:00Z",
        },
        {
          id: `evidence:${ABOUT}:pool-latency`,
          supports: [SATURATION, VERIFIED],
          text: "Checkout latency crossed two seconds, then returned to 180 ms after restoration.",
          source: "pool telemetry",
          time: "2026-08-27T10:22:00Z",
        },
        {
          id: `evidence:${ABOUT}:request-volume`,
          supports: [VOLUME_FLAT, ROOT_CAUSE],
          text: "Request volume stayed flat across the incident window.",
          source: "request-rate telemetry",
          time: "2026-08-27T09:52:00Z",
        },
      ],
    },
  };
}

function launchDecision({ about, ref, summary, evidence, actor, idempotencyKey }) {
  return {
    about,
    intent: "record_decision",
    actor,
    occurred_at: "2026-08-28T15:58:00Z",
    observed_at: "2026-08-28T16:00:00Z",
    scope: { process: "campaign:kmp-embedded-launch" },
    current: { ref, kind: "decision", summary, evidence },
    idempotency_key: idempotencyKey,
    options: { dry_run: false, strict: false, sequence: 1 },
  };
}

function storeFingerprint(dataDir) {
  return require("node:crypto")
    .createHash("sha256")
    .update(path.resolve(dataDir))
    .digest("hex")
    .slice(0, 12);
}

class McpSession {
  constructor(port, name, sharedDataDir = null) {
    this.nextId = 1;
    this.pending = new Map();
    this.stderr = [];
    const dataDir = sharedDataDir || path.join(workRoot, "stores", name);
    fs.mkdirSync(dataDir, { recursive: true });
    this.dataDir = dataDir;
    this.origin = port === null ? null : `http://127.0.0.1:${port}/`;
    this.child = spawn(binary, [], {
      cwd: root,
      env: {
        ...process.env,
        KMP_MCP_BACKEND: "embedded",
        KMP_MCP_ENGINE: "sqlite",
        KMP_MCP_DATA_DIR: dataDir,
        KMP_VIEWER_ADDR: port === null ? "off" : `127.0.0.1:${port}`,
        RUST_LOG: "error",
      },
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.name = name;
    captureEvidence.processes[name] = {
      process_id: this.child.pid,
      store_fingerprint: storeFingerprint(dataDir),
      viewer_enabled: port !== null,
    };
    this.child.stderr.setEncoding("utf8");
    this.child.stderr.on("data", (chunk) => this.stderr.push(chunk));
    readline.createInterface({ input: this.child.stdout }).on("line", (line) => {
      let message;
      try {
        message = JSON.parse(line);
      } catch (_) {
        return;
      }
      recordWire(this.name, this.child.pid, "response", line);
      const waiter = this.pending.get(message.id);
      if (!waiter) return;
      this.pending.delete(message.id);
      if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
      else waiter.resolve(message.result);
    });
    this.child.on("exit", (code) => {
      const detail = this.stderr.join("").trim();
      for (const waiter of this.pending.values()) {
        waiter.reject(new Error(`kmp-mcp exited ${code}${detail ? `: ${detail}` : ""}`));
      }
      this.pending.clear();
    });
  }

  rpc(method, params) {
    const id = this.nextId++;
    const message = JSON.stringify({ jsonrpc: "2.0", id, method, params });
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      recordWire(this.name, this.child.pid, "request", message);
      this.child.stdin.write(`${message}\n`);
    });
  }

  async tool(name, args) {
    const result = await this.rpc("tools/call", { name, arguments: args });
    const body = result && result.structuredContent;
    if (!body) throw new Error(`${name} returned no structuredContent: ${JSON.stringify(result)}`);
    if (body.error) throw new Error(`${name}: ${body.error.code}: ${body.error.message}`);
    return body;
  }

  async viewerUrl() {
    if (this.origin === null) throw new Error("this MCP process has no viewer");
    const deadline = Date.now() + 20000;
    while (Date.now() < deadline) {
      const match = this.stderr
        .join("")
        .match(/memory viewer at (http:\/\/127\.0\.0\.1:\d+\/\?k=[0-9a-f]{64})/);
      if (match) return match[1];
      if (this.child.exitCode !== null) {
        throw new Error(`kmp-mcp exited before offering ChronoLoom: ${this.stderr.join("").trim()}`);
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`kmp-mcp did not offer ChronoLoom at ${this.origin}`);
  }

  async stop() {
    if (this.child.exitCode !== null) return;
    this.child.stdin.end();
    await new Promise((resolve) => {
      this.child.once("exit", resolve);
      setTimeout(() => {
        if (this.child.exitCode === null) this.child.kill("SIGTERM");
      }, 2000);
    });
  }
}

async function waitForViewer(url) {
  const deadline = Date.now() + 20000;
  let last;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { redirect: "manual" });
      if (response.status === 303 && response.headers.get("location") === "/") return;
      last = new Error(`HTTP ${response.status}`);
    } catch (error) {
      last = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`viewer did not start: ${last}`);
}

async function waitForLoom(loom, requireAbout = true) {
  await loom.waitForFunction(() => {
    const entries = document.getElementById("s-entries");
    const about = document.querySelector("#about-list .active");
    return entries && (!window.__kmpRequireAbout || about);
  }, undefined, { timeout: 20000 });
  await loom.waitForTimeout(700);
}

// Campaign picture is captured by OBS from a real PTY and a real browser.
// This browser probe never draws or reconstructs a terminal inside ChronoLoom.
async function reloadAgentLoom(page, requireAbout = true) {
  await page.evaluate(() => {
    localStorage.clear();
    sessionStorage.clear();
  });
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.evaluate((required) => { window.__kmpRequireAbout = required; }, requireAbout);
  await waitForLoom(page, requireAbout);
}

async function openAgentLoom(browser, url, requireAbout = true) {
  const context = await browser.newContext({
    viewport: { width: 1920, height: 1080 },
    colorScheme: "dark",
    deviceScaleFactor: 1,
    reducedMotion: "no-preference",
  });
  const page = await context.newPage();
  // ChronoLoom intentionally keeps one long-poll request in flight, so
  // networkidle would mean the product had stopped listening to the agent.
  await page.goto(url, { waitUntil: "domcontentloaded" });
  await page.evaluate((required) => { window.__kmpRequireAbout = required; }, requireAbout);
  await waitForLoom(page, requireAbout);
  return { context, page, loom: page };
}

async function setTerminalPhase(_page, phase) {
  captureEvidence.editorial_copy.push({ phase });
}

async function setTerminalCard(_page, card) {
  captureEvidence.editorial_copy.push({
    brand: card.brand || null,
    hook: card.hook || null,
    prompt: card.prompt || null,
    receipt: card.receipt || null,
    cta: card.cta || null,
  });
}

async function screenshot(page, name) {
  let previous = null;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    const current = await page.screenshot({ animations: "disabled" });
    if (previous && current.equals(previous)) {
      fs.writeFileSync(path.join(frames, `${name}.png`), current);
      return;
    }
    previous = current;
    await page.waitForTimeout(100);
  }
  throw new Error(`${name} never reached two identical render frames`);
}

async function currentRevision(session) {
  return (await session.tool("kmp_view_get_state", { view_id: "default" })).view_revision;
}

async function selectInLoom(session, selection, explanation, key) {
  return session.tool("kmp_view_apply_intent", {
    view_id: "default",
    expected_revision: await currentRevision(session),
    idempotency_key: key,
    actor: "agent",
    explanation,
    focus: { refs: [selection] },
    projection: { semantic_zoom: "moment" },
    selection,
  });
}

async function captureFreshProcessStory(browser) {
  const shared = path.join(workRoot, "stores", "fresh-process-shared");
  const fingerprint = storeFingerprint(shared);
  const about = "project:campaign-fresh-process";
  const ref = `${about}:decision:sqlite-wal`;
  const write = launchDecision({
    about,
    ref,
    summary: "Use SQLite WAL so independent KMP processes can recover the same local memory.",
    evidence: "The two-process regression writes through one store and both processes recover the committed decision.",
    actor: "agent:session-01",
    idempotencyKey: "write:campaign:fresh-process:sqlite-wal:v1",
  });
  const evidence = { about, ref, store_fingerprint: fingerprint, calls: [] };
  captureEvidence.scenarios.fresh_process = evidence;

  const first = new McpSession(17318, "fresh-process-01", shared);
  let firstStopped = false;
  let firstContext;
  try {
    evidence.session_01_pid = first.child.pid;
    const firstUrl = await first.viewerUrl();
    await waitForViewer(firstUrl);
    const written = await first.tool("kmp_write_memory", write);
    evidence.calls.push({ process: 1, tool: "kmp_write_memory", accepted: written.accepted, ref });
    if (written.accepted !== true) throw new Error(`fresh write was not accepted: ${JSON.stringify(written)}`);
    await first.tool("kmp_view_open", { about });
    await selectInLoom(first, ref, "show the committed decision and its evidence", "view:campaign:fresh:commit");
    ({ context: firstContext } = await openAgentLoom(browser, firstUrl, true));
    let firstPage = firstContext.pages()[0];

    await setTerminalCard(firstPage, {
      brand: "SESSION 01 × KMP",
      hook: "End the session.\nKeep the why.",
      sub: `local store · ${fingerprint}`,
      footRight: "MEMORY, WITH RECEIPTS",
    });
    await screenshot(firstPage, "fresh-01-hook");

    await setTerminalCard(firstPage, {
      brand: "SESSION 01 × KMP",
      shell: `pid ${first.child.pid} · store ${fingerprint}`,
      speaker: "agent",
      prompt: "Remember why KMP Embedded uses SQLite WAL.",
      tool: "kmp_write_memory",
      args: [["about", about], ["intent", "record_decision"]],
      footRight: "LOCAL SQLITE WAL",
    });
    await screenshot(firstPage, "fresh-02-write");

    await firstPage.waitForFunction((expected) => document.getElementById("d-id")?.textContent === expected, ref);
    await setTerminalCard(firstPage, {
      brand: "SESSION 01 × KMP",
      shell: `pid ${first.child.pid} · store ${fingerprint}`,
      stamp: "DECISION COMMITTED",
      tool: "kmp_write_memory",
      args: [["accepted", String(written.accepted)], ["ref", ref]],
      receipt: "Evidence attached: the two-process regression recovers the same committed decision.",
      footRight: "PERSISTED LOCALLY",
    });
    await screenshot(firstPage, "fresh-03-committed");

    await first.stop();
    firstStopped = true;
    evidence.session_01_exit_code = first.child.exitCode;
    await setTerminalCard(firstPage, {
      brand: "SESSION 01 × KMP",
      status: "ENDED",
      stamp: "SESSION 01 ENDED",
      sub: `pid ${evidence.session_01_pid} exited · store ${fingerprint} remains`,
      footLeft: "PROCESS STATE GONE",
      footRight: "MEMORY STAYS",
    });
    await screenshot(firstPage, "fresh-04-ended");
    await firstContext.close();
    firstContext = null;

    const second = new McpSession(17318, "fresh-process-02", shared);
    evidence.session_02_pid = second.child.pid;
    try {
      await second.tool("kmp_view_open", { about });
      const secondUrl = await second.viewerUrl();
      await waitForViewer(secondUrl);
      const opened = await openAgentLoom(browser, secondUrl, true);
      const secondPage = opened.page;
      try {
        await setTerminalCard(secondPage, {
          brand: "SESSION 02 × KMP",
          stamp: "SESSION 02 · SAME STORE",
          sub: `new pid ${second.child.pid} · store ${fingerprint}`,
          footLeft: "FRESH PROCESS",
          footRight: "NO EXPORT · NO IMPORT",
        });
        await screenshot(secondPage, "fresh-05-second-process");

        await setTerminalCard(secondPage, {
          brand: "SESSION 02 × KMP",
          shell: `pid ${second.child.pid} · store ${fingerprint}`,
          speaker: "you",
          prompt: "Why does KMP Embedded use SQLite WAL?",
          tool: "kmp_inspect",
          args: [["about", about], ["ref", ref]],
          footRight: "FRESH PROCESS",
        });
        await screenshot(secondPage, "fresh-06-question");

        const inspected = await second.tool("kmp_inspect", {
          about,
          ref,
          include: { details: true, incoming: true, outgoing: true },
          budget: { max_bytes: 12000 },
        });
        evidence.calls.push({ process: 2, tool: "kmp_inspect", ref, object: inspected.object?.text });
        if (!inspected.object?.text?.includes("SQLite WAL")) {
          throw new Error(`fresh process did not recover the decision: ${JSON.stringify(inspected)}`);
        }
        await selectInLoom(second, ref, "recover the decision in a fresh process", "view:campaign:fresh:recover");
        await secondPage.waitForFunction((expected) => document.getElementById("d-id")?.textContent === expected, ref);
        await setTerminalCard(secondPage, {
          brand: "SESSION 02 × KMP",
          shell: `pid ${second.child.pid} · store ${fingerprint}`,
          stamp: "RECOVERED FROM LOCAL MEMORY",
          receipt: inspected.object.text,
          sub: "The evidence is still attached to the decision.",
          footLeft: "FRESH PROCESS",
          footRight: "SAME WHY",
        });
        await screenshot(secondPage, "fresh-07-recovered");

        await setTerminalCard(secondPage, {
          brand: "KMP EMBEDDED",
          hook: "Fresh process.\nSame decision.",
          sub: "Evidence attached.",
          cta: "Run KMP Embedded → github.com/underpass-ai/kmp",
          footLeft: "NO KMP ACCOUNT",
          footRight: "LOCAL-FIRST",
        });
        await screenshot(secondPage, "fresh-08-close");
      } finally {
        await opened.context.close();
      }
    } finally {
      await second.stop();
    }
  } finally {
    if (firstContext) await firstContext.close();
    if (!firstStopped) await first.stop();
  }
}

async function captureTwoProcessesStory(browser) {
  const shared = path.join(workRoot, "stores", "two-processes-shared");
  const fingerprint = storeFingerprint(shared);
  const about = "incident:campaign-pool-limit";
  const ref = `${about}:decision:restore-max-connections`;
  const write = launchDecision({
    about,
    ref,
    summary: "Restore max_connections to 200. The pool ceiling caused saturation.",
    evidence: "Checkout latency returned to 180 ms after restoring the connection ceiling while request volume stayed flat.",
    actor: "agent:a",
    idempotencyKey: "write:campaign:two-processes:restore-pool:v1",
  });
  const evidence = { about, ref, store_fingerprint: fingerprint, calls: [] };
  captureEvidence.scenarios.two_processes = evidence;
  const agentA = new McpSession(17319, "two-processes-a", shared);
  const agentB = new McpSession(17320, "two-processes-b", shared);
  evidence.agent_a_pid = agentA.child.pid;
  evidence.agent_b_pid = agentB.child.pid;
  try {
    const urlA = await agentA.viewerUrl();
    await agentB.viewerUrl();
    await waitForViewer(urlA);
    const written = await agentA.tool("kmp_write_memory", write);
    evidence.calls.push({ process: "A", tool: "kmp_write_memory", accepted: written.accepted, ref });
    if (written.accepted !== true) throw new Error(`process A write failed: ${JSON.stringify(written)}`);
    await agentA.tool("kmp_view_open", { about });
    await selectInLoom(agentA, ref, "show the memory Process A committed", "view:campaign:processes:a");
    let openedA = await openAgentLoom(browser, urlA, true);
    try {
      await setTerminalCard(openedA.page, {
        brand: "PROCESS A × KMP",
        hook: "Process A writes it.\nProcess B recovers the why.",
        sub: `two real MCP processes · store ${fingerprint}`,
        footRight: "ONE LOCAL MEMORY",
      });
      await screenshot(openedA.page, "processes-01-hook");

      await setTerminalCard(openedA.page, {
        brand: "PROCESS A × KMP",
        shell: `pid ${agentA.child.pid} · store ${fingerprint}`,
        speaker: "process a",
        prompt: "Remember: restore max_connections to 200. The pool ceiling caused saturation.",
        tool: "kmp_write_memory",
        args: [["about", about], ["intent", "record_decision"]],
        footRight: "LOCAL SQLITE WAL",
      });
      await screenshot(openedA.page, "processes-02-a-write");
      await openedA.page.waitForFunction((expected) => document.getElementById("d-id")?.textContent === expected, ref);
      await setTerminalCard(openedA.page, {
        brand: "PROCESS A × KMP",
        stamp: "PROCESS A COMMITTED",
        receipt: "Restore max_connections to 200. The pool ceiling caused saturation.",
        sub: `pid ${agentA.child.pid} · store ${fingerprint}`,
        footRight: "EVIDENCE ATTACHED",
      });
      await screenshot(openedA.page, "processes-03-a-committed");
    } finally {
      await openedA.context.close();
    }

    await agentB.tool("kmp_view_open", { about });
    const urlB = await agentB.viewerUrl();
    await waitForViewer(urlB);
    const openedB = await openAgentLoom(browser, urlB, true);
    try {
      await setTerminalCard(openedB.page, {
        brand: "PROCESS A → PROCESS B",
        stamp: "SAME LOCAL STORE",
        sub: `pid ${agentA.child.pid} → pid ${agentB.child.pid}\nstore ${fingerprint}`,
        hook: "No export.\nNo import.",
        footLeft: "TWO PROCESSES",
        footRight: "ONE SQLITE WAL",
      });
      await screenshot(openedB.page, "processes-04-transition");

      await setTerminalCard(openedB.page, {
        brand: "PROCESS B × KMP",
        shell: `pid ${agentB.child.pid} · store ${fingerprint}`,
        speaker: "you",
        prompt: "Why is max_connections back at 200?",
        tool: "kmp_ask",
        args: [["about", about], ["question", "Why is max_connections back at 200?"]],
        footRight: "READING PROCESS A'S MEMORY",
      });
      await screenshot(openedB.page, "processes-05-b-question");

      const asked = await agentB.tool("kmp_ask", {
        about,
        question: "Why is max_connections back at 200?",
        answer_policy: "evidence_or_unknown",
        budget: { detail: "full", max_bytes: 12000 },
      });
      const inspected = await agentB.tool("kmp_inspect", {
        about,
        ref,
        include: { details: true, incoming: true, outgoing: true },
        budget: { max_bytes: 12000 },
      });
      evidence.calls.push({ process: "B", tool: "kmp_ask", answer: asked.answer });
      evidence.calls.push({ process: "B", tool: "kmp_inspect", object: inspected.object?.text, ref });
      if (!inspected.object?.text?.includes("max_connections")) {
        throw new Error(`process B did not recover Process A's decision: ${JSON.stringify(inspected)}`);
      }

      await setTerminalCard(openedB.page, {
        brand: "PROCESS B × KMP",
        stamp: "ONE SQLITE WAL STORE",
        sub: `process A ${agentA.child.pid} · process B ${agentB.child.pid} · ${fingerprint}`,
        receipt: "No export. No import. Both MCP processes opened the same local store.",
        footLeft: "INDEPENDENT PROCESSES",
        footRight: "SHARED MEMORY",
      });
      await screenshot(openedB.page, "processes-06-wal");

      await selectInLoom(agentB, ref, "recover why max_connections returned to 200", "view:campaign:agents:b");
      await openedB.page.waitForFunction((expected) => document.getElementById("d-id")?.textContent === expected, ref);
      await setTerminalCard(openedB.page, {
        brand: "PROCESS B × KMP",
        stamp: "PROCESS B RECOVERED THE WHY",
        receipt: inspected.object.text,
        sub: "Evidence: checkout latency returned to 180 ms while request volume stayed flat.",
        footLeft: "PROCESS A WROTE",
        footRight: "PROCESS B RECOVERED",
      });
      await screenshot(openedB.page, "processes-07-recovered");

      await setTerminalCard(openedB.page, {
        brand: "KMP EMBEDDED",
        hook: "Two processes.\nOne SQLite WAL store.",
        sub: "No memory service.",
        cta: "Install KMP Embedded → github.com/underpass-ai/kmp",
        footLeft: "LOCAL-FIRST",
        footRight: "MEMORY, WITH RECEIPTS",
      });
      await screenshot(openedB.page, "processes-08-close");
    } finally {
      await openedB.context.close();
    }
  } finally {
    await agentA.stop();
    await agentB.stop();
  }
}

async function captureAgentStory(browser) {
  const session = new McpSession(17317, "agent-story");
  try {
    const evidence = {
      about: ABOUT,
      store_fingerprint: storeFingerprint(session.dataDir),
      process_pid: session.child.pid,
      calls: [],
    };
    captureEvidence.scenarios.wrong_turn = evidence;
    const viewerUrl = await session.viewerUrl();
    await waitForViewer(viewerUrl);
    const ingested = await session.tool("kmp_ingest", memoryFixture());
    evidence.calls.push({ tool: "kmp_ingest", accepted: ingested.memory?.accepted });
    const { context, page, loom } = await openAgentLoom(browser, viewerUrl);
    try {
      await loom.waitForFunction(() =>
        document.getElementById("agent-chip-text").textContent === "human-controlled view"
      );
      await screenshot(page, "wrong-01-hook");

      await setTerminalPhase(page, "question");
      await screenshot(page, "wrong-02-question");

      await session.tool("kmp_view_apply_intent", {
        view_id: "default",
        expected_revision: await currentRevision(session),
        idempotency_key: "view:chronoloom-readme-select-cfg-change",
        actor: "agent",
        explanation: "show the memory behind the pool-limit decision",
        focus: { refs: [CFG_CHANGE] },
        projection: { semantic_zoom: "moment" },
        selection: CFG_CHANGE,
      });
      await loom.waitForFunction((expected) => {
        const chip = document.getElementById("agent-chip-text");
        const selected = document.getElementById("d-id");
        return chip && chip.textContent.includes("show the memory behind the pool-limit decision") &&
          selected && selected.textContent === expected;
      }, CFG_CHANGE);
      await loom.waitForFunction(() => {
        const prism = [...document.querySelectorAll("#prism .prism-rail")].map((row) => row.textContent);
        return prism.some((text) => text.includes("03:12")) &&
          prism.some((text) => text.includes("09:40")) &&
          prism.some((text) => text.includes("09:41"));
      });
      await setTerminalPhase(page, "selection");
      await page.waitForTimeout(500);
      await screenshot(page, "wrong-03-selection");

      await setTerminalPhase(page, "followup");
      await screenshot(page, "wrong-04-followup");

      const trace = { from: VERIFIED, to: CFG_CHANGE };
      const traced = await session.tool("kmp_view_apply_intent", {
        view_id: "default",
        expected_revision: await currentRevision(session),
        idempotency_key: "view:chronoloom-readme-trace-proof",
        actor: "agent",
        explanation: "light up the proof path",
        trace,
      });
      evidence.calls.push({ tool: "kmp_view_apply_intent", trace, view_revision: traced.view_revision });
      try {
        await loom.waitForFunction(() => {
          const chip = document.getElementById("agent-chip-text");
          const status = document.getElementById("trace-status");
          return chip && chip.textContent.includes("light up the proof path") &&
            status && status.textContent.startsWith("4 hops") &&
            document.querySelectorAll("#trace-hops > li").length === 4;
        }, undefined, { timeout: 4000 });
      } catch (error) {
        const state = await loom.evaluate(() => ({
          chip: document.getElementById("agent-chip-text")?.textContent,
          clock: document.querySelector("#clock-chips .chip.active")?.dataset.clock,
          entries: document.getElementById("s-entries")?.textContent,
          relations: document.getElementById("s-relations")?.textContent,
          trace: document.getElementById("trace-status")?.textContent,
          hops: document.querySelectorAll("#trace-hops > li").length,
          error: document.getElementById("error-slot")?.textContent,
        }));
        throw new Error(`trace did not render: ${JSON.stringify(state)}`, { cause: error });
      }
      await setTerminalPhase(page, "trace");
      await page.waitForTimeout(500);
      await screenshot(page, "wrong-05-reveal");

      const hops = [
        [VERIFIED, ROOT_CAUSE, "verified_by"],
        [ROOT_CAUSE, HYPO_TRAFFIC, "supersedes"],
        [HYPO_TRAFFIC, SATURATION, "chosen_because"],
        [SATURATION, CFG_CHANGE, "depends_on"],
      ];
      for (let index = 0; index < hops.length; index += 1) {
        const [from, to, relation] = hops[index];
        const focused = await session.tool("kmp_view_apply_intent", {
          view_id: "default",
          expected_revision: await currentRevision(session),
          idempotency_key: `view:chronoloom-campaign-proof-hop-${index + 1}`,
          actor: "agent",
          explanation: `proof hop ${index + 1} — ${relation}`,
          focus: { refs: [from, to] },
          selection: from,
          trace,
        });
        evidence.calls.push({
          tool: "kmp_view_apply_intent",
          proof_hop: index + 1,
          relation,
          from,
          to,
          view_revision: focused.view_revision,
        });
        await loom.waitForFunction(
          (expected) => document.getElementById("agent-chip-text")?.textContent.includes(expected),
          `proof hop ${index + 1}`
        );
        await page.waitForTimeout(350);
        await screenshot(page, `wrong-${String(index + 6).padStart(2, "0")}-hop${index + 1}`);
      }

      await loom.locator("#trace-box").scrollIntoViewIfNeeded();
      await setTerminalPhase(page, "nice");
      await page.waitForTimeout(500);
      await screenshot(page, "wrong-10-nice");

      await setTerminalCard(page, {
        brand: "KMP EMBEDDED",
        hook: "Memory,\nwith receipts.",
        sub: "The wrong hypothesis stays. So does the evidence that replaced it.",
        cta: "See KMP Embedded → github.com/underpass-ai/kmp",
        footLeft: "4 AUDITABLE HOPS",
        footRight: "WHY PRESERVED",
      });
      await screenshot(page, "wrong-11-close");
    } finally {
      await context.close();
    }
  } finally {
    await session.stop();
  }
}

async function main() {
  if (!fs.existsSync(binary)) throw new Error(`kmp-mcp binary not found: ${binary}`);
  if (!fs.existsSync(chrome)) throw new Error(`Chrome not found: ${chrome}`);
  fs.writeFileSync(wirePath, "");
  captureEvidence.product_commit = process.env.KMP_CAMPAIGN_COMMIT || "unknown";
  captureEvidence.source_worktree_dirty = process.env.KMP_CAMPAIGN_WORKTREE_DIRTY === "true";
  captureEvidence.product_binary_sha256 = crypto
    .createHash("sha256")
    .update(fs.readFileSync(binary))
    .digest("hex");
  const version = spawnSync(binary, ["--version"], { encoding: "utf8" });
  if (version.status !== 0) throw new Error(`could not read product version: ${version.stderr}`);
  captureEvidence.product_version = version.stdout.trim();

  const catalog = new McpSession(null, "tool-list");
  try {
    const tools = await catalog.rpc("tools/list", {});
    captureEvidence.tool_count = tools.tools?.length;
    fs.writeFileSync(
      path.join(workRoot, "tools-list.json"),
      `${JSON.stringify(tools, null, 2)}\n`
    );
  } finally {
    await catalog.stop();
  }

  const selected = process.env.KMP_CAPTURE_SCENARIO || "all";
  const known = new Set(["all", "fresh-process", "two-processes", "wrong-turn"]);
  if (!known.has(selected)) throw new Error(`unknown KMP_CAPTURE_SCENARIO=${selected}`);
  const browser = await chromium.launch({
    executablePath: chrome,
    headless: true,
    args: ["--disable-extensions", "--disable-component-extensions-with-background-pages"],
  });
  try {
    if (selected === "all" || selected === "fresh-process") {
      await captureFreshProcessStory(browser);
    }
    if (selected === "all" || selected === "two-processes") {
      await captureTwoProcessesStory(browser);
    }
    if (selected === "all" || selected === "wrong-turn") {
      await captureAgentStory(browser);
    }
  } finally {
    await browser.close();
  }
  fs.writeFileSync(
    path.join(workRoot, "capture-evidence.json"),
    `${JSON.stringify(captureEvidence, null, 2)}\n`
  );
}

main().catch((error) => {
  console.error(error.stack || error);
  process.exitCode = 1;
});
