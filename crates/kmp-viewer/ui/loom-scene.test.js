"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

function modules(names, extras = {}) {
  const context = vm.createContext({
    KMP_APP: {},
    console,
    AbortController,
    ...extras,
  });
  for (const name of names)
    vm.runInContext(
      fs.readFileSync(path.join(__dirname, name), "utf8"),
      context,
      { filename: name },
    );
  return context;
}
const plain = (value) => JSON.parse(JSON.stringify(value));
const start = Date.parse("2026-09-01T00:00:00Z");
const entry = (about, id, hour, labels = ["topic"]) => ({
  ref_id: `${about}:${id}`,
  kind: "decision",
  text: id,
  coordinates: labels.map((dimension) => ({
    dimension,
    scope_id: `${dimension}:review`,
    observed_at: new Date(start + hour * 3600000).toISOString(),
  })),
});
const state = {
  mode: "3d",
  gap: 130,
  clock: "observed",
  from: start,
  to: start + 86400000,
  relations: "all",
  selected: null,
};
const ratio = (time) => (time - start) / 86400000;

test("each memory belongs to one about plane, independently of its labels", () => {
  const { KMP_APP: app } = modules(["loom-core.js", "loom-scene-model.js"]);
  const first = entry("about:a", "one", 4, ["topic", "status", "arbitrary"]),
    second = entry("about:b", "two", 4);
  const layout = app.sceneModel.layout(
    [
      {
        about: "about:a",
        lod: "moment",
        projection: { entries: [first, first] },
      },
      { about: "about:b", lod: "moment", projection: { entries: [second] } },
    ],
    state,
    ratio,
  );
  assert.equal(layout.nodes.length, 2);
  assert.equal(
    layout.nodes[0].x,
    layout.nodes[1].x,
    "one clock and scale across abouts",
  );
  assert.notEqual(layout.nodes[0].z, layout.nodes[1].z);
  assert.deepEqual(plain(layout.nodes.map((node) => node.entry.about)), [
    "about:a",
    "about:b",
  ]);
  assert.equal(
    layout.shownRelations.length,
    0,
    "layer proximity invents no relation",
  );
});

test("time is strict and half open; only visible stored endpoints produce edges", () => {
  const { KMP_APP: app } = modules(["loom-core.js", "loom-scene-model.js"]);
  const one = entry("a", "one", 0),
    end = entry("a", "end", 24),
    two = entry("b", "two", 2);
  const missing = {
    ...entry("a", "missing", 3),
    coordinates: [
      {
        dimension: "topic",
        scope_id: "x",
        occurred_at: new Date(start).toISOString(),
      },
    ],
  };
  const layout = app.sceneModel.layout(
    [
      {
        about: "a",
        lod: "moment",
        projection: {
          entries: [one, end, missing],
          relations: [
            {
              from: one.ref_id,
              to: two.ref_id,
              rel: "follows",
              class: "procedural",
            },
            { from: one.ref_id, to: end.ref_id, rel: "follows" },
          ],
        },
      },
      { about: "b", lod: "moment", projection: { entries: [two] } },
    ],
    state,
    ratio,
  );
  assert.deepEqual(plain(layout.nodes.map((node) => node.entry.ref)), [
    "a:one",
    "b:two",
  ]);
  assert.equal(layout.shownRelations.length, 1);
  assert.equal(layout.shownRelations[0].target, "b:two");
});

test("flat and perspective share the same semantic layout; coarse marks stay aggregates", () => {
  const { KMP_APP: app } = modules(["loom-core.js", "loom-scene-model.js"]);
  const layers = ["a", "b"].map((about) => ({
    about,
    lod: "episode",
    projection: {
      clusters: [
        {
          from: new Date(start).toISOString(),
          to: new Date(start + 3600000).toISOString(),
          dimension: "topic",
          scope_id: "review",
          total: 12,
        },
      ],
    },
  }));
  const depth = app.sceneModel.layout(layers, state, ratio),
    flat = app.sceneModel.layout(layers, { ...state, mode: "flat" }, ratio);
  assert.ok(depth.nodes.every((node) => node.entry.aggregate));
  assert.match(depth.planes[0].caption, /aggregates/);
  assert.deepEqual(
    plain(flat.nodes.map((node) => node.x)),
    plain(depth.nodes.map((node) => node.x)),
  );
  assert.notEqual(flat.nodes[0].y, flat.nodes[1].y);
  assert.equal(flat.nodes[0].z, flat.nodes[1].z);
});

test("an audited path highlights its actual hops beyond the selected node", () => {
  const { KMP_APP: app } = modules(["loom-core.js", "loom-scene-model.js"]);
  const layers = [
    {
      about: "a",
      lod: "moment",
      projection: {
        entries: [
          entry("a", "one", 0),
          entry("a", "two", 1),
          entry("a", "three", 2),
          entry("a", "other", 3),
        ],
        relations: [
          { from: "a:one", to: "a:two", rel: "follows" },
          { from: "a:two", to: "a:three", rel: "follows" },
          { from: "a:one", to: "a:other", rel: "follows" },
        ],
      },
    },
  ];
  const layout = app.sceneModel.layout(
    layers,
    {
      ...state,
      selected: "a:one",
      trace: {
        refs: new Set(["a:one", "a:two", "a:three"]),
        edgeKeys: new Set(["a:one follows a:two", "a:two follows a:three"]),
      },
    },
    ratio,
  );
  assert.equal(layout.shownRelations.length, 2);
  assert.ok(layout.shownRelations.some((edge) => edge.source === "a:two"));
  assert.ok(layout.nodes.find((node) => node.entry.ref === "a:other").dimmed);
  assert.ok(!layout.nodes.find((node) => node.entry.ref === "a:three").dimmed);
});

test("the camera fits the actual layered volume in perspective and orthographic modes", () => {
  const {
    KMP_APP: app,
    KMP_THREE: { THREE },
  } = modules(["vendor/three.min.js", "loom-camera.js"]);
  for (const count of [1, 3, 6])
    for (const aspect of [0.65, 1.6, 3]) {
      for (const mode of ["3d", "flat"]) {
        const planes = Array.from({ length: count }, (_, i) => ({
          y: mode === "flat" ? ((count - 1) / 2 - i) * 310 : 0,
          z: mode === "3d" ? ((count - 1) / 2 - i) * 300 : 0,
        }));
        const { camera } = app.camera.fittedCamera({ planes }, mode, aspect);
        camera.updateMatrixWorld();
        assert.equal(Boolean(camera.isPerspectiveCamera), mode === "3d");
        for (const plane of planes)
          for (const x of [-470, 470])
            for (const dy of [-140, 140]) {
              const point = new THREE.Vector3(x, plane.y + dy, plane.z).project(
                camera,
              );
              assert.ok(
                Math.abs(point.x) <= 1 &&
                  Math.abs(point.y) <= 1 &&
                  Math.abs(point.z) <= 1,
                `${mode}/${count}/${aspect} fits`,
              );
            }
      }
    }
});

test("a late additional-about read cannot replace a newer window or removed layer", async () => {
  const { KMP_APP: app } = modules([
    "loom-core.js",
    "loom-state.js",
    "loom-layers.js",
  ]);
  const pending = [];
  let draws = 0;
  app.api = {
    fetchProjection: (...args) =>
      new Promise((resolve) => pending.push({ args, resolve })),
  };
  app.scene = { requestDraw: () => draws++ };
  Object.assign(app.state.model, { about: "a", currentLod: "moment" });
  Object.assign(app.state.view, {
    layerAbouts: ["b"],
    full: { t0: start, t1: start + 7200000 },
    clock: "observed",
    t0: start,
    t1: start + 3600000,
  });
  const old = app.layers.load();
  app.state.view.t1 += 3600000;
  const fresh = app.layers.load();
  pending[1].resolve({ entries: [entry("b", "fresh", 1)] });
  await fresh;
  pending[0].resolve({ entries: [entry("b", "stale", 1)] });
  await old;
  assert.equal(
    app.state.model.layerProjections[0].projection.entries[0].ref_id,
    "b:fresh",
  );
  assert.equal(draws, 1);
  const removed = app.layers.load();
  app.layers.invalidate();
  pending[2].resolve({ entries: [entry("b", "removed", 1)] });
  await removed;
  assert.equal(app.state.model.layerProjections.length, 0);
});

test("the shared extent includes non-overlapping abouts without fetching entry bodies", async () => {
  const { KMP_APP: app } = modules([
    "loom-core.js",
    "loom-state.js",
    "loom-layers.js",
  ]);
  const calls = [];
  app.state.view.layerAbouts = ["b"];
  const probe = (hour) => ({
    clusters: [
      {
        from: new Date(start + hour * 3600000).toISOString(),
        to: new Date(start + (hour + 1) * 3600000).toISOString(),
        total: 1,
        dimension: "topic",
        scope_id: "review",
      },
    ],
  });
  app.api = {
    EXTENT_FROM: "begin",
    EXTENT_TO: "end",
    fetchProjection: async (...args) => {
      calls.push(args);
      return probe(24);
    },
  };
  const result = await app.layers.extent("a", "observed", [], probe(0));
  assert.ok(result.full.t0 < start);
  assert.ok(result.full.t1 > start + 25 * 3600000);
  assert.deepEqual(plain(calls[0].slice(0, 6)), [
    "b",
    "observed",
    "begin",
    "end",
    "episode",
    128,
  ]);
  const emptyPrimary = await app.layers.extent("a", "observed", [], {});
  assert.ok(emptyPrimary.full.t0 < start + 24 * 3600000);
  assert.ok(emptyPrimary.full.t1 > start + 25 * 3600000);
});

test("adding an about expands All while retaining a focused window", async () => {
  const { KMP_APP: app } = modules([
    "loom-core.js",
    "loom-state.js",
    "loom-layers.js",
  ]);
  const calls = [];
  app.state.model.about = "a";
  app.state.view.full = { t0: start, t1: start + 3600000 };
  Object.assign(app.state.view, app.state.view.full);
  app.panels = { renderAbouts() {} };
  app.dom = { showError: (error) => assert.fail(error) };
  app.api = {
    fetchProjection: async (about) => ({
      clusters: [
        {
          from: new Date(
            start + (about === "a" ? 0 : 24) * 3600000,
          ).toISOString(),
          to: new Date(
            start + (about === "a" ? 1 : 25) * 3600000,
          ).toISOString(),
          dimension: "topic",
          total: 1,
        },
      ],
    }),
  };
  app.viewport = {
    setWindow: (from, to) => {
      calls.push([from, to]);
      app.state.view.t0 = from;
      app.state.view.t1 = to;
    },
  };
  await app.layers.set(["b"]);
  assert.ok(calls[0][1] > start + 25 * 3600000);
  app.state.view.t0 = start + 1000;
  app.state.view.t1 = start + 2000;
  await app.layers.set(["b", "c"]);
  assert.deepEqual(calls[1], [start + 1000, start + 2000]);
});

test("taking human control calls the shared revision operation without reframing", async () => {
  const { KMP_APP: app } = modules(["loom-state.js", "loom-control.js"]);
  const rendered = [],
    commands = [];
  app.state.sync.revision = 8;
  app.provenance = {
    render: (s) => rendered.push(s),
    busy: () => {},
    unavailable: () => assert.fail("available"),
  };
  app.sync = {
    VIEW_ID: "default",
    applyAgentState: () => assert.fail("successful handoff never reframes"),
  };
  app.dom = { showError: () => assert.fail("no error") };
  app.api = {
    call: async (...args) => {
      commands.push(args);
      return {
        view_revision: 9,
        last_change: { actor: "human" },
        can_undo: true,
      };
    },
  };
  app.control.render({
    view_revision: 8,
    last_change: { actor: "agent:review" },
  });
  await app.control.takeControl();
  assert.equal(commands[0][0], "/api/view/take-control");
  assert.deepEqual(plain(commands[0][1]), {
    id: "default",
    expected_revision: 8,
  });
  assert.equal(commands[0][2], "POST");
  assert.equal(app.state.sync.revision, 9);
  await app.control.takeControl();
  assert.equal(commands.length, 1, "already human: no duplicate request");
});

test("the embedded renderer matches its reviewed vendor pin", () => {
  const crypto = require("node:crypto");
  const bytes = fs.readFileSync(path.join(__dirname, "vendor/three.min.js"));
  assert.equal(
    crypto.createHash("sha256").update(bytes).digest("hex"),
    "45d8f97107302c103faebbe44ac4f1f3b2124e8ac1163998603dc3d8f452cc73",
  );
});
