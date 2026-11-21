"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const deferred = () => {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};

function startup(options = {}) {
  const reads = new Map([
    ["/api/info", deferred()],
    ["/api/abouts", deferred()],
    ["/api/view", deferred()],
  ]);
  const calls = [];
  const errors = [];
  const notices = new Map();
  const elements = new Map();
  const wire = () => {};
  const context = vm.createContext({
    console,
    KMP_APP: {
      state: { model: { abouts: [] }, sync: {} },
      api: {
        call: (route) => {
          calls.push(route);
          return reads.get(route).promise;
        },
      },
      dom: {
        showError: (message) => errors.push(message),
        setNotice: (key, message) => notices.set(key, message),
        $: (id) => elements.get(id) || elements.set(id, { hidden: true, textContent: "" }).get(id),
      },
      scene: {
        wire,
        applyTheme: wire,
        setup: options.setup || (async () => {}),
        unavailable: (reason) => calls.push(`scene-unavailable:${reason}`),
      },
      provenance: { wire, idle: (owner, status) => calls.push(`idle:${owner}:${status}`) },
      panels: { wire, renderAbouts: wire, renderProvenance: wire },
      catalogue: { wire },
      evidence: { wire },
      timeControls: { wire },
      gestures: { wire },
      paths: { wire },
      data: { loadAbout: async () => {} },
      sync: {
        VIEW_ID: "default",
        applyAgentState: async (state) => calls.push(`apply:${state.about}`),
        startViewPolling: () => calls.push("poll"),
      },
    },
  });
  vm.runInContext(
    fs.readFileSync(path.join(__dirname, "loom.js"), "utf8"),
    context,
    { filename: "loom.js" },
  );
  return { calls, errors, reads, notices, elements };
}

test("startup begins its independent reads together and applies one authoritative state", async () => {
  const { calls, errors, reads } = startup();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls, ["/api/info", "/api/abouts", "/api/view"]);
  reads.get("/api/info").resolve({ kernel_version: "fixture" });
  reads.get("/api/abouts").resolve({ abouts: ["project:x"] });
  reads.get("/api/view").resolve({ about: "project:x", view_revision: 4 });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls.slice(3), ["apply:project:x", "poll"]);
  assert.deepEqual(errors, []);
});

test("an absent initial view does not hide an independent catalogue failure", async () => {
  const { errors, reads } = startup();
  await new Promise((resolve) => setImmediate(resolve));
  reads.get("/api/info").resolve({ kernel_version: "fixture" });
  reads.get("/api/view").reject(new Error("no aggregate"));
  reads.get("/api/abouts").reject(new Error("catalogue unavailable"));
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(errors, ["catalogue unavailable"]);
});

/* V-07: without WebGL only the scene is lost. The page still reads the
   catalogue and joins the view, and says for as long as it is true what is
   missing. */
test("a renderer that cannot start leaves everything else loading", async () => {
  const { calls, notices, reads, errors } = startup({
    setup: async () => {
      throw new Error("Error creating WebGL context.");
    },
  });
  await new Promise((resolve) => setImmediate(resolve));
  assert.ok(calls.includes("scene-unavailable:Error creating WebGL context."));
  assert.match(notices.get("renderer"), /WebGL/);
  assert.deepEqual(calls.filter((call) => call.startsWith("/api/")), ["/api/info", "/api/abouts", "/api/view"]);
  reads.get("/api/info").resolve({ kernel_version: "fixture" });
  reads.get("/api/abouts").resolve({ abouts: ["project:x"] });
  reads.get("/api/view").resolve({ about: "project:x", view_revision: 4 });
  await new Promise((resolve) => setImmediate(resolve));
  assert.ok(calls.includes("apply:project:x") && calls.includes("poll"));
  assert.deepEqual(errors, []);
});

test("an empty store is said until it changes, and the open store is named", async () => {
  const { calls, notices, reads, elements, errors } = startup();
  await new Promise((resolve) => setImmediate(resolve));
  reads.get("/api/info").resolve({ kernel_version: "fixture", data_dir: "/home/someone/.local/share/kmp" });
  reads.get("/api/abouts").resolve({ abouts: [] });
  reads.get("/api/view").resolve(null);
  await new Promise((resolve) => setImmediate(resolve));
  assert.match(notices.get("store"), /no memory yet/);
  assert.ok(calls.some((call) => call.startsWith("idle:")), "the control line stops saying it is joining");
  const store = elements.get("store-path");
  assert.equal(store.textContent, "/home/someone/.local/share/kmp");
  assert.equal(store.hidden, false);
  assert.deepEqual(errors, []);
});
