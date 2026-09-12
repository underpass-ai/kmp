"use strict";

/* The panel behaviour that is not the DOM adapters' layout: what a row does
   when a keyboard reaches it, what the rail says when a search matches
   nothing, and what a dimmed memory kind reports about itself.

   These are behavioural. They say nothing about how any of it looks — the
   wording, the spacing and the colours are free to change without touching
   this file. The visual conventions are written down in the design notes
   instead, where they inform rather than freeze. */

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const read = (name) => fs.readFileSync(path.join(__dirname, name), "utf8");

/* loom-panels.js under node, with a DOM small enough to watch. */
function panels() {
  const element = (tag) => {
    const node = {
      tag, className: "", children: [], listeners: {},
      style: {}, attributes: {}, hidden: false, tabIndex: undefined, title: "",
      append(...items) { this.children.push(...items); },
      replaceChildren(...items) { this.children = items; },
      addEventListener(event, handler) { this.listeners[event] = handler; },
      setAttribute(name, value) { this.attributes[name] = value; },
      querySelectorAll: () => [],
    };
    /* Assigning textContent empties the node, the way a real one does — the
       panels clear a list with `list.textContent = ""`. */
    let text = "";
    Object.defineProperty(node, "textContent", {
      get: () => text,
      set(value) { text = value; node.children = []; },
    });
    return node;
  };
  const registry = new Map();
  const context = vm.createContext({
    setTimeout, clearTimeout, console, URLSearchParams, Date,
    document: {
      getElementById: (id) => {
        if (!registry.has(id)) registry.set(id, element("div"));
        return registry.get(id);
      },
      createElement: element,
      createTextNode: (text) => ({ textContent: text }),
      querySelectorAll: () => [],
    },
  });
  context.globalThis = context;
  for (const file of ["loom-core.js", "loom-state.js"]) {
    vm.runInContext(read(file), context, { filename: file });
  }
  const app = vm.runInContext("KMP_APP", context);
  app.scene = {
    kindColor: () => "#888", classColor: () => "#888",
    requestDraw() {}, palette: () => ({}),
  };
  app.catalogue = { renderAbouts() {}, renderLabels() {} };
  app.selection = { selectEntry: () => true };
  app.viewport = { centerOn() {} };
  app.sync = { reportView() {} };
  vm.runInContext(read("loom-panels.js"), context, { filename: "loom-panels.js" });
  return { app, el: (id) => context.document.getElementById(id) };
}

function withEntries(app, entries) {
  app.state.model.entries = entries;
  app.state.model.byRef = new Map(entries.map((entry) => [entry.ref, entry]));
}

test("a search that matches nothing reports the absence instead of emptying the rail", () => {
  const { app, el } = panels();
  withEntries(app, [{ ref: "a:one", text: "keep the axes", kind: "decision", coords: [] }]);
  el("search").value = "zzzznothingmatchesthis";
  app.panels.runSearch();

  const results = el("search-results").children;
  assert.equal(results.length, 1, "the rail says something rather than going blank");
  const [row] = results;
  assert.ok(row.textContent.trim().length > 0, "the row carries a message");
  assert.equal(row.tabIndex, undefined, "the message is not a selectable result");
  assert.equal(app.state.view.searchHits.size, 0, "and nothing was selected into the view");
});

test("a search that matches leaves the message out and the results in", () => {
  const { app, el } = panels();
  withEntries(app, [{ ref: "a:one", text: "keep the axes", kind: "decision", coords: [] }]);
  el("search").value = "axes";
  app.panels.runSearch();

  const results = el("search-results").children;
  assert.equal(results.length, 1);
  assert.equal(results[0].tabIndex, 0, "the only row is the hit itself");
});

test("a search result is reachable and operable from the keyboard", () => {
  const { app, el } = panels();
  withEntries(app, [{ ref: "a:one", text: "keep the axes", kind: "decision", coords: [] }]);
  const picked = [];
  app.selection.selectEntry = (ref) => { picked.push(ref); return true; };
  el("search").value = "axes";
  app.panels.runSearch();

  const [row] = el("search-results").children;
  assert.equal(row.tabIndex, 0, "the row takes a tab stop");
  assert.equal(row.attributes.role, "button", "the row says what it is");

  let prevented = false;
  row.listeners.keydown({ key: "Enter", preventDefault: () => { prevented = true; } });
  assert.ok(prevented, "Enter is consumed by the row");
  assert.deepEqual(picked, ["a:one"], "Enter selects the entry a click would");

  row.listeners.keydown({ key: " ", preventDefault() {} });
  assert.deepEqual(picked, ["a:one", "a:one"], "Space does the same");

  row.listeners.keydown({ key: "Tab", preventDefault() {} });
  assert.equal(picked.length, 2, "Tab still moves on rather than selecting");
});

test("a relation counterpart is reachable from the keyboard too", async () => {
  const { app, el } = panels();
  const reached = [];
  app.selection.selectEntry = async (ref) => { reached.push(ref); return true; };
  app.evidence = { renderPrism() {} };
  app.timeControls = { refresh() {} };

  app.panels.renderDetail(
    {
      node: { id: "a:one", kind: "decision", title: "Keep the axes" },
      incoming: [{ source: "a:two", target: "a:one", rel: "follows", class: "causal" }],
      outgoing: [],
    },
    { ref: "a:one", coords: [] }
  );

  const find = (node) => (node.tag === "a" ? node : (node.children || []).map(find).find(Boolean));
  const link = find(el("d-incoming"));
  assert.ok(link, "the relation shows its counterpart");
  assert.equal(link.tabIndex, 0);
  assert.equal(link.attributes.role, "button");
  await link.listeners.keydown({ key: " ", preventDefault() {} });
  assert.deepEqual(reached, ["a:two"], "the keyboard reaches the same entry the pointer does");
});

test("a dimmed memory kind reports its state rather than only its colour", () => {
  const { app, el } = panels();
  const model = app.state.model;
  model.projection = { by_kind: { decision: 3 }, entries: [], labels: [] };
  model.layerProjections = [];
  model.edges = [];
  model.currentLod = "moment";

  app.panels.renderRail();
  const [row] = el("kind-legend").children;
  assert.equal(row.attributes["aria-pressed"], "false", "an undimmed kind says so");
  assert.equal(row.attributes.role, "button");
  assert.equal(row.tabIndex, 0, "and a keyboard can reach it");

  row.listeners.click();
  const [after] = el("kind-legend").children;
  assert.equal(after.attributes["aria-pressed"], "true", "a dimmed kind says so");
  assert.ok(app.state.view.dimmedKinds.has("decision"), "and the view agrees");
});
