"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const source = fs.readFileSync(path.join(__dirname, "loom-core.js"), "utf8");
const context = vm.createContext({});
vm.runInContext(`${source}\nglobalThis.__KMP_LOOM__ = KMP_LOOM;`, context);
const loom = context.__KMP_LOOM__;

test("semantic zoom separates dense and sparse windows at the same time scale", () => {
  const msPerPx = 129_203;
  const width = 2_784;

  assert.equal(loom.lodFor(msPerPx, width, 400), "atlas");
  assert.equal(loom.lodFor(msPerPx, width, 4), "episode");
  assert.notEqual(
    loom.lodFor(msPerPx, width, 400),
    loom.lodFor(msPerPx, width, 4)
  );
});

test("a nearly empty long window does not use Atlas", () => {
  assert.equal(loom.lodFor(2_000_000, 2_784, 3), "episode");
});

test("projection density uses the busiest lane", () => {
  const projection = {
    clusters: [
      { dimension: "agentic_episode", total: 190 },
      { dimension: "agentic_episode", total: 210 },
      { dimension: "agentic_process", total: 399 },
      { dimension: "task", total: 400 },
    ],
    included_dimensions: ["agentic_episode", "agentic_process", "task"],
    page: { total: 1_200 },
  };

  assert.equal(loom.maxMarksPerLane(projection), 400);
});

test("page totals provide a safe density estimate for a body-only page", () => {
  const projection = {
    entries: [],
    included_dimensions: ["agentic_episode", "agentic_process", "task"],
    page: { total: 1_200 },
  };

  assert.equal(loom.maxMarksPerLane(projection), 400);
});
