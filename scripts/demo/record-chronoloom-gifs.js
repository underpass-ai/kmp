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
const { spawn } = require("node:child_process");
const { chromium } = require("playwright");

const root = path.resolve(__dirname, "../..");
const workRoot = process.env.KMP_CHRONOLOOM_CAPTURE_ROOT;
if (!workRoot) throw new Error("KMP_CHRONOLOOM_CAPTURE_ROOT is required");

const binary = process.env.KMP_MCP_BIN || path.join(root, "target/debug/kmp-mcp");
const chrome = process.env.KMP_CAPTURE_CHROME || "/usr/bin/google-chrome";
const frames = path.join(workRoot, "states");
fs.mkdirSync(frames, { recursive: true });

const ABOUT = "incident:pool-saturation";
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
          id: "incident:cfg-change",
          kind: "decision",
          text: "The deploy lowered max_connections from 200 to 20.",
          coordinates: [
            inTimeline(1, "2026-08-27T03:12:00Z", "2026-08-27T09:40:00Z", "2026-08-27T09:41:00Z"),
            inDecisions(1, "2026-08-27T03:12:00Z", "2026-08-27T09:40:00Z", "2026-08-27T09:41:00Z"),
          ],
        },
        {
          id: "incident:saturation",
          kind: "observation",
          text: "Connection checkout latency crossed two seconds.",
          coordinates: [
            inTimeline(2, "2026-08-27T09:38:00Z", "2026-08-27T09:38:20Z", "2026-08-27T09:39:00Z"),
          ],
        },
        {
          id: "incident:hypo-traffic",
          kind: "decision",
          text: "This looks like a traffic spike; scale the service.",
          coordinates: [
            inTimeline(3, "2026-08-27T09:41:00Z", "2026-08-27T09:41:00Z", "2026-08-27T09:41:20Z"),
            inDecisions(2, "2026-08-27T09:41:00Z", "2026-08-27T09:41:00Z", "2026-08-27T09:41:20Z"),
          ],
        },
        {
          id: "incident:volume-flat",
          kind: "observation",
          text: "Request volume had stayed flat all morning.",
          coordinates: [
            inTimeline(4, "2026-08-27T09:52:00Z", "2026-08-27T09:52:00Z", "2026-08-27T09:52:20Z"),
          ],
        },
        {
          id: "incident:root-cause",
          kind: "decision",
          text: "The pool ceiling caused the incident; restore max_connections to 200.",
          coordinates: [
            inTimeline(5, "2026-08-27T10:05:00Z", "2026-08-27T10:05:00Z", "2026-08-27T10:05:20Z"),
            inDecisions(3, "2026-08-27T10:05:00Z", "2026-08-27T10:05:00Z", "2026-08-27T10:05:20Z"),
          ],
        },
        {
          id: "incident:deploy-freeze",
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
          id: "incident:verified",
          kind: "success_path",
          text: "Checkout latency is back to 180 ms and the pool is healthy.",
          coordinates: [
            inTimeline(7, "2026-08-27T10:22:00Z", "2026-08-27T10:22:00Z", "2026-08-27T10:22:20Z"),
          ],
        },
      ],
      relations: [
        {
          from: "incident:saturation",
          to: "incident:cfg-change",
          rel: "depends_on",
          class: "causal",
          why: "A pool ceiling of 20 is what saturated under ordinary load.",
          evidence: "The ceiling was the only capacity change before checkout latency crossed two seconds.",
          confidence: "high",
        },
        {
          from: "incident:hypo-traffic",
          to: "incident:saturation",
          rel: "chosen_because",
          class: "motivational",
          why: "The saturation was read as demand before request volume was checked.",
          evidence: "The first mitigation proposed scaling while the dashboard still showed a flat request rate.",
          confidence: "high",
        },
        {
          from: "incident:volume-flat",
          to: "incident:hypo-traffic",
          rel: "contradicts",
          class: "evidential",
          why: "Flat request volume contradicts the traffic-spike hypothesis.",
          evidence: "The request-rate series stayed inside its morning baseline through the incident.",
          confidence: "high",
        },
        {
          from: "incident:root-cause",
          to: "incident:hypo-traffic",
          rel: "supersedes",
          class: "evidential",
          why: "The config change replaces the traffic-spike reading.",
          evidence: "The pool limit changed from 200 to 20 while traffic remained flat.",
          confidence: "high",
        },
        {
          from: "incident:deploy-freeze",
          to: "incident:root-cause",
          rel: "chosen_because",
          class: "motivational",
          why: "The team froze unrelated deploys so the pool restoration could be verified cleanly.",
          evidence: "The freeze began two minutes after the root cause was recorded.",
          confidence: "high",
        },
        {
          from: "incident:verified",
          to: "incident:root-cause",
          rel: "verified_by",
          class: "evidential",
          why: "Restoring the pool returned latency to baseline.",
          evidence: "Checkout latency fell to 180 ms after max_connections returned to 200.",
          confidence: "high",
        },
        {
          from: "incident:volume-flat",
          to: "incident:root-cause",
          rel: "supports",
          class: "evidential",
          why: "Stable demand isolates the configuration change as the relevant capacity shift.",
          evidence: "Request volume did not rise while pool wait time did.",
          confidence: "high",
        },
      ],
      evidence: [
        {
          id: "evidence:pool-config-diff",
          supports: ["incident:cfg-change", "incident:root-cause"],
          text: "The deployment diff changed max_connections from 200 to 20.",
          source: "deployment diff",
          time: "2026-08-27T03:12:00Z",
        },
        {
          id: "evidence:pool-latency",
          supports: ["incident:saturation", "incident:verified"],
          text: "Checkout latency crossed two seconds, then returned to 180 ms after restoration.",
          source: "pool telemetry",
          time: "2026-08-27T10:22:00Z",
        },
        {
          id: "evidence:request-volume",
          supports: ["incident:volume-flat", "incident:root-cause"],
          text: "Request volume stayed flat across the incident window.",
          source: "request-rate telemetry",
          time: "2026-08-27T09:52:00Z",
        },
      ],
    },
  };
}

class McpSession {
  constructor(port, name) {
    this.nextId = 1;
    this.pending = new Map();
    this.stderr = [];
    const dataDir = path.join(workRoot, "stores", name);
    fs.mkdirSync(dataDir, { recursive: true });
    this.origin = `http://127.0.0.1:${port}/`;
    this.child = spawn(binary, [], {
      cwd: root,
      env: {
        ...process.env,
        KMP_MCP_BACKEND: "embedded",
        KMP_MCP_ENGINE: "sqlite",
        KMP_MCP_DATA_DIR: dataDir,
        KMP_VIEWER_ADDR: `127.0.0.1:${port}`,
        RUST_LOG: "error",
      },
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.child.stderr.setEncoding("utf8");
    this.child.stderr.on("data", (chunk) => this.stderr.push(chunk));
    readline.createInterface({ input: this.child.stdout }).on("line", (line) => {
      let message;
      try {
        message = JSON.parse(line);
      } catch (_) {
        return;
      }
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

async function waitForLoom(loom) {
  await loom.waitForFunction(() => {
    const entries = document.getElementById("s-entries");
    const about = document.querySelector("#about-list .active");
    return entries && entries.textContent === "7" && about;
  });
  await loom.waitForTimeout(700);
}

function agentComposite() {
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <style>
    #capture-root { color-scheme: dark; background: #070910; color: #eef2ff; }
    #capture-root * { box-sizing: border-box; }
    body.capture-mode { margin: 0; overflow: hidden; }
    #capture-root {
      width: 100vw;
      height: 100vh;
      display: grid;
      grid-template-columns: 480px minmax(0, 1fr);
      font-family: Inter, ui-sans-serif, system-ui, sans-serif;
      background: radial-gradient(circle at 15% 10%, #171634 0, #070910 36%);
    }
    #capture-terminal {
      min-width: 0;
      margin: 14px 0 14px 14px;
      border: 1px solid #2d3354;
      border-radius: 16px 0 0 16px;
      background: linear-gradient(180deg, #10131f, #090b13);
      box-shadow: 0 20px 60px #0008;
      overflow: hidden;
      display: flex;
      flex-direction: column;
    }
    #capture-terminal .terminal-head {
      height: 58px;
      flex: none;
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 22px;
      border-bottom: 1px solid #252b45;
      color: #f4f5ff;
      font-size: 13px;
      font-weight: 750;
      letter-spacing: .12em;
    }
    #capture-terminal .brand span { color: #9587ff; }
    #capture-terminal .live { color: #79e5ad; font-size: 11px; }
    #capture-terminal .live::before {
      content: "";
      display: inline-block;
      width: 7px;
      height: 7px;
      margin-right: 8px;
      border-radius: 50%;
      background: #79e5ad;
      box-shadow: 0 0 14px #79e5ad99;
    }
    #capture-terminal .terminal-body {
      flex: 1;
      padding: 26px 24px;
      overflow: hidden;
      font: 16px/1.48 ui-monospace, "SFMono-Regular", Consolas, monospace;
    }
    #capture-terminal .phase { display: none; }
    #capture-root[data-phase="ready"] .phase[data-phase="ready"],
    #capture-root[data-phase="question"] .phase[data-phase="question"],
    #capture-root[data-phase="selection"] .phase[data-phase="selection"],
    #capture-root[data-phase="followup"] .phase[data-phase="followup"],
    #capture-root[data-phase="trace"] .phase[data-phase="trace"],
    #capture-root[data-phase="nice"] .phase[data-phase="nice"] { display: block; }
    #capture-terminal .shell { color: #6f7a9e; margin-bottom: 24px; }
    #capture-terminal .shell strong { color: #9ca6c7; font-weight: 500; }
    #capture-terminal .speaker { color: #79e5ad; font-size: 12px; letter-spacing: .08em; text-transform: uppercase; }
    #capture-terminal .prompt { margin: 8px 0 24px; color: #f4f6ff; font-size: 19px; line-height: 1.48; }
    #capture-terminal .chevron { color: #9587ff; font-weight: 800; }
    #capture-terminal .tool {
      margin-top: 18px;
      padding: 14px;
      border: 1px solid #363e67;
      border-radius: 11px;
      background: #181c2fd1;
      color: #cbd2e9;
      font-size: 14px;
    }
    #capture-terminal .tool-name { color: #67d8ff; font-weight: 750; }
    #capture-terminal .args {
      display: grid;
      grid-template-columns: 82px minmax(0, 1fr);
      gap: 5px 10px;
      margin-top: 12px;
      color: #c9d0e7;
      font-size: 13px;
      line-height: 1.4;
    }
    #capture-terminal .key { color: #7d88aa; }
    #capture-terminal .value { overflow-wrap: anywhere; }
    #capture-terminal .done { margin-top: 16px; color: #79e5ad; font-size: 14px; font-weight: 700; }
    #capture-terminal .arrow { color: #aa9fff; font-size: 19px; }
    #capture-terminal .prior {
      margin-bottom: 22px;
      padding-bottom: 18px;
      border-bottom: 1px solid #252b45;
      color: #8490b2;
      font-size: 13px;
    }
    #capture-terminal .prior strong { color: #bec6df; }
    #capture-terminal .ready {
      display: grid;
      place-items: center;
      min-height: 450px;
      text-align: center;
      color: #dfe3f5;
      font-size: 19px;
    }
    #capture-terminal .ready-mark {
      width: 52px;
      height: 52px;
      display: grid;
      place-items: center;
      margin: 0 auto 18px;
      border: 1px solid #564e92;
      border-radius: 15px;
      background: #6a58ff18;
      color: #aa9fff;
      font-size: 23px;
    }
    #capture-terminal .ready small { display: block; margin-top: 8px; color: #7d88aa; font-size: 13px; }
    #capture-terminal .terminal-foot {
      flex: none;
      display: flex;
      justify-content: space-between;
      padding: 15px 22px;
      border-top: 1px solid #252b45;
      color: #6f7a9e;
      font-size: 11px;
      letter-spacing: .1em;
      text-transform: uppercase;
    }
    #capture-terminal .terminal-foot strong { color: #aa9fff; }
    #capture-browser {
      min-width: 0;
      margin: 14px 14px 14px 0;
      border: 1px solid #2d3354;
      border-left: 0;
      border-radius: 0 16px 16px 0;
      overflow: hidden;
      background: #0b0d15;
      box-shadow: 0 20px 60px #0008;
    }
    #capture-browser #app { width: 100%; height: 100%; }
  </style>
</head>
<body>
  <aside id="capture-terminal">
    <header class="terminal-head">
      <div class="brand">CODEX <span>×</span> KMP</div>
      <div class="live">LIVE</div>
    </header>
    <main class="terminal-body">
      <section class="phase" data-phase="ready">
        <div class="shell">~/kmp <strong>on main</strong></div>
        <div class="ready"><div><div class="ready-mark">⌁</div>ChronoLoom is listening<small>Shared memory, live.</small></div></div>
      </section>
      <section class="phase" data-phase="question">
        <div class="shell">~/kmp <strong>on main</strong></div>
        <div class="speaker">you</div>
        <p class="prompt"><span class="chevron">›</span> Show me the memory behind this decision.</p>
      </section>
      <section class="phase" data-phase="selection">
        <div class="speaker">you</div>
        <p class="prompt"><span class="chevron">›</span> Show me the memory behind this decision.</p>
        <div class="tool">
          <span class="tool-name">kmp_view_apply_intent</span>
          <div class="args">
            <span class="key">selection</span><span class="value">incident:cfg-change</span>
            <span class="key">why</span><span class="value">show the memory behind this decision</span>
          </div>
        </div>
        <div class="done">✓ ChronoLoom updated <span class="arrow">→</span></div>
      </section>
      <section class="phase" data-phase="followup">
        <div class="prior">✓ Selected <strong>incident:cfg-change</strong></div>
        <div class="speaker">you</div>
        <p class="prompt"><span class="chevron">›</span> Great! Can you light up the proof path?</p>
      </section>
      <section class="phase" data-phase="trace">
        <div class="speaker">you</div>
        <p class="prompt"><span class="chevron">›</span> Great! Can you light up the proof path?</p>
        <div class="tool">
          <span class="tool-name">kmp_view_apply_intent</span>
          <div class="args">
            <span class="key">trace</span><span class="value">incident:verified → incident:cfg-change</span>
            <span class="key">why</span><span class="value">light up the proof path</span>
          </div>
        </div>
      </section>
      <section class="phase" data-phase="nice">
        <div class="prior">✓ 4-hop proof path rendered</div>
        <div class="speaker">you</div>
        <p class="prompt"><span class="chevron">›</span> Nice!</p>
      </section>
    </main>
    <footer class="terminal-foot"><span>Agent-directed</span><strong>Human-controlled</strong></footer>
  </aside>
  <main id="capture-browser"></main>
</body>
</html>`;
}

async function openAgentLoom(browser, url) {
  const context = await browser.newContext({
    viewport: { width: 2000, height: 800 },
    colorScheme: "dark",
    deviceScaleFactor: 1,
    reducedMotion: "no-preference",
    bypassCSP: true,
  });
  const page = await context.newPage();
  // ChronoLoom intentionally keeps one long-poll request in flight, so
  // networkidle would mean the product had stopped listening to the agent.
  await page.goto(url, { waitUntil: "domcontentloaded" });
  await waitForLoom(page);
  await page.evaluate((markup) => {
    const parsed = new DOMParser().parseFromString(markup, "text/html");
    const original = [...document.body.childNodes];
    const root = document.createElement("div");
    root.id = "capture-root";
    root.dataset.phase = "ready";
    const terminal = parsed.getElementById("capture-terminal");
    const browser = parsed.getElementById("capture-browser");
    if (!terminal || !browser) throw new Error("capture shell is incomplete");
    for (const node of original) browser.appendChild(node);
    root.append(terminal, browser);
    document.body.appendChild(root);
    document.body.classList.add("capture-mode");
    const style = parsed.querySelector("style");
    if (!style) throw new Error("capture shell has no stylesheet");
    document.head.appendChild(style);
  }, agentComposite());
  await page.waitForTimeout(300);
  return { context, page, loom: page };
}

async function setTerminalPhase(page, phase) {
  await page.locator("#capture-root").evaluate((root, next) => {
    root.dataset.phase = next;
  }, phase);
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

async function captureAgentStory(browser) {
  const session = new McpSession(17317, "agent-story");
  try {
    const viewerUrl = await session.viewerUrl();
    await waitForViewer(viewerUrl);
    await session.tool("kmp_ingest", memoryFixture());
    const { context, page, loom } = await openAgentLoom(browser, viewerUrl);
    try {
      await loom.waitForFunction(() =>
        document.getElementById("agent-chip-text").textContent === "human-controlled view"
      );
      await screenshot(page, "agent-01-ready");

      await setTerminalPhase(page, "question");
      await screenshot(page, "agent-02-question");

      await session.tool("kmp_view_apply_intent", {
        view_id: "default",
        expected_revision: await currentRevision(session),
        idempotency_key: "view:chronoloom-readme-select-cfg-change",
        actor: "agent",
        explanation: "show the memory behind this decision",
        focus: { refs: ["incident:cfg-change"] },
        projection: { semantic_zoom: "moment" },
        selection: "incident:cfg-change",
      });
      await loom.waitForFunction(() => {
        const chip = document.getElementById("agent-chip-text");
        const selected = document.getElementById("d-id");
        return chip && chip.textContent.includes("show the memory behind this decision") &&
          selected && selected.textContent === "incident:cfg-change";
      });
      await loom.waitForFunction(() => {
        const prism = [...document.querySelectorAll("#prism .prism-rail")].map((row) => row.textContent);
        return prism.some((text) => text.includes("03:12")) &&
          prism.some((text) => text.includes("09:40")) &&
          prism.some((text) => text.includes("09:41"));
      });
      await setTerminalPhase(page, "selection");
      await page.waitForTimeout(500);
      await screenshot(page, "agent-03-selection");

      await setTerminalPhase(page, "followup");
      await screenshot(page, "agent-04-followup");

      await session.tool("kmp_view_apply_intent", {
        view_id: "default",
        expected_revision: await currentRevision(session),
        idempotency_key: "view:chronoloom-readme-trace-proof",
        actor: "agent",
        explanation: "light up the proof path",
        trace: { from: "incident:verified", to: "incident:cfg-change" },
      });
      try {
        await loom.waitForFunction(() => {
          const chip = document.getElementById("agent-chip-text");
          const status = document.getElementById("trace-status");
          return chip && chip.textContent.includes("light up the proof path") &&
            status && status.textContent.startsWith("4 hops") &&
            document.querySelectorAll("#trace-hops > li").length === 4 &&
            Number(document.getElementById("s-entries")?.textContent) >= 5 &&
            Number(document.getElementById("s-relations")?.textContent) >= 4;
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
      await screenshot(page, "agent-05-trace");

      await loom.locator("#trace-box").scrollIntoViewIfNeeded();
      await setTerminalPhase(page, "nice");
      await page.waitForTimeout(500);
      await screenshot(page, "agent-06-nice");
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
  const browser = await chromium.launch({
    executablePath: chrome,
    headless: true,
    args: ["--disable-extensions", "--disable-component-extensions-with-background-pages"],
  });
  try {
    await captureAgentStory(browser);
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  console.error(error.stack || error);
  process.exitCode = 1;
});
