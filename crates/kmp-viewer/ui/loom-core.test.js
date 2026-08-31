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

test("a whole-second cluster endpoint still covers the entry inside that second", () => {
  // #454: one entry at 03:12:33.731471Z. The episode projection reported it,
  // the endpoint was serialized as the second that contains it, and the
  // moment window derived from that endpoint ended at .002Z — before the
  // entry — so a populated about rendered 0/0.
  const extent = loom.projectionExtent({
    clusters: [
      { dimension: "agentic_episode", from: "2026-08-31T03:12:33Z", to: "2026-08-31T03:12:33Z", total: 1 },
    ],
    page: { total: 1 },
  });

  const entry = Date.parse("2026-08-31T03:12:33.731Z");
  assert.ok(extent.t0 <= entry, `window starts at ${extent.t0}, entry at ${entry}`);
  assert.ok(extent.t1 >= entry, `window ends at ${extent.t1}, entry at ${entry}`);
});

test("a fractional cluster endpoint covers what Date.parse truncates below the millisecond", () => {
  const extent = loom.projectionExtent({
    clusters: [
      {
        dimension: "agentic_episode",
        from: "2026-08-31T03:12:33.731471Z",
        to: "2026-08-31T03:12:33.731471Z",
        total: 1,
      },
    ],
  });

  assert.ok(extent.t0 <= Date.parse("2026-08-31T03:12:33.731Z"));
  assert.ok(extent.t1 >= Date.parse("2026-08-31T03:12:33.732Z"));
});

test("bins are the coarse fallback and clusters win when both are present", () => {
  const projection = {
    bins: [{ from: "2020-01-01T00:00:00Z", to: "2030-01-01T00:00:00Z" }],
    clusters: [{ from: "2026-08-31T03:12:33Z", to: "2026-08-31T03:12:34Z" }],
  };
  const withClusters = loom.projectionExtent(projection);
  assert.equal(withClusters.t0, Date.parse("2026-08-31T03:12:33Z"));

  const binsOnly = loom.projectionExtent({ bins: projection.bins });
  assert.equal(binsOnly.t0, Date.parse("2020-01-01T00:00:00Z"));
  assert.ok(binsOnly.t1 >= Date.parse("2030-01-01T00:00:00Z"));
});

test("a projection with no aggregates has no extent", () => {
  assert.equal(loom.projectionExtent({}), null);
  assert.equal(loom.projectionExtent({ clusters: [], bins: [] }), null);
});
