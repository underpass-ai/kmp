"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

class Element {
  constructor() {
    this.children = [];
    this.hidden = false;
    this.parent = null;
    this.textContent = "";
    this.style = { setProperty() {} };
    this.classList = { add() {} };
  }
  append(...children) {
    for (const child of children) {
      child.parent = this;
      this.children.push(child);
    }
  }
  remove() {
    if (!this.parent) return;
    const index = this.parent.children.indexOf(this);
    if (index >= 0) this.parent.children.splice(index, 1);
    this.parent = null;
  }
  setAttribute() {}
}

function rendererContext() {
  const document = {
    createElement: () => new Element(),
    createElementNS: () => new Element(),
  };
  const context = vm.createContext({ KMP_APP: {}, KMP_THREE: {}, AbortController, document, console });
  for (const name of ["vendor/three.min.js", "loom-core.js", "loom-planes.js"])
    vm.runInContext(
      fs.readFileSync(path.join(__dirname, name), "utf8"),
      context,
      { filename: name },
    );
  return context;
}

test("rendered plane ticks keep hours and minutes across camera and lens modes", () => {
  const result = vm.runInContext(`(() => {
    const container = document.createElement("div");
    const planes = new KMP_APP.planes.Planes(new KMP_THREE.THREE.Group(), container);
    const from = Date.parse("2026-09-11T14:50:00Z");
    const to = Date.parse("2026-09-11T15:30:00Z");
    const times = [from, from + 13 * 60e3 + 20e3, from + 26 * 60e3 + 40e3, to];
    for (const mode of ["3d", "flat"]) for (const scale of ["elapsed", "event_density"]) {
      const layout = { planes: [{ about: "a", index: 0, y: 0, z: 0 }], nodes: [], times,
        timeX: time => -410 + 820 * ((time - from) / (to - from)) };
      planes.update(layout, { mode, scale, from, to, opacity: 23 }, false);
      const record = planes.records.get("a");
      if (record.ticks.some(tick => tick.element.hidden)) throw new Error(mode + "/" + scale + " hid a tick");
      if (new Set(record.ticks.map(tick => tick.element.textContent)).size !== 4)
        throw new Error(mode + "/" + scale + " collapsed labels");
    }
    return planes.records.get("a").ticks.map(tick => tick.element.textContent);
  })()`, rendererContext());
  assert.deepEqual(JSON.parse(JSON.stringify(result)), ["14:50", "15:03", "15:16", "15:30"]);
});

test("rendered plane ticks show seconds, midnight dates, and multiday dates", () => {
  const result = vm.runInContext(`(() => {
    const container = document.createElement("div");
    const planes = new KMP_APP.planes.Planes(new KMP_THREE.THREE.Group(), container);
    const cases = [
      ["2026-09-11T14:59:50Z", "2026-09-11T15:00:10Z", ["14:59:50", "14:59:56", "15:00:03", "15:00:10"]],
      ["2026-09-11T23:20:00Z", "2026-09-12T00:00:00Z", ["11 Sept 23:20", "11 Sept 23:33", "11 Sept 23:46", "12 Sept 00:00"]],
      ["2026-09-11T00:00:00Z", "2026-09-14T00:00:00Z", ["11 Sept", "12 Sept", "13 Sept", "14 Sept"]],
      ["2026-12-31T23:50:00Z", "2027-01-01T00:30:00Z", ["31 Dec 2026 23:50", "01 Jan 2027 00:03", "01 Jan 2027 00:16", "01 Jan 2027 00:30"]],
    ];
    return cases.map(([fromText, toText, expected]) => {
      const from = Date.parse(fromText), to = Date.parse(toText);
      const times = [from, from + (to - from) / 3, from + 2 * (to - from) / 3, to];
      const layout = { planes: [{ about: "a", index: 0, y: 0, z: 0 }], nodes: [], times,
        timeX: time => -410 + 820 * ((time - from) / (to - from)) };
      planes.update(layout, { mode: "3d", scale: "elapsed", from, to, opacity: 23 }, false);
      const actual = planes.records.get("a").ticks.map(tick => tick.element.textContent);
      if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error(JSON.stringify(actual));
      return actual;
    });
  })()`, rendererContext());
  assert.deepEqual(JSON.parse(JSON.stringify(result)), [
    ["14:59:50", "14:59:56", "15:00:03", "15:00:10"],
    ["11 Sept 23:20", "11 Sept 23:33", "11 Sept 23:46", "12 Sept 00:00"],
    ["11 Sept", "12 Sept", "13 Sept", "14 Sept"],
    ["31 Dec 2026 23:50", "01 Jan 2027 00:03", "01 Jan 2027 00:16", "01 Jan 2027 00:30"],
  ]);
});

test("event-density labels refine seconds when four samples share a minute", () => {
  const result = vm.runInContext(`(() => {
    const container = document.createElement("div");
    const planes = new KMP_APP.planes.Planes(new KMP_THREE.THREE.Group(), container);
    const from = Date.parse("2026-09-11T14:50:00Z"), to = Date.parse("2026-09-11T15:30:00Z");
    const times = [1, 2, 3, 4].map(seconds => from + seconds * 1000);
    planes.update({ planes: [{ about: "a", index: 0, y: 0, z: 0 }], nodes: [], times,
      timeX: time => -410 + 820 * ((time - from) / (to - from)) },
      { mode: "flat", scale: "event_density", from, to, opacity: 23 }, false);
    return planes.records.get("a").ticks.map(tick => tick.element.textContent);
  })()`, rendererContext());
  assert.deepEqual(JSON.parse(JSON.stringify(result)), ["14:50:01", "14:50:02", "14:50:03", "14:50:04"]);
});
