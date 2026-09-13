"use strict";

/* The application half of the loom under node: state, viewport, data and
   sync loaded into a bare context with the adapters stubbed out. The scene,
   panels and gestures are DOM and Pixi adapters — they are exercised by the
   HTTP smoke suite and the MCP App contracts, not here. */

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const MODULES = [
  "loom-core.js",
  "loom-state.js",
  "loom-api.js",
  "loom-viewport.js",
  "loom-data.js",
  "loom-selection.js",
  "loom-sync.js",
];

/* Loads the application modules into a fresh context. `stubs` become the
   adapter namespaces the modules reach late-bound (scene, panels, dom …),
   so a test can watch every effect without a browser. */
function loom(stubs = {}) {
  const calls = [];
  const spy =
    (name, result) =>
    (...args) => {
      calls.push({ name, args });
      return typeof result === "function" ? result(...args) : result;
    };
  const context = vm.createContext({
    setTimeout,
    clearTimeout,
    URLSearchParams,
    Date,
    console,
    Event: class Event {
      constructor(type) {
        this.type = type;
      }
    },
  });
  for (const name of MODULES) {
    const file = path.join(__dirname, name);
    vm.runInContext(fs.readFileSync(file, "utf8"), context, { filename: file });
  }
  const app = vm.runInContext("KMP_APP", context);
  const core = vm.runInContext("KMP_LOOM", context);
  const searchBox = {
    value: "",
    events: [],
    dispatchEvent(event) {
      this.events.push(event && event.constructor ? "input" : event);
    },
    blur() {},
  };
  const elements = new Map([["search", searchBox], ["trace-box", { hidden: true }]]);
  app.dom = {
    $: (id) => {
      if (!elements.has(id)) elements.set(id, { hidden: false, textContent: "" });
      return elements.get(id);
    },
    showError: spy("dom.showError"),
    el: () => ({ append() {}, style: {} }),
    fmtMs: () => "",
    fmtMsFull: () => "",
  };
  app.scene = {
    canvas: () => ({ clientWidth: 1000, clientHeight: 600 }),
    requestDraw: spy("scene.requestDraw"),
    drawNavigator: spy("scene.drawNavigator"),
    ...stubs.scene,
  };
  app.panels = {
    renderStats: spy("panels.renderStats"),
    renderRail: spy("panels.renderRail"),
    renderAbouts: spy("panels.renderAbouts"),
    renderPulseLegend: spy("panels.renderPulseLegend"),
    renderProvenance: spy("panels.renderProvenance"),
    renderDetailEmpty: spy("panels.renderDetailEmpty"),
    renderDiffPanel: spy("panels.renderDiffPanel"),
    renderPrism: spy("panels.renderPrism"),
    renderDetail: spy("panels.renderDetail"),
    renderTrace: spy("panels.renderTrace"),
    syncFocusButton: spy("panels.syncFocusButton"),
    syncClockChips: spy("panels.syncClockChips"),
    hideTraceBox: spy("panels.hideTraceBox"),
    renderChips: spy("panels.renderChips"),
    setSearch: (text) => {
      calls.push({ name: "panels.setSearch", args: [text] });
      searchBox.value = text;
      searchBox.events.push("input");
    },
    searchText: () => searchBox.value.trim(),
    ...stubs.panels,
  };
  if (stubs.api) app.api = { ...app.api, ...stubs.api };
  return { app, core, calls, searchBox, spy, context };
}

/* An entry pinned to one instant on the occurred clock. */
const entryAt = (core, ref, occurredAt) =>
  core.entryModel({
    ref_id: ref,
    kind: "decision",
    text: ref,
    coordinates: [{ dimension: "d", scope_id: "s", occurred_at: occurredAt }],
  });

function nodeBatchStub(app) {
  app.api.call = async (path, params) => {
    assert.equal(path, "/api/nodes");
    const entries = params.ids.split(",").map(id => app.state.model.byRef.get(id));
    return { snapshot: "fixture:1", nodes: entries.map(e => ({ id: e.ref, kind: e.kind, summary: e.text })),
      coordinates: Object.fromEntries(entries.map(e => [e.ref, e.coords.map(c => ({ dimension: c.dimension, scope_id: c.scope, occurred_at: c.occurred === null ? null : new Date(c.occurred).toISOString() }))])), missing: [], omitted: [], incomplete_coordinates: [] };
  };
}

test("clampWindow honors the extent and the one-second floor", () => {
  const { app } = loom();
  const full = { t0: 0, t1: 100000 };
  const same = (actual, expected) => assert.equal(JSON.stringify(actual), JSON.stringify(expected));
  same(app.state.clampWindow(full, -5000, 50), { t0: 0, t1: 1000 });
  same(app.state.clampWindow(full, 20000, 99999999), { t0: 20000, t1: 100000 });
  same(app.state.clampWindow(full, 10000, 30000), { t0: 10000, t1: 30000 });
});

test("entryAlpha lets the trace outrank a search miss", () => {
  const { app } = loom();
  const m = { ref: "r", kind: "decision" };
  assert.equal(app.state.entryAlpha(m), 1);
  app.state.view.searchHits = new Set(["other"]);
  assert.ok(app.state.entryAlpha(m) < 0.2, "a search miss fades");
  app.state.view.trace = { refs: new Set(["r"]) };
  assert.equal(app.state.entryAlpha(m), 1, "a traced entry stays legible");
});

test("setWindow clamps, remembers, redraws, reports and reloads", () => {
  const { app, calls } = loom();
  app.state.view.full = { t0: 0, t1: 100000 };
  app.state.view.t0 = 0;
  app.state.view.t1 = 100000;
  app.sync.reportView = () => calls.push({ name: "sync.reportView" });
  app.viewport.setWindow(-5000, 20000);
  assert.equal(app.state.view.t0, 0);
  assert.equal(app.state.view.t1, 20000);
  assert.equal(JSON.stringify(app.state.view.windowStack), JSON.stringify([[0, 100000]]));
  const names = calls.map((call) => call.name);
  for (const effect of ["panels.renderStats", "scene.requestDraw", "scene.drawNavigator", "sync.reportView"]) {
    assert.ok(names.includes(effect), `${effect} runs on a window move`);
  }
});

test("the backend port builds a query and refuses a kernel error", async () => {
  const loomInstance = loom();
  const { app } = loomInstance;
  const seen = [];
  // The api module reads its context's global fetch; stub it there.
  const { context } = loomInstance;
  context.fetch = async (url) => {
    seen.push(url);
    return { ok: true, json: async () => ({ fine: true }) };
  };
  const body = await app.api.call("/api/projection", { about: "a b", empty: "", skipped: null });
  assert.equal(body.fine, true);
  assert.equal(seen[0], "/api/projection?about=a%20b");
  context.fetch = async () => ({
    ok: false,
    status: 500,
    json: async () => ({ error: "the kernel said no" }),
  });
  await assert.rejects(() => app.api.call("/api/node", { id: "x" }), /the kernel said no/);
});

test("applyProjection classifies edges and refreshes every reader", () => {
  const { app, calls } = loom();
  app.state.view.full = { t0: 0, t1: 10 };
  app.data.applyProjection(
    {
      entries: [
        { ref_id: "a", kind: "decision", text: "a", coordinates: [{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T10:00:00Z" }] },
        { ref_id: "b", kind: "evidence", text: "b", coordinates: [{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T10:05:00Z" }] },
      ],
      relations: [
        { from: "b", to: "a", rel: "supports", class: "evidential" },
        { from: "b", to: "a", rel: "supersedes", class: "structural" },
      ],
      page: { total: 2 },
    },
    "moment"
  );
  const { model } = app.state;
  assert.equal(model.entries.length, 2);
  assert.equal(model.edges.length, 1, "supersedes is not an explanatory arc");
  assert.equal(model.supersessions.length, 1);
  assert.ok(model.supersededRefs.has("a"));
  const names = calls.map((call) => call.name);
  assert.ok(names.includes("panels.renderRail"));
  assert.ok(names.includes("scene.requestDraw"));
});

test("an agent snapshot moves clock, frames refs and applies the trace", async (t) => {
  const { app, core, calls } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: Date.parse("2026-09-01T00:00:00Z") };
  model.byRef = new Map([
    ["decision:new", entryAt(core, "decision:new", "2026-08-31T18:02:14Z")],
    ["success:old", entryAt(core, "success:old", "2026-08-31T16:50:00Z")],
  ]);
  app.data.loadProjection = async () => calls.push({ name: "data.loadProjection" });
  app.data.cancelScheduledProjection = () => {};
  app.selection.runTrace = async (options) => {
    calls.push({ name: "selection.runTrace", args: [options] });
    return false;
  };

  nodeBatchStub(app);
  await app.sync.applyAgentState({
    view_id: "default",
    view_revision: 42,
    about: "project:x",
    clock: "occurred",
    focus: { refs: ["decision:new", "success:old"] },
    projection: {},
    trace: { from: "decision:new", to: "success:old" },
    can_undo: true,
  });

  const framed = calls.find((call) => call.name === "data.loadProjection");
  assert.ok(framed, "a ref-only focus loads the framed projection atomically");
  const trace = calls.find((call) => call.name === "selection.runTrace");
  assert.equal(
    JSON.stringify(trace.args[0]),
    JSON.stringify({
      framePath: true,
      preserveWindow: false,
    }),
  );
  assert.equal(app.state.tracePick.from, "decision:new");
  assert.ok(view.t0 <= Date.parse("2026-08-31T16:50:00Z"));
  assert.ok(view.t1 >= Date.parse("2026-08-31T18:02:14Z"));
});

test("a cold snapshot adopts its clock before the extent and projects once", async () => {
  const { app } = loom();
  const asked = [];
  const entry = {
    ref_id: "project:x:observation:one",
    kind: "observation",
    text: "one",
    coordinates: [{
      dimension: "task",
      scope_id: "proof",
      observed_at: "2026-09-01T10:00:00Z",
    }],
  };
  app.api.fetchProjection = async (about, axis, from, to, lod) => {
    asked.push({ about, axis, from, to, lod });
    return lod === "episode"
      ? {
          level_of_detail: "episode",
          entries: [],
          bins: [],
          clusters: [{
            dimension: "task",
            total: 1,
            from: "2026-09-01T10:00:00Z",
            to: "2026-09-01T10:00:00Z",
          }],
          relations: [],
          page: { total: 1 },
        }
      : {
          level_of_detail: "moment",
          entries: [entry],
          bins: [],
          clusters: [],
          relations: [],
          page: { total: 1 },
        };
  };

  await app.sync.applyAgentState({
    about: "project:x",
    clock: "observed",
    focus: {},
    projection: {},
  });

  assert.deepEqual(asked.map(({ axis, lod }) => ({ axis, lod })), [
    { axis: "observed", lod: "episode" },
    { axis: "observed", lod: "moment" },
  ]);
  assert.equal(app.state.view.clock, "observed");
  assert.equal(app.state.model.currentLod, "moment");
  assert.deepEqual([...app.state.model.byRef.keys()], [entry.ref_id]);
});

test("a snapshot leaves an empty clock through one awaited replacement load", async () => {
  const { app } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.clock = "occurred";
  view.full = null;
  const asked = [];
  app.api.fetchProjection = async (about, axis, from, to, lod) => {
    asked.push({ axis, lod });
    return lod === "episode"
      ? {
          level_of_detail: "episode",
          entries: [],
          bins: [],
          clusters: [{
            dimension: "task",
            total: 1,
            from: "2026-09-01T10:00:00Z",
            to: "2026-09-01T10:00:00Z",
          }],
          relations: [],
          page: { total: 1 },
        }
      : {
          level_of_detail: "moment",
          entries: [{
            ref_id: "project:x:observation:one",
            kind: "observation",
            text: "one",
            coordinates: [{
              dimension: "task",
              scope_id: "proof",
              observed_at: "2026-09-01T10:00:00Z",
            }],
          }],
          bins: [],
          clusters: [],
          relations: [],
          page: { total: 1 },
        };
  };

  await app.sync.applyAgentState({
    about: "project:x",
    clock: "observed",
    focus: {},
    projection: { abouts: ["project:x"] },
  });
  await new Promise((resolve) => setImmediate(resolve));

  assert.deepEqual(asked, [
    { axis: "observed", lod: "episode" },
    { axis: "observed", lod: "moment" },
  ]);
  assert.equal(model.total, 1);
  assert.equal(model.currentLod, "moment");
  assert.equal(view.layerAbouts.length, 0, "the primary about is not its own layer");
});

test("focus and trace frame the authoritative proof path once", async () => {
  const { app, core, calls } = loom();
  const { model, view } = app.state;
  const refs = [
    "decision:new",
    "success:root",
    "evidence:middle",
    "focus:outside-path",
  ];
  model.about = "project:x";
  view.full = { t0: 0, t1: Date.parse("2026-09-01T00:00:00Z") };
  model.byRef = new Map(refs.map((ref, index) => [
    ref,
    entryAt(core, ref, `2026-08-31T18:0${index}:00Z`),
  ]));
  const nodeReads = [];
  app.api.call = async (path, params) => {
    if (path === "/api/trace") {
      return {
        nodes: [refs[0], refs[1], refs[2]].map((id) => ({ id })),
        edges: [{ source: refs[0], target: refs[2], rel: "supports" }],
      };
    }
    assert.equal(path, "/api/nodes");
    nodeReads.push(params.ids.split(","));
    const selected = params.ids.split(",");
    return {
      snapshot: "fixture:1",
      nodes: selected.map((id) => ({ id, kind: "observation", summary: id })),
      coordinates: Object.fromEntries(selected.map((id, index) => [id, [{
        dimension: "task",
        scope_id: "proof",
        occurred_at: `2026-08-31T18:0${index}:00Z`,
      }]])),
      missing: [],
      omitted: [],
      incomplete_coordinates: [],
      scanned_edges: 2,
    };
  };
  let projections = 0;
  app.data.loadProjection = async () => { projections += 1; };
  app.data.cancelScheduledProjection = () => {};

  await app.sync.applyAgentState({
    about: "project:x",
    clock: "occurred",
    focus: { refs: [refs[0], refs[3]] },
    trace: { from: refs[0], to: refs[1] },
    projection: {},
  });

  assert.deepEqual(
    nodeReads,
    [[refs[0], refs[1], refs[2]]],
    "collapsing the reads does not widen the final trace projection",
  );
  assert.equal(projections, 1);
  assert.ok(view.trace.refs.has(refs[2]));
  assert.ok(
    view.t1 < Date.parse("2026-08-31T20:00:00Z"),
    "a focus-only ref outside the path does not widen the final trace frame",
  );
  assert.ok(calls.some((call) => call.name === "panels.renderTrace"));
});

test("selection after a collapsed trace still reveals and centers an outside ref", async () => {
  const { app, core } = loom();
  const { model, view } = app.state;
  const from = "decision:from";
  const to = "success:to";
  const outside = "focus:selected-outside-path";
  const stamps = new Map([
    [from, "2026-08-31T18:00:00Z"],
    [to, "2026-08-31T18:01:00Z"],
    [outside, "2026-08-31T23:00:00Z"],
  ]);
  model.about = "project:x";
  view.full = {
    t0: Date.parse("2026-08-31T17:00:00Z"),
    t1: Date.parse("2026-09-01T00:00:00Z"),
  };
  model.byRef = new Map([...stamps].map(([ref, stamp]) => [
    ref,
    entryAt(core, ref, stamp),
  ]));
  app.api.call = async (path, params) => {
    if (path === "/api/trace") {
      return {
        nodes: [{ id: from }, { id: to }],
        edges: [{ source: from, target: to, rel: "supports" }],
      };
    }
    if (path === "/api/node") {
      return {
        node: { id: outside, kind: "observation", summary: outside },
        raw_coordinates: [{
          dimension: "task",
          scope_id: "proof",
          occurred_at: stamps.get(outside),
        }],
      };
    }
    assert.equal(path, "/api/nodes");
    const selected = params.ids.split(",");
    return {
      snapshot: "fixture:1",
      nodes: selected.map((id) => ({ id, kind: "observation", summary: id })),
      coordinates: Object.fromEntries(selected.map((id) => [id, [{
        dimension: "task",
        scope_id: "proof",
        occurred_at: stamps.get(id),
      }]])),
      missing: [],
      omitted: [],
      incomplete_coordinates: [],
      scanned_edges: 1,
    };
  };
  let projections = 0;
  app.data.loadProjection = async () => {
    projections += 1;
    const visible = projections === 1 ? [from, to] : [outside];
    model.byRef = new Map(visible.map((ref) => [
      ref,
      entryAt(core, ref, stamps.get(ref)),
    ]));
  };
  app.data.cancelScheduledProjection = () => {};

  await app.sync.applyAgentState({
    about: "project:x",
    clock: "occurred",
    focus: { refs: [from, outside] },
    trace: { from, to },
    selection: outside,
    projection: {},
  });

  assert.equal(projections, 2, "path frame plus the legacy outside-selection reveal");
  assert.equal(view.selectedRef, outside);
  assert.ok(view.t0 <= Date.parse(stamps.get(outside)));
  assert.ok(view.t1 >= Date.parse(stamps.get(outside)));
});

/* #463's regression: the browser reconciles the full snapshot instead of
   treating it as a patch. An agent-cleared search empties the input, and a
   ref-only focus clears a previous explicit focus range — the revision-42
   provenance banner and the rendered filters can no longer split. */
test("a snapshot with cleared facets clears the stale browser filters (#463)", async () => {
  const { app, core, calls, searchBox } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: Date.parse("2026-09-01T00:00:00Z") };
  view.focusRange = { from: 1, to: 2 };
  searchBox.value = "attempt-000005";
  model.byRef = new Map([["decision:new", entryAt(core, "decision:new", "2026-08-31T18:02:14Z")]]);
  app.data.loadProjection = async () => {};
  app.data.cancelScheduledProjection = () => {};

  nodeBatchStub(app);
  await app.sync.applyAgentState({
    view_revision: 42,
    about: "project:x",
    clock: "occurred",
    focus: { time_range: null, refs: ["decision:new"] },
    projection: {},
    search: null,
    can_undo: true,
  });

  assert.equal(searchBox.value, "", "an agent-cleared search empties the input");
  assert.equal(view.focusRange, null, "a ref-only focus clears the stale explicit range");
  assert.ok(
    calls.some((call) => call.name === "panels.syncFocusButton"),
    "the focus button reflects the cleared range"
  );
});

/* #463's second half: the cached extent probe may predate the refs an
   intent names. Framing must grow the known extent before the window is
   clamped, or the newer endpoint never enters a projection. */
test("frameRefs reaches refs ingested after the cached extent probe", async () => {
  const { app, core } = loom();
  const { model, view, sync } = app.state;
  model.about = "project:x";
  const oldStamp = Date.parse("2026-08-31T16:50:00Z");
  const newStamp = Date.parse("2026-08-31T18:02:14Z");
  // The probe ended before the bridge entry was ingested.
  view.full = { t0: oldStamp - 3600e3, t1: oldStamp + 600e3 };
  view.t0 = view.full.t0;
  view.t1 = view.full.t1;
  sync.applying = true; // reportView stays quiet, as during a real intent
  model.byRef = new Map([
    ["success:old", entryAt(core, "success:old", "2026-08-31T16:50:00Z")],
    ["decision:new", entryAt(core, "decision:new", "2026-08-31T18:02:14Z")],
  ]);
  app.data.loadProjection = async () => {};
  app.data.cancelScheduledProjection = () => {};

  nodeBatchStub(app);
  const framed = await app.sync.frameRefs(["decision:new", "success:old"]);

  assert.equal(framed, true);
  assert.ok(view.full.t1 > newStamp, "the known extent grew past the stale probe");
  assert.ok(view.t1 >= newStamp, "the window reaches the newer endpoint");
  assert.ok(view.t0 <= oldStamp, "and still holds the older one");
});

test("undoing to an unframed snapshot restores All and clears stale dimensions", async () => {
  const { app } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: 100000 };
  view.t0 = 20000;
  view.t1 = 30000;
  view.hiddenLanes = new Set(["topic"]);
  app.data.loadProjection = async () => {};
  await app.sync.applyAgentState({ about: model.about, clock: view.clock, focus: null, projection: {} });
  assert.equal(view.t0, 0);
  assert.equal(view.t1, 100000);
  assert.equal(view.hiddenLanes.size, 0);
});

test("an explicit agent search lands in the input and re-runs it", async () => {
  const { app, searchBox } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: 10 };
  app.data.loadProjection = async () => {};

  await app.sync.applyAgentState({
    view_revision: 7,
    about: "project:x",
    clock: "occurred",
    focus: {},
    projection: {},
    search: "kind:decision",
    can_undo: false,
  });
  assert.equal(searchBox.value, "kind:decision");
  assert.equal(searchBox.events.length, 1, "the input event re-runs the search");
});

test("frameRefs is honest when no ref carries the clock", async () => {
  const { app } = loom({
    api: { call: async () => ({ snapshot: "fixture:1", nodes: [{ id: "x", kind: "decision" }], coordinates: { x: [] } }) },
  });
  app.state.model.about = "project:x";
  app.state.view.full = { t0: 0, t1: 10 };
  const framed = await app.sync.frameRefs(["unclocked"]);
  assert.equal(framed, false, "nothing to frame is reported, not invented");
});

/* ---------------- viewport ---------------- */

test("setClock resets the window, syncs the chips and reports", () => {
  const { app, calls } = loom();
  const { view } = app.state;
  view.full = { t0: 0, t1: 100000 };
  view.t0 = 10;
  view.t1 = 20;
  app.sync.reportView = () => calls.push({ name: "sync.reportView" });
  app.data.scheduleProjection = () => calls.push({ name: "data.scheduleProjection" });
  app.viewport.setClock("observed", true);
  assert.equal(view.clock, "observed");
  assert.equal(view.t0, 0);
  assert.equal(view.t1, 100000);
  const names = calls.map((call) => call.name);
  assert.ok(names.includes("panels.syncClockChips"));
  assert.ok(names.includes("sync.reportView"));
  assert.ok(names.includes("data.scheduleProjection"));
});

test("a clock change on an empty extent re-probes the about instead of trapping", () => {
  const { app, calls } = loom();
  app.state.model.about = "project:x";
  app.state.view.full = null;
  app.state.view.clock = "occurred";
  app.data.loadAbout = (about, announce) => calls.push({ name: "data.loadAbout", args: [about, announce] });
  app.viewport.setClock("ingested", false);
  const probe = calls.find((call) => call.name === "data.loadAbout");
  assert.ok(probe, "the empty state is not a trap (#421)");
  assert.equal(probe.args[1], false, "a re-probe does not announce");
  // The same clock again has nothing to re-probe: the message speaks.
  app.state.view.clock = "ingested";
  app.viewport.setClock("ingested", false);
  assert.ok(calls.some((call) => call.name === "dom.showError"));
});

test("a zoom rung is a density, applied around the window's center", () => {
  const { app } = loom();
  const { view } = app.state;
  view.full = { t0: 0, t1: 1e10 };
  view.t0 = 4e9;
  view.t1 = 6e9;
  app.sync.reportView = () => {};
  app.viewport.applyZoomRung("moment");
  const span = view.t1 - view.t0;
  assert.equal(span, 8000 * 1000, "moment asks for 8s/px across a 1000px stage");
  const centre = (view.t0 + view.t1) / 2;
  assert.equal(centre, 5e9);
  const before = { t0: view.t0, t1: view.t1 };
  app.viewport.applyZoomRung("panorama");
  assert.deepEqual({ t0: view.t0, t1: view.t1 }, before, "an unknown rung is a no-op");
});

test("centerOn moves the window only when the entry is outside it", () => {
  const { app, core } = loom();
  const { model, view } = app.state;
  const at = Date.parse("2026-08-31T18:00:00Z");
  model.byRef = new Map([["decision:new", entryAt(core, "decision:new", "2026-08-31T18:00:00Z")]]);
  view.full = { t0: at - 1e7, t1: at + 1e7 };
  view.t0 = at - 5e6;
  view.t1 = at - 4e6;
  app.sync.reportView = () => {};
  app.viewport.centerOn("decision:new");
  assert.ok(view.t0 <= at && at <= view.t1, "the entry is brought into view");
  const held = { t0: view.t0, t1: view.t1 };
  app.viewport.centerOn("decision:new");
  assert.deepEqual({ t0: view.t0, t1: view.t1 }, held, "an in-window entry does not move it");
});

/* ---------------- data ---------------- */

test("loadProjection re-fetches when the projection's density resolves another rung", async () => {
  const { app, calls } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  model.maxMarksPerLane = 0;
  view.full = { t0: 0, t1: 200000 };
  view.t0 = 0;
  view.t1 = 200000;
  app.sync.reportView = () => {};
  const fetched = [];
  app.api = {
    ...app.api,
    fetchProjection: async (about, axis, from, to, lod) => {
      fetched.push(lod);
      return {
        entries: [],
        bins: Array.from({ length: 200 }, (_, i) => ({ dimension: "d", total: 50, from: "2026-08-31T00:00:00Z", to: "2026-08-31T01:00:00Z" })),
        clusters: [],
        relations: [],
        page: { total: 10000 },
        truncated: true,
      };
    },
  };
  await app.data.loadProjection();
  assert.ok(fetched.length >= 1);
  const error = calls.find((call) => call.name === "dom.showError" && String(call.args[0]).includes("projection is partial"));
  assert.ok(error, "a truncated projection says so");
});

test("the applied projection owns its resolved level of detail", async () => {
  const { app } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: 200000 };
  view.t0 = 0;
  view.t1 = 200000;
  view.requestedLod = "moment";
  app.api.fetchProjection = async () => ({
    level_of_detail: "episode",
    entries: [],
    bins: [],
    clusters: [],
    relations: [],
    page: { total: 0 },
  });

  await app.data.loadProjection();

  assert.equal(model.currentLod, "episode");
});

test("loadObservability names unavailable series and gives lanes back on failure", async () => {
  const { app, calls } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: 10 };
  app.api = {
    ...app.api,
    call: async () => ({ series: [{ name: "noise_ratio", points: [] }], exemplars: [], missing: ["projection_lag"] }),
  };
  await app.data.loadObservability(["noise_ratio", "projection_lag"]);
  assert.ok(
    calls.some((call) => call.name === "dom.showError" && String(call.args[0]).includes("telemetry series unavailable")),
    "missing series are reported"
  );
  app.api = {
    ...app.api,
    call: async () => {
      throw new Error("journal locked");
    },
  };
  await app.data.loadObservability(["noise_ratio"]);
  assert.equal(view.overlays.length, 0, "a failed pulse gives the lanes their space back");
});

test("loadAbout probes the extent, resets the view and announces once", async () => {
  const { app, calls } = loom();
  const { model, view, sync } = app.state;
  view.clock = "occurred";
  view.selectedRef = "stale";
  view.focusRange = { from: 1, to: 2 };
  app.api = {
    ...app.api,
    fetchProjection: async () => ({
      entries: [],
      bins: [],
      clusters: [{ from: "2026-08-31T10:00:00Z", to: "2026-08-31T11:00:00Z", dimension: "d", total: 3 }],
      relations: [],
      page: { total: 3 },
    }),
  };
  app.data.loadProjection = async () => calls.push({ name: "data.loadProjection" });
  app.sync.viewOpen = async () => calls.push({ name: "sync.viewOpen" });
  app.sync.reportView = () => {};
  await app.data.loadAbout("project:x");
  assert.equal(model.about, "project:x");
  assert.ok(view.full, "the probe framed an extent");
  assert.equal(view.selectedRef, null, "a fresh about clears the selection");
  assert.equal(view.focusRange, null);
  assert.equal(sync.applying, false);
  assert.ok(calls.some((call) => call.name === "sync.viewOpen"), "the open is announced");
});

test("an about with nothing on this clock renders an explicit empty state", async () => {
  const { app, calls } = loom();
  const { model, view } = app.state;
  app.api = {
    ...app.api,
    fetchProjection: async () => ({ entries: [], bins: [], clusters: [], relations: [], page: { total: 0 } }),
  };
  app.sync.viewOpen = async () => calls.push({ name: "sync.viewOpen" });
  app.sync.reportView = () => {};
  await app.data.loadAbout("project:empty");
  assert.equal(model.about, "project:empty", "the clicked about becomes current even when empty (#421)");
  assert.equal(view.full, null);
  assert.ok(!calls.some((call) => call.name === "sync.viewOpen"), "nothing is announced without a range");
});

test("an empty clock preserves the probe's missing-time evidence", async () => {
  const { app } = loom();
  const { model, view } = app.state;
  const metrics = [{ name: "missing_axis_entries", value: 3, scope: "selected_source" }];
  const missing = ["temporal_positions"];
  view.clock = "occurred";
  app.api = {
    ...app.api,
    fetchProjection: async () => ({ entries: [], bins: [], clusters: [], relations: [],
      page: { total: 0 }, metrics, missing }),
  };
  app.sync.reportView = () => {};
  await app.data.loadAbout("project:no-occurrence");
  assert.equal(view.full, null);
  assert.equal(model.total, 0);
  assert.deepEqual(model.projection.metrics, metrics);
  assert.deepEqual(model.projection.missing, missing);
});

/* ---------------- selection & trace ---------------- */

test("selecting a projected entry inspects it and renders the evidence", async () => {
  const { app, core, calls } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  model.byRef = new Map([["decision:new", entryAt(core, "decision:new", "2026-08-31T18:00:00Z")]]);
  app.api = {
    ...app.api,
    call: async (path) => {
      assert.equal(path, "/api/node");
      return { node: { id: "decision:new", kind: "decision" } };
    },
  };
  app.sync.reportView = () => {};
  const selected = await app.selection.selectEntry("decision:new");
  assert.equal(selected, true);
  assert.equal(view.selectedRef, "decision:new");
  const names = calls.map((call) => call.name);
  assert.ok(names.includes("panels.renderPrism"));
  assert.ok(names.includes("panels.renderDetail"));
});

test("selection reaches newly written memory beyond the cached extent without dropping filters", async () => {
  for (const [clock, stamp, included] of [
    ["observed", "2026-09-01T10:05:00Z", true],
    ["occurred", "2026-09-01T08:05:00Z", true],
    ["observed", "2026-09-01T10:05:00Z", false],
  ]) {
    const { app, calls } = loom();
    const { model, view } = app.state;
    model.about = "project:x";
    const instant = Date.parse(stamp);
    Object.assign(view, {
      clock, full: { t0: Date.parse("2026-09-01T09:00:00Z"), t1: Date.parse("2026-09-01T09:30:00Z") },
      t0: Date.parse("2026-09-01T09:00:00Z"), t1: Date.parse("2026-09-01T09:30:00Z"),
      requestedLod: "moment", selectedRef: "decision:old",
      selectors: [{ key: "review", op: "in", values: ["HND-9"] }],
    });
    const record = { ref_id: "feedback:new", kind: "feedback", text: "Selection confirmed; test still pending",
      coordinates: [{ dimension: "review", scope_id: "HND-9", [clock + "_at"]: stamp }] };
    app.api.call = async (path, params) => {
      assert.equal(path, "/api/node");
      assert.equal(params.about, "project:x");
      assert.equal(params.id, record.ref_id);
      return { node: { id: record.ref_id, kind: record.kind, summary: record.text }, raw_coordinates: record.coordinates };
    };
    let reached = false;
    app.api.fetchProjection = async (about, axis, from, to, lod, bins, labels) => {
      assert.equal(about, "project:x");
      assert.equal(axis, clock);
      assert.equal(labels, "review in HND-9");
      reached = Date.parse(from) <= instant && instant < Date.parse(to);
      const entries = reached && included ? [record] : [];
      return { entries, relations: [], page: { total: entries.length } };
    };
    app.sync.reportView = () => {};

    assert.equal(await app.selection.selectEntry(record.ref_id), included);
    assert.ok(reached, "the real clamped window must reach the inspected clock");
    assert.equal(view.selectedRef, included ? record.ref_id : "decision:old");
    assert.equal(view.clock, clock);
    assert.equal(model.about, "project:x");
    if (!included) assert.ok(calls.some(call => call.name === "dom.showError" &&
      String(call.args[0]).includes("not present in the Moment projection")), "a filtered target remains unavailable");
  }
});

test("relation links reveal entries outside the projection and center only successful selections", async () => {
  for (const [projected, accepted] of [[false, true], [true, true], [false, false]]) {
    const { app, context } = loom();
    const elements = new Map();
    const element = (tag) => ({
      tag, children: [], listeners: {}, style: {}, attributes: {},
      append(...children) { this.children.push(...children); },
      addEventListener(event, handler) { this.listeners[event] = handler; },
      setAttribute(name, value) { this.attributes[name] = value; },
    });
    const get = (id) => {
      if (!elements.has(id)) elements.set(id, element("div"));
      return elements.get(id);
    };
    context.document = {
      getElementById: get,
      createElement: element,
      createTextNode: (text) => ({ textContent: text }),
    };
    app.scene.kindColor = app.scene.classColor = () => "#888";
    if (projected) app.state.model.byRef.set("later", { ref: "later" });
    const effects = [];
    app.selection.selectEntry = async (ref) => { effects.push(["select", ref]); return accepted; };
    app.viewport.centerOn = (ref) => effects.push(["center", ref]);
    const file = path.join(__dirname, "loom-panels.js");
    vm.runInContext(fs.readFileSync(file, "utf8"), context, { filename: file });
    app.panels.renderDetail({
      node: { id: "earlier", kind: "decision", title: "Earlier decision" },
      incoming: [{ source: "later", target: "earlier", rel: "supersedes", class: "evidential" }],
      outgoing: [],
    }, { ref: "earlier", coords: [] });
    const findLink = (node) => node.tag === "a" ? node : (node.children || []).map(findLink).find(Boolean);
    const link = findLink(get("d-incoming"));
    assert.ok(link, "the rendered relation exposes its actual counterpart");
    await link.listeners.click();
    assert.deepEqual(effects, accepted ? [["select", "later"], ["center", "later"]] : [["select", "later"]]);
  }
});

test("a projected additional-about entry is selected without changing owner or explicit window", async () => {
  const { app, calls, context } = loom();
  vm.runInContext(fs.readFileSync(path.join(__dirname, "loom-layers.js"), "utf8"), context);
  const { model, view } = app.state;
  model.about = "support";
  Object.assign(view, {layerAbouts: ["project"], requestedLod: "moment", clock: "occurred",
    full: {t0: 0, t1: 10000}, t0: 0, t1: 10000});
  let reads = 0;
  app.api.fetchProjection = async (about) => {
    reads++;
    return {entries: [{ref_id: about === "support" ? "outage" : "recovery",
      kind: about === "support" ? "observation" : "success_path", text: about,
      coordinates: [{dimension:"incident", scope_id:"INC-17", occurred_at:"1970-01-01T00:00:05Z"}]}],
      page:{total:1}, relations:[]};
  };
  app.api.call = async (path, params) => {
    assert.equal(path, "/api/node");
    assert.equal(params.about, "project", "inspect uses the returned projection's owner, not a parsed ref");
    assert.equal(params.id, "recovery");
    return {node:{id:"recovery", kind:"success_path", summary:"restored"}};
  };
  app.sync.reportView = () => {};
  await app.data.loadProjection();
  assert.equal(model.entries.length, 2);
  assert.equal(model.total, 2);
  assert.equal(model.byRef.get("recovery").about, "project");
  assert.equal(await app.selection.selectEntry("recovery"), true);
  assert.equal(model.about, "support");
  assert.equal(view.selectedRef, "recovery");
  assert.equal(view.t0, 0);
  assert.equal(view.t1, 10000);
  assert.equal(reads, 2, "no failed reveal or unintended reframe");
  assert.ok(calls.some(call => call.name === "panels.renderDetail"));
  view.layerAbouts = [];
  await app.data.loadProjection();
  assert.equal(model.entries.length, 1);
  assert.equal(model.total, 1);
  assert.equal(model.byRef.has("recovery"), false, "removed owners are not kept in the selectable index");
});

test("revealing a ref without temporal coordinates is refused, not faked", async () => {
  const { app, calls } = loom();
  app.state.model.about = "project:x";
  app.state.view.full = { t0: 0, t1: 10 };
  app.api = {
    ...app.api,
    call: async () => ({ node: { id: "x", kind: "decision" }, raw_coordinates: [] }),
  };
  const selected = await app.selection.selectEntry("unclocked");
  assert.equal(selected, false);
  assert.ok(
    calls.some((call) => call.name === "dom.showError" && String(call.args[0]).includes("carries no temporal coordinate"))
  );
});

test("runTrace frames its path, keeps the hops and reports", async () => {
  const { app, calls } = loom();
  const { view, tracePick } = app.state;
  app.state.model.about = "project:x";
  tracePick.from = "decision:new";
  tracePick.to = "success:old";
  app.api = {
    ...app.api,
    call: async (path) => {
      assert.equal(path, "/api/trace");
      return {
        nodes: [{ id: "decision:new" }, { id: "bridge" }, { id: "success:old" }],
        edges: [{ source: "decision:new", rel: "led_to", target: "bridge" }],
        rendered: { token_count: 12 },
      };
    },
  };
  app.sync.frameRefs = async (refs) => {
    calls.push({ name: "sync.frameRefs", args: [refs] });
    return true;
  };
  app.sync.reportView = () => calls.push({ name: "sync.reportView" });
  await app.selection.runTrace({ framePath: true });
  assert.ok(view.trace.refs.has("bridge"));
  assert.ok(view.trace.edgeKeys.has("decision:new led_to bridge"));
  const names = calls.map((call) => call.name);
  assert.ok(names.includes("sync.frameRefs"));
  assert.ok(names.includes("panels.renderTrace"));
  assert.ok(names.includes("sync.reportView"));
});

/* ---------------- sync ---------------- */

test("viewOpen adopts the aggregate's revision and starts the long poll", async () => {
  const { app, calls } = loom();
  app.state.model.about = "project:x";
  app.api = {
    ...app.api,
    call: async (path) => {
      if (path === "/api/view/open") return { view_revision: 7, last_change: { actor: "human" } };
      // Park the long poll forever; one iteration is not the loop's test.
      return new Promise(() => {});
    },
  };
  await app.sync.viewOpen();
  assert.equal(app.state.sync.revision, 7);
  assert.equal(app.state.sync.polling, true);
  assert.ok(calls.some((call) => call.name === "panels.renderProvenance"));
});

test("a view-sync failure keeps the loom drawing", async () => {
  const { app, calls } = loom();
  app.state.model.about = "project:x";
  app.api = {
    ...app.api,
    call: async () => {
      throw new Error("no aggregate here");
    },
  };
  await app.sync.viewOpen();
  assert.ok(
    calls.some((call) => call.name === "dom.showError" && String(call.args[0]).includes("view sync unavailable"))
  );
});

test("an explicit agent range is the exact shared brush window", async () => {
  const { app, calls } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: Date.parse("2026-09-01T00:00:00Z") };
  app.data.loadObservability = async (series) => calls.push({ name: "data.loadObservability", args: [series] });
  model.lanes = [{ name: "keep" }, { name: "drop" }];
  await app.sync.applyAgentState({
    view_revision: 9,
    about: "project:x",
    clock: "occurred",
    focus: { time_range: { from: "2026-08-31T16:49:00Z", to: "2026-08-31T17:39:00Z" } },
    projection: { overlays: ["noise_ratio"], dimensions: ["keep"], semantic_zoom: "moment" },
    can_undo: true,
  });
  assert.equal(view.focusRange, null);
  assert.equal(view.t0, Date.parse("2026-08-31T16:49:00Z"));
  assert.ok(view.hiddenLanes.has("drop"), "a keep-list hides the other lanes");
  assert.ok(!view.hiddenLanes.has("keep"));
  assert.ok(calls.some((call) => call.name === "data.loadObservability"));
  // The intent named its own window; the rung is a fallback, not an override.
  assert.equal(view.t1, Date.parse("2026-08-31T17:39:00Z"));
});

test("an agent selection is selected and centered", async () => {
  const { app, calls } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: 10 };
  app.selection.selectEntry = async (ref) => {
    calls.push({ name: "selection.selectEntry", args: [ref] });
    return true;
  };
  await app.sync.applyAgentState({
    view_revision: 3,
    about: "project:x",
    clock: "occurred",
    focus: {},
    projection: {},
    selection: "decision:new",
    can_undo: true,
  });
  const selected = calls.find((call) => call.name === "selection.selectEntry");
  assert.deepEqual(selected.args, ["decision:new"]);
});

test("reportView posts once per distinct signature", async () => {
  const loomInstance = loom();
  const { app, calls, context } = loomInstance;
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: 10 };
  view.t0 = 0;
  view.t1 = 10;
  // Collapse the debounce: run the report immediately.
  context.setTimeout = (fn) => {
    fn();
    return 0;
  };
  context.clearTimeout = () => {};
  const posted = [];
  app.api = {
    ...app.api,
    call: async (path, params) => {
      posted.push(params);
      return { view_revision: 11 };
    },
  };
  app.sync.reportView();
  await new Promise((resolve) => setImmediate(resolve));
  app.sync.reportView();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(posted.length, 1, "an identical report is not repeated");
  assert.equal(app.state.sync.revision, 11);
  assert.equal(posted[0].about, "project:x");
});

test("undoing an agent move applies the stepped-back snapshot", async () => {
  const { app, calls } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: 10 };
  app.api = {
    ...app.api,
    call: async (path) => {
      assert.equal(path, "/api/view/undo");
      return { view_revision: 4, about: "project:x", clock: "occurred", focus: {}, projection: {}, can_undo: false };
    },
  };
  await app.sync.undoAgentMove();
  assert.equal(app.state.sync.revision, 4);
  assert.ok(calls.some((call) => call.name === "panels.renderProvenance"));
});

/* ---------------- label selectors ---------------- */

test("a snapshot's labels become the loom's selectors and re-probe the about", async () => {
  const { app, calls } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: 10 };
  app.data.loadAbout = async (about, announce) => calls.push({ name: "data.loadAbout", args: [about, announce] });
  await app.sync.applyAgentState({
    view_revision: 5,
    about: "project:x",
    clock: "occurred",
    focus: {},
    projection: { labels: [{ key: "task", op: "in", values: ["launch", "launch"] }, { key: "incident", op: "notexists" }] },
    can_undo: true,
  });
  assert.equal(
    JSON.stringify(view.selectors),
    JSON.stringify([{ key: "incident", op: "notexists", values: [] }, { key: "task", op: "in", values: ["launch"] }])
  );
  const reload = calls.find((call) => call.name === "data.loadAbout");
  assert.ok(reload, "a changed filter asks the kernel again");
  assert.equal(reload.args[1], false, "without announcing a fresh open");
  assert.ok(calls.some((call) => call.name === "panels.renderChips"));
  calls.length = 0;
  await app.sync.applyAgentState({
    view_revision: 6,
    about: "project:x",
    clock: "occurred",
    focus: {},
    projection: { labels: [{ key: "incident", op: "notexists" }, { key: "task", op: "in", values: ["launch"] }] },
    can_undo: true,
  });
  assert.ok(!calls.some((call) => call.name === "data.loadAbout"), "the same filter in another order is the same filter");
});

test("the projection is asked through the selectors and the report carries them", async () => {
  const loomInstance = loom();
  const { app, context } = loomInstance;
  const { model, view } = app.state;
  model.about = "project:x";
  view.full = { t0: 0, t1: 200000 };
  view.t0 = 0;
  view.t1 = 200000;
  view.selectors = [{ key: "task", op: "in", values: ["launch", "other"] }];
  app.sync.reportView = () => {};
  const asked = [];
  app.api = {
    ...app.api,
    fetchProjection: async (about, axis, from, to, lod, bins, labels) => {
      asked.push(labels);
      return { entries: [], bins: [], clusters: [], relations: [], page: { total: 0 } };
    },
  };
  await app.data.loadProjection();
  assert.equal(asked[0], "task in launch|other");

  context.setTimeout = (fn) => {
    fn();
    return 0;
  };
  context.clearTimeout = () => {};
  const posted = [];
  app.api = { ...app.api, call: async (path, params) => (posted.push(params), { view_revision: 2 }) };
  app.sync.reportView = Object.getPrototypeOf(app.sync).reportView || app.sync.reportView;
  // reportView was replaced above for loadProjection; rebuild a fresh loom to read the real one.
  const fresh = loom();
  fresh.context.setTimeout = (fn) => {
    fn();
    return 0;
  };
  fresh.context.clearTimeout = () => {};
  fresh.app.state.model.about = "project:x";
  fresh.app.state.view.full = { t0: 0, t1: 10 };
  fresh.app.state.view.selectors = [{ key: "incident", op: "notexists", values: [] }];
  const reported = [];
  fresh.app.api = { ...fresh.app.api, call: async (path, params) => (reported.push(params), { view_revision: 3 }) };
  fresh.app.sync.reportView();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(reported[0].labels, "incident notexists");
});

test("setSelectors normalizes, redraws the chips, re-probes and reports", async () => {
  const { app, calls } = loom();
  const { model, view } = app.state;
  model.about = "project:x";
  app.data.loadAbout = async (about, announce) => calls.push({ name: "data.loadAbout", args: [about, announce] });
  app.sync.reportView = () => calls.push({ name: "sync.reportView" });
  await app.data.setSelectors([{ key: "task", op: "like", values: ["x"] }, { key: "task", op: "exists" }]);
  assert.equal(JSON.stringify(view.selectors), JSON.stringify([{ key: "task", op: "exists", values: [] }]));
  const names = calls.map((call) => call.name);
  assert.ok(names.includes("panels.renderChips"));
  assert.ok(names.includes("data.loadAbout"));
  assert.ok(names.includes("sync.reportView"));
});

test("changing a populated clock re-probes its own extent before reporting", async () => {
  const {app,calls}=loom();
  app.state.model.about="project:clocks";
  app.state.view.full={t0:0,t1:1000};
  app.data.loadAbout=async (about,announce)=>{calls.push({name:"clock.probe",about,announce});};
  app.sync.reportView=()=>calls.push({name:"clock.report"});
  await app.viewport.setClock("observed",false);
  assert.deepEqual(calls.filter(call=>call.name.startsWith("clock.")).map(call=>call.name),["clock.probe","clock.report"]);
});

test("focus uses one fresh batch even for cached refs and never frames incomplete coordinates",async()=>{
  const {app,core,calls}=loom();
  const {model,view}=app.state;
  model.about="project:x"; view.clock="observed"; view.full={t0:0,t1:Date.parse("2026-10-01")};
  model.byRef.set("a",entryAt(core,"a","2020-01-01T00:00:00Z"));
  let reads=0;
  app.api.call=async(path,params)=>{
    reads++;
    assert.equal(path,"/api/nodes"); assert.equal(params.ids,"a,b");
    return {snapshot:"fixture:1",nodes:[{id:"a"},{id:"b"}],coordinates:{a:[{observed_at:"2026-09-13T10:00:00Z"}],b:[{observed_at:"2026-09-13T11:00:00Z"}]},missing:[],omitted:[],incomplete_coordinates:[]};
  };
  app.data.loadProjection=async()=>{};app.data.cancelScheduledProjection=()=>{};app.sync.reportView=()=>{};
  assert.equal(await app.sync.frameRefs(["a","b","a"]),true);
  assert.equal(reads,1);assert.ok(view.t0 > Date.parse("2026-09-12"));
  const old=[view.t0,view.t1];
  app.api.call=async()=>({snapshot:"fixture:1",nodes:[{id:"a"}],coordinates:{a:[{observed_at:"2020-01-01T00:00:00Z"}]},omitted:["b"],incomplete_coordinates:["a"]});
  await assert.rejects(()=>app.sync.frameRefs(["a","b"]),/exceed the read budget/);
  assert.deepEqual([view.t0,view.t1],old);
});

test("a late batch cannot overwrite a newer focus or a changed about",async()=>{
  const {app}=loom();const {model,view}=app.state;
  model.about="project:x";view.clock="occurred";view.full={t0:0,t1:Date.parse("2026-10-01")};
  const waiting=[];app.api.call=()=>new Promise(resolve=>waiting.push(resolve));
  app.data.loadProjection=async()=>{};app.data.cancelScheduledProjection=()=>{};app.sync.reportView=()=>{};
  const batch=(id,date)=>({snapshot:"fixture:1",nodes:[{id}],coordinates:{[id]:[{occurred_at:date}]}});
  const older=app.sync.frameRefs(["old"]),newer=app.sync.frameRefs(["new"]);
  waiting[1](batch("new","2026-09-13T12:00:00Z"));assert.equal(await newer,true);
  const latest=[view.t0,view.t1];
  waiting[0](batch("old","2020-01-01T00:00:00Z"));assert.equal(await older,false);
  assert.deepEqual([view.t0,view.t1],latest);
  const changed=app.sync.frameRefs(["old"]);model.about="project:y";
  waiting[2](batch("old","2020-01-01T00:00:00Z"));assert.equal(await changed,false);
  assert.deepEqual([view.t0,view.t1],latest);
});

test("long focuses bind every batch to the first snapshot before moving the window",async()=>{
  const {app}=loom();const {model,view}=app.state;
  model.about="project:x";view.clock="occurred";view.full={t0:0,t1:Date.parse("2026-10-01")};
  const refs=Array.from({length:130},(_,i)=>`n${i}`),requests=[];
  app.api.call=async(path,params)=>{
    requests.push(params);const ids=params.ids.split(",");
    assert.equal(params.expect_snapshot,requests.length===1?undefined:"fixture:1");
    return {snapshot:"fixture:1",nodes:ids.map(id=>({id})),coordinates:Object.fromEntries(ids.map(id=>[id,[{occurred_at:"2026-09-13T12:00:00Z"}]])),scanned_edges:ids.length};
  };
  app.data.loadProjection=async()=>{};app.data.cancelScheduledProjection=()=>{};app.sync.reportView=()=>{};
  assert.equal(await app.sync.frameRefs(refs),true);
  assert.deepEqual(requests.map(r=>r.ids.split(",").length),[64,64,2]);
  assert.deepEqual(requests.map(r=>r.max_edges),[32768,32704,32640]);
  const before=[view.t0,view.t1];let reads=0;
  // A store that keeps moving: every batch answers from a newer revision, so
  // the one retry the loom allows itself fails the same way and the window stays.
  app.api.call=async()=>({snapshot:`fixture:${++reads}`,nodes:[],coordinates:{}});
  await assert.rejects(()=>app.sync.frameRefs(refs),/snapshot changed/);
  assert.equal(reads,4,"two batches per pass, one retry");
  assert.deepEqual([view.t0,view.t1],before);
});

test("a focus whose store moved under it is read again once from the new revision",async()=>{
  for (const refuse of [error=>{error.status=409;},error=>{error.code="conflict";}]) {
    const {app}=loom();const {model,view}=app.state;
    model.about="project:x";view.clock="occurred";view.full={t0:0,t1:Date.parse("2026-10-01")};
    app.data.loadProjection=async()=>{};app.data.cancelScheduledProjection=()=>{};app.sync.reportView=()=>{};
    const refs=Array.from({length:70},(_,i)=>`n${i}`),requests=[];
    app.api.call=async(path,params)=>{
      requests.push(params.expect_snapshot);
      // The store commits between the first pass's two batches: the bound
      // batch is refused, and the retry starts over from the new revision.
      if (requests.length===2){const refused=new Error("node batch snapshot changed; discard previous batches and restart the focus");refuse(refused);throw refused;}
      const ids=params.ids.split(","),snapshot=requests.length<2?"fixture:1":"fixture:2";
      return {snapshot,nodes:ids.map(id=>({id})),coordinates:Object.fromEntries(ids.map(id=>[id,[{occurred_at:"2026-09-13T12:00:00Z"}]])),scanned_edges:ids.length};
    };
    assert.equal(await app.sync.frameRefs(refs),true);
    assert.deepEqual(requests,[undefined,"fixture:1",undefined,"fixture:2"]);
    assert.ok(view.t0>Date.parse("2026-09-12"));
  }
});

test("an agent intent the loom cannot obey is said aloud and its revision consumed",async()=>{
  const {app,calls}=loom();const {model,view}=app.state;
  model.about="project:x";view.clock="occurred";view.full={t0:0,t1:Date.parse("2026-10-01")};
  app.data.loadProjection=async()=>{};app.data.cancelScheduledProjection=()=>{};app.data.loadObservability=async()=>{};app.sync.reportView=()=>{};
  app.api.call=async(path)=>{assert.equal(path,"/api/nodes");return {snapshot:"fixture:1",nodes:[{id:"a"}],coordinates:{a:[]},omitted:["b"]};};
  const before=[view.t0,view.t1];
  await app.sync.adoptAgentState({view_revision:12,about:"project:x",focus:{refs:["a","b"]},projection:{}});
  assert.equal(app.state.sync.revision,12,"a move the loom cannot obey is still consumed, not replayed");
  const shown=calls.find(call=>call.name==="dom.showError");
  assert.match(String(shown&&shown.args[0]),/could not be applied.*read budget/);
  assert.ok(calls.some(call=>call.name==="panels.renderProvenance"));
  assert.deepEqual([view.t0,view.t1],before);
});
