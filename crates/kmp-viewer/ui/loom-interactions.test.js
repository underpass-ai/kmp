"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
function load(context, name) {
  vm.runInContext(fs.readFileSync(path.join(__dirname, name), "utf8"), context, { filename: name });
}
function harness() {
  const elements = new Map(), frames = new Map(), timers = new Map();
  let id = 0;
  class Element {
    constructor() { this.children = []; this.events = {}; this.value = ""; this.textContent = ""; this.clientWidth = 1000; this.style = {}; }
    addEventListener(name, fn) { this.events[name] = fn; }
    setPointerCapture() {}
    emit(name, event = {}) { this.events[name]?.(event); }
    append(...children) { children.forEach(child => this.insertBefore(child, null)); }
    insertBefore(child, next) { child.remove(); child.parent = this; const i = this.children.indexOf(next); this.children.splice(i < 0 ? this.children.length : i, 0, child); }
    remove() { if (this.parent) { const children = this.parent.children; children.splice(children.indexOf(this), 1); this.parent = null; } }
    get nextSibling() { return this.parent?.children[this.parent.children.indexOf(this) + 1] || null; }
    blur() {}
    setAttribute() {}
  }
  const $ = id => { if (!elements.has(id)) elements.set(id, new Element()); return elements.get(id); };
  const calls = [], view = { full: { t0: 0, t1: 1000 }, t0: 200, t1: 600, windowStack: [] };
  const app = { state: { model: { entries: [], byRef: new Map() }, view, tracePick: {} },
    dom: { $, el: () => new Element() },
    viewport: { setWindow: (...args) => { calls.push(args); [view.t0, view.t1] = args; } },
    scene: { requestDraw: () => calls.push("draw") }, sync: { reportView() {} } };
  const context = vm.createContext({ KMP_APP: app, console,
    document: { getElementById: $, createElement: () => new Element(), querySelectorAll: () => [] },
    addEventListener() {},
    requestAnimationFrame: fn => { frames.set(++id, fn); return id; }, cancelAnimationFrame: id => frames.delete(id),
    setTimeout: fn => { timers.set(++id, fn); return id; }, clearTimeout: id => timers.delete(id),
  });
  const flush = queue => { const pending = [...queue.values()]; queue.clear(); pending.forEach(fn => fn()); };
  return { context, app, $, calls, frames, timers, flush };
}

test("navigator bursts apply the newest position once, flush release and keep one undo", () => {
  const { context, app, $, calls, frames, flush } = harness();
  load(context, "loom-gestures.js"); app.gestures.wire();
  const nav = $("nav-canvas");
  nav.emit("pointerdown", { offsetX: 400, pointerId: 1 });
  for (let x = 405; x <= 450; x++) nav.emit("pointermove", { offsetX: x });
  assert.equal(calls.length, 0); assert.equal(frames.size, 1);
  flush(frames); assert.deepEqual(calls, [[250, 650, false]]);
  nav.emit("pointermove", { offsetX: 470 });
  nav.emit("pointerup", { offsetX: 500 });
  assert.deepEqual(calls.at(-1), [300, 700, false]);
  assert.equal(frames.size, 0); assert.equal(app.state.view.windowStack.length, 1);
  nav.emit("pointerdown", { offsetX: 400, pointerId: 2 });
  nav.emit("pointermove", { offsetX: 500 }); nav.emit("pointercancel"); flush(frames);
  assert.equal(calls.length, 2);
  nav.emit("pointerdown", { offsetX: 400, pointerId: 3 });
  nav.emit("pointermove", { offsetX: 600 }); app.state.view.full = { t0: 0, t1: 2000 };
  flush(frames); assert.equal(calls.length, 2, "a projection replacement cancels the old gesture");
});

test("navigator edge resize, selection and click preserve final intervals", () => {
  for (const [down, up, expected] of [[200, 100, [100, 600, false]], [600, 800, [200, 800, false]], [700, 900, [700, 900, false]], [800, 800, [600, 1000]]]) {
    const { context, app, $, calls } = harness(); load(context, "loom-gestures.js"); app.gestures.wire();
    $("nav-canvas").emit("pointerdown", { offsetX: down, pointerId: 1 });
    $("nav-canvas").emit("pointerup", { offsetX: up });
    assert.deepEqual(calls.at(-1), expected);
  }
});

test("memory picker retains options through selection, edits, reordering and removal", () => {
  const { context, app, $ } = harness(); load(context, "loom-evidence.js");
  const model = app.state.model, view = app.state.view;
  model.currentLod = "moment"; model.entries = [{ ref: "a", kind: "decision", text: "A" }, { ref: "b", kind: "observation", text: "B" }];
  app.evidence.renderPicker(); const picker = $("memory-picker"), [prompt, a, b] = picker.children;
  view.selectedRef = "b"; app.evidence.renderPicker(); assert.equal(picker.value, "b"); assert.deepEqual(picker.children, [prompt, a, b]);
  model.entries = [{ ...model.entries[1], text: "Changed" }, model.entries[0]]; app.evidence.renderPicker();
  assert.deepEqual(picker.children, [prompt, b, a]); assert.equal(b.textContent, "observation · Changed");
  model.entries = [model.entries[0]]; app.evidence.renderPicker(); assert.deepEqual(picker.children, [prompt, b]);
  model.entries = []; model.currentLod = "atlas"; app.evidence.renderPicker(); assert.equal(picker.disabled, true); assert.match(prompt.textContent, /Memories detail/);
});

test("search debounces typing but explicit intent and Escape cancel pending work", () => {
  const { context, app, $, calls, timers, flush } = harness();
  load(context, "loom-core.js"); load(context, "loom-panels.js"); app.panels.wire();
  const model = app.state.model;
  model.entries = [{ ref: "a", kind: "decision", text: "alpha", coords: [] }, { ref: "b", kind: "observation", text: "beta", coords: [] }];
  model.byRef = new Map(model.entries.map(entry => [entry.ref, entry]));
  for (const value of ["a", "al", "alpha"]) { $("search").value = value; $("search").emit("input"); }
  assert.equal(calls.length, 0); assert.equal(timers.size, 1); flush(timers);
  assert.deepEqual([...app.state.view.searchHits], ["a"]); assert.equal(calls.length, 1);
  $("search").value = "alpha"; $("search").emit("input"); app.panels.setSearch("beta");
  assert.equal(timers.size, 0); assert.deepEqual([...app.state.view.searchHits], ["b"]);
  $("search").value = "alpha"; $("search").emit("input"); $("search").emit("keydown", { key: "Escape" }); flush(timers);
  assert.equal($("search").value, ""); assert.equal(app.state.view.searchHits.size, 0);
});

test("indexed time lenses match the linear oracle at every boundary and scale logarithmically", () => {
  const context = vm.createContext({}); load(context, "loom-core.js");
  for (const mode of ["elapsed", "event_density"]) for (const focus of [null, { from: 40, to: 5000 }]) {
    const lens = vm.runInContext("KMP_LOOM", context).temporalLens({ mode, t0: 0, t1: 100000, events: Array.from({ length: 2048 }, (_, i) => i * i / 50), focus });
    const segments = lens.segments;
    for (const segment of segments) for (const t of [segment.t0, segment.t1, (segment.t0 + segment.t1) / 2]) {
      const old = segments.find(s => t <= s.t1) || segments.at(-1);
      const expected = old.u0 + (t - old.t0) / Math.max(1, old.t1 - old.t0) * (old.u1 - old.u0);
      assert.equal(lens.toRatio(t), expected);
      const u = segment.u1, reverse = segments.find(s => u <= s.u1) || segments.at(-1);
      assert.equal(lens.fromRatio(u), reverse.t0 + (u - reverse.u0) / Math.max(Number.EPSILON, reverse.u1 - reverse.u0) * (reverse.t1 - reverse.t0));
    }
    assert.equal(lens.toRatio(-Infinity), 0); assert.equal(lens.fromRatio(Infinity), 100000); assert.ok(Number.isNaN(lens.toRatio(NaN)));
    let reads = 0;
    for (const segment of segments) { const end = segment.t1; Object.defineProperty(segment, "t1", { get() { reads++; return end; } }); }
    lens.toRatio(99999);
    assert.ok(reads <= Math.ceil(Math.log2(segments.length)) + 2, `${reads} segment reads`);
  }
});

test("scene sliders coalesce to the final state and replacing the renderer disposes it", async () => {
  const { context, app, $, frames, flush } = harness();
  const renders = [], disposed = [];
  app.state.model.currentLod = "moment";
  app.state.view.layerGap = 130; app.state.view.layerOpacity = 30;
  app.theme = { wire() {} }; app.viewport.lens = () => ({ toRatio: x => x });
  app.sceneModel = { layout: () => ({ nodes: [] }) };
  app.three = { MemoryScene: class {
    constructor() { this.controls = { addEventListener() {} }; }
    update(layout, state) { renders.push(state); }
    dispose() { disposed.push(this); }
  } };
  load(context, "loom-scene.js"); await app.scene.setup(); app.scene.wire(); flush(frames);
  for (let i = 0; i < 100; i++) {
    $("layer-gap").emit("input", { target: { value: 130 + i } });
    $("layer-opacity").emit("input", { target: { value: i } });
  }
  assert.equal(renders.length, 1); assert.equal(frames.size, 1); flush(frames);
  assert.equal(renders.length, 2); assert.equal(renders.at(-1).gap, 229); assert.equal(renders.at(-1).opacity, 99);
  await app.scene.setup(); assert.equal(disposed.length, 1); flush(frames);
});
