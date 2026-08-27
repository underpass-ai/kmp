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
    this.url = `http://127.0.0.1:${port}/`;
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
      const response = await fetch(`${url}api/info`);
      if (response.ok) return;
      last = new Error(`HTTP ${response.status}`);
    } catch (error) {
      last = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`viewer did not start: ${last}`);
}

async function openLoom(browser, url) {
  const context = await browser.newContext({
    // Capture oversized and downsample once: text stays crisp in the README,
    // while the complete agent explanation fits in the product's top bar.
    viewport: { width: 2000, height: 800 },
    colorScheme: "dark",
    deviceScaleFactor: 1,
    reducedMotion: "no-preference",
  });
  const page = await context.newPage();
  // ChronoLoom intentionally keeps one long-poll request in flight, so
  // networkidle would mean the product had stopped listening to the agent.
  await page.goto(url, { waitUntil: "domcontentloaded" });
  await page.waitForFunction(() => {
    const entries = document.getElementById("s-entries");
    const about = document.querySelector("#about-list .active");
    return entries && entries.textContent === "7" && about;
  });
  await page.waitForTimeout(700);
  return { context, page };
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
    await waitForViewer(session.url);
    await session.tool("kmp_ingest", memoryFixture());
    const { context, page } = await openLoom(browser, session.url);
    try {
      await page.waitForFunction(() =>
        document.getElementById("agent-chip-text").textContent === "human-controlled view"
      );
      await screenshot(page, "agent-01-idle");

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
      await page.waitForFunction(() => {
        const chip = document.getElementById("agent-chip-text");
        const selected = document.getElementById("d-id");
        return chip && chip.textContent.includes("show the memory behind this decision") &&
          selected && selected.textContent === "incident:cfg-change";
      });
      await page.waitForFunction(() => {
        const prism = [...document.querySelectorAll("#prism .prism-rail")].map((row) => row.textContent);
        return prism.some((text) => text.includes("03:12")) &&
          prism.some((text) => text.includes("09:40")) &&
          prism.some((text) => text.includes("09:41"));
      });
      await page.waitForTimeout(500);
      await screenshot(page, "agent-02-selection");

      await session.tool("kmp_view_apply_intent", {
        view_id: "default",
        expected_revision: await currentRevision(session),
        idempotency_key: "view:chronoloom-readme-trace-proof",
        actor: "agent",
        explanation: "light up the proof path",
        trace: { from: "incident:verified", to: "incident:cfg-change" },
      });
      await page.waitForFunction(() => {
        const chip = document.getElementById("agent-chip-text");
        const status = document.getElementById("trace-status");
        return chip && chip.textContent.includes("light up the proof path") &&
          status && status.textContent.startsWith("4 hops") &&
          document.querySelectorAll("#trace-hops > li").length === 4;
      });
      await page.locator("#trace-box").scrollIntoViewIfNeeded();
      await page.waitForTimeout(500);
      await screenshot(page, "agent-03-trace");
    } finally {
      await context.close();
    }
  } finally {
    await session.stop();
  }
}

async function captureClockStory(browser) {
  const session = new McpSession(17318, "clock-story");
  try {
    await waitForViewer(session.url);
    await session.tool("kmp_ingest", memoryFixture());
    const { context, page } = await openLoom(browser, session.url);
    try {
      await page.waitForFunction(() => {
        const active = document.querySelector("#clock-chips .chip.active");
        return active && active.dataset.clock === "occurred" &&
          document.getElementById("agent-chip-text").textContent === "human-controlled view";
      });
      await screenshot(page, "clocks-01-occurred");

      await session.tool("kmp_view_apply_intent", {
        view_id: "default",
        expected_revision: await currentRevision(session),
        idempotency_key: "view:chronoloom-readme-observed-clock",
        actor: "agent",
        explanation: "now show when the incident was understood",
        focus: {
          time_range: { axis: "observed" },
        },
        selection: null,
        trace: null,
      });
      await page.waitForFunction(() => {
        const active = document.querySelector("#clock-chips .chip.active");
        const chip = document.getElementById("agent-chip-text");
        return active && active.dataset.clock === "observed" &&
          chip && chip.textContent.includes("when the incident was understood");
      });
      await page.waitForTimeout(650);
      await screenshot(page, "clocks-02-observed");
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
    await captureClockStory(browser);
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  console.error(error.stack || error);
  process.exitCode = 1;
});
