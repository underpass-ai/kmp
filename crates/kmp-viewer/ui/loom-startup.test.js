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

function startup() {
  const reads = new Map([
    ["/api/info", deferred()],
    ["/api/abouts", deferred()],
    ["/api/view", deferred()],
  ]);
  const calls = [];
  const errors = [];
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
      dom: { showError: (message) => errors.push(message) },
      scene: { wire, applyTheme: wire, setup: async () => {} },
      panels: { wire, renderAbouts: wire, renderProvenance: wire },
      catalogue: { wire },
      evidence: { wire },
      timeControls: { wire },
      provenance: { wire },
      gestures: { wire },
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
  return { calls, errors, reads };
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
