"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

class LabelElement {
  constructor(text, width = 108) {
    this.textContent = text;
    this.width = width;
    this.hidden = false;
    this.reads = 0;
    this.style = {};
  }
  get offsetWidth() {
    this.reads += 1;
    return this.hidden ? 0 : this.width;
  }
  get offsetHeight() {
    return this.hidden ? 0 : 16;
  }
}

function layoutAxisLabels() {
  const context = vm.createContext({
    KMP_APP: { camera: { fittedCamera() {} } },
    KMP_THREE: {},
    console,
  });
  vm.runInContext(
    fs.readFileSync(path.join(__dirname, "loom-three.js"), "utf8"),
    context,
    { filename: "loom-three.js" },
  );
  return context.KMP_APP.three.layoutAxisLabels;
}

test("axis label collision metrics survive repeated draws of hidden date labels", () => {
  const layout = layoutAxisLabels();
  const retained = [
    { type: "axis", x: 20, y: 40, element: new LabelElement("31 Dec 2026 23:50") },
    { type: "axis", x: 80, y: 40, element: new LabelElement("01 Jan 2027 00:03") },
    { type: "axis", x: 160, y: 40, element: new LabelElement("01 Jan 2027 00:16") },
  ];
  const project = () => retained.map((label) => ({ ...label, source: label }));

  layout(project(), 180, 100);
  const firstVisibility = retained.map((label) => !label.element.hidden);
  const firstReads = retained.map((label) => label.element.reads);
  layout(project(), 180, 100);

  assert.deepEqual(retained.map((label) => !label.element.hidden), firstVisibility);
  assert.deepEqual(retained.map((label) => label.element.reads), firstReads);
  assert.deepEqual(firstVisibility, [true, false, true]);
  assert.ok(retained.every((label) => label.axisWidth === 108));
});
