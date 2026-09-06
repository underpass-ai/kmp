"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
function fixture(extra = {}, names = ["loom-loading.js", "loom-api.js"]) {
  const states = [];
  const context = vm.createContext({
    KMP_APP: { loadingView: { render: (state) => states.push(state) } },
    ...extra,
  });
  for (const name of names)
    vm.runInContext(
      fs.readFileSync(path.join(__dirname, name), "utf8"),
      context,
    );
  return { app: context.KMP_APP, states, context, last: () => states.at(-1) };
}

test("HTTP content remains busy through response decoding, independently per about", async () => {
  const first = deferred(),
    second = deferred();
  const { app, last } = fixture({
    fetch: async (url) => ({
      ok: true,
      json: () => (url.includes("about=a") ? first.promise : second.promise),
    }),
  });
  const a = app.api.fetchProjection("a", "observed");
  const b = app.api.fetchProjection("b", "observed");
  assert.equal(last().scene.busy, true);
  assert.match(last().scene.label, /Observed/);
  first.resolve({ entries: [] });
  await a;
  assert.equal(last().scene.busy, true);
  second.resolve({ entries: [] });
  await b;
  assert.equal(last().scene.busy, false);
});

test("a superseded clock response cannot clear the current read", async () => {
  const old = deferred(),
    current = deferred();
  const { app, last } = fixture({
    KMP_APP_API: (path, params) =>
      params.axis === "observed" ? old.promise : current.promise,
  });
  const a = app.api.fetchProjection("a", "observed");
  const b = app.api.fetchProjection("a", "ingested");
  old.resolve({});
  await a;
  assert.equal(last().scene.busy, true);
  assert.match(last().scene.label, /Ingested/);
  current.resolve({});
  await b;
  assert.equal(last().scene.busy, false);
});

test("an obsolete slow read cannot prolong a completed replacement", async () => {
  const old = deferred();
  const { app, last } = fixture({
    KMP_APP_API: (path, params) =>
      params.axis === "observed" ? old.promise : Promise.resolve({}),
  });
  const a = app.api.fetchProjection("a", "observed");
  await app.api.fetchProjection("a", "occurred");
  assert.equal(last().scene.busy, false);
  old.resolve({});
  await a;
  assert.equal(last().scene.busy, false);
});

test("failed HTTP and MCP reads release their own region and retain the error", async () => {
  for (const extra of [
    {
      fetch: async () => ({
        ok: false,
        status: 503,
        json: async () => ({ error: "unavailable" }),
      }),
    },
    {
      fetch: async () => {
        throw new Error("unavailable");
      },
    },
    {
      KMP_APP_API: async () => {
        throw new Error("unavailable");
      },
    },
  ]) {
    const { app, last } = fixture(extra);
    const finishScene = app.loading.begin(
      "layers",
      "scene",
      "Updating layers…",
    );
    await assert.rejects(
      app.api.call("/api/node", { about: "a", id: "one" }),
      /unavailable/,
    );
    assert.equal(last().detail.busy, false);
    assert.equal(last().scene.busy, true);
    finishScene();
    assert.equal(last().scene.busy, false);
  }
});

test("view polling and human/agent control never trigger a content loader", async () => {
  const poll = deferred();
  const { app, states } = fixture({ KMP_APP_API: () => poll.promise });
  const requests = [
    app.api.call("/api/view", { after: 3 }),
    app.api.call("/api/view/take-control", {}, "POST"),
    app.api.call("/api/view/report", {}, "POST"),
  ];
  assert.equal(states.length, 0);
  poll.resolve({});
  await Promise.all(requests);
  assert.equal(states.length, 0);
});

test("layer operation covers the gaps between reads and repeated completion is harmless", () => {
  const { app, states, last } = fixture();
  const finish = app.loading.begin("layers", "scene", "Updating layers…", 1);
  app.loading.beginRequest("/api/projection", {
    about: "a",
    axis: "observed",
  })();
  assert.equal(last().scene.label, "Updating layers…");
  assert.equal(last().scene.busy, true);
  finish();
  assert.equal(last().scene.busy, false);
  const count = states.length;
  finish();
  assert.equal(states.length, count);
});

test("DOM feedback waits for paint and coalesces chained reads without a flash", () => {
  const nodes = new Map(),
    frames = new Map();
  let next = 0;
  const node = (id) => {
    if (!nodes.has(id))
      nodes.set(id, {
        dataset: {},
        attrs: {},
        textContent: "",
        setAttribute(key, value) {
          this.attrs[key] = value;
        },
      });
    return nodes.get(id);
  };
  const paint = () => {
    const queued = [...frames.values()];
    frames.clear();
    queued.forEach((fn) => fn());
  };
  const { app } = fixture(
    {
      document: { getElementById: node },
      requestAnimationFrame: (fn) => {
        frames.set(++next, fn);
        return next;
      },
      cancelAnimationFrame: (id) => frames.delete(id),
    },
    ["loom-loading.js", "loom-loading-view.js"],
  );
  const finish = app.loading.begin("one", "scene", "Loading memory…");
  assert.equal(node("stage").dataset.loading, "true");
  assert.equal(node("loom-canvas").attrs["aria-busy"], "true");
  assert.equal(
    node("stage").attrs["aria-busy"],
    undefined,
    "live status is outside the busy subtree",
  );
  finish();
  paint();
  assert.equal(node("stage").dataset.loading, "true");
  const finishNext = app.loading.begin("two", "scene", "Updating layers…");
  paint();
  paint();
  assert.equal(node("scene-loading-label").textContent, "Updating layers…");
  assert.equal(node("stage").dataset.loading, "true");
  finishNext();
  paint();
  paint();
  assert.equal(node("stage").dataset.loading, "false");
  assert.equal(node("loom-canvas").attrs["aria-busy"], "false");
  assert.equal(node("scene-loading").attrs["aria-hidden"], "true");
});

test("adding and removing layers stay busy until the replacement projection is applied", async () => {
  for (const abouts of [["b"], []]) {
    const projection = deferred();
    const { app, last } = fixture({}, [
      "loom-core.js",
      "loom-state.js",
      "loom-loading.js",
      "loom-layers.js",
    ]);
    app.state.model.about = "a";
    Object.assign(app.state.view, {
      layerAbouts: ["old"],
      full: { t0: 0, t1: 1000 },
      t0: 0,
      t1: 1000,
    });
    app.panels = { renderAbouts() {} };
    app.dom = { showError: (error) => assert.fail(error) };
    app.api = {
      fetchProjection: async () => ({
        clusters: [
          {
            from: "2026-09-01T00:00:00Z",
            to: "2026-09-02T00:00:00Z",
            total: 1,
          },
        ],
      }),
    };
    app.viewport = { setWindow() {} };
    let cancelled = false,
      loadingProjection = false;
    app.data = {
      cancelScheduledProjection() {
        cancelled = true;
      },
      async loadProjection() {
        loadingProjection = true;
        await projection.promise;
      },
    };
    const change = app.layers.set(abouts);
    assert.equal(last().scene.busy, true);
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(cancelled, true);
    assert.equal(loadingProjection, true);
    assert.equal(
      last().scene.busy,
      true,
      "extent completion must not finish a layer change",
    );
    assert.equal(last().scene.label, "Updating layers…");
    projection.resolve();
    await change;
    assert.equal(last().scene.busy, false);
    assert.deepEqual([...app.state.view.layerAbouts], abouts);
  }
});

test("failed layer updates clear loading and expose their failure", async () => {
  const { app, last } = fixture({}, [
    "loom-core.js",
    "loom-state.js",
    "loom-loading.js",
    "loom-layers.js",
  ]);
  app.state.model.about = "a";
  app.panels = { renderAbouts() {} };
  const errors = [];
  app.dom = { showError: (error) => errors.push(error) };
  app.api = {
    fetchProjection: async () => {
      throw new Error("unavailable");
    },
  };
  await app.layers.set(["b"]);
  assert.equal(last().scene.busy, false);
  assert.deepEqual(errors, ["unavailable"]);
});
