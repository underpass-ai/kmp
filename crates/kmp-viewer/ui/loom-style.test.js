"use strict";

/* The presentation contract, checked on the stylesheets the viewer actually
   serves. These are not snapshots of how the loom looks — they are the rules
   that let it look like one thing: one palette, one type scale, both themes
   complete, and no control that only a mouse can reach. */

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const read = (name) => fs.readFileSync(path.join(__dirname, name), "utf8");
const LOOM_CSS = read("loom.css");
const SHELL_CSS = read("loom-shell.css");
const LOADING_CSS = read("loom-loading.css");
const INDEX_HTML = read("index.html");
const ALL_CSS = `${LOOM_CSS}\n${SHELL_CSS}\n${LOADING_CSS}`;

/* Every `--name:` declaration, with the selector block it was declared in.
   Good enough for :root blocks, which is all the palette uses. */
function declarations(css) {
  const found = new Map();
  const blocks = css.matchAll(/([^{}]+)\{([^{}]*)\}/g);
  for (const [, selector, body] of blocks) {
    for (const [, name] of body.matchAll(/(--[a-z0-9-]+)\s*:/g)) {
      if (!found.has(name)) found.set(name, new Set());
      found.get(name).add(selector.trim());
    }
  }
  return found;
}

/* Every `var(--name)` reference, ignoring the fallback after a comma. */
function references(css) {
  return new Set([...css.matchAll(/var\(\s*(--[a-z0-9-]+)/g)].map(([, name]) => name));
}

const PALETTE = [
  "--bg", "--panel", "--soft", "--raised", "--text", "--text-2", "--muted",
  "--line", "--line-strong", "--accent", "--accent-ink", "--teal", "--danger",
  "--field", "--canvas", "--pane", "--tint",
];

test("the palette is declared once, in the shell, and completely in both themes", () => {
  const shell = declarations(SHELL_CSS);
  const loom = declarations(LOOM_CSS);
  for (const name of PALETTE) {
    const selectors = shell.get(name);
    assert.ok(selectors, `${name} is declared in loom-shell.css`);
    assert.ok(
      [...selectors].some((selector) => selector.includes('[data-theme="dark"]')),
      `${name} has a dark value`
    );
    assert.ok(
      [...selectors].some((selector) => selector.includes('[data-theme="light"]')),
      `${name} has a light value`
    );
    assert.ok(
      !loom.has(name),
      `${name} is not re-declared in loom.css, which would give the loom two palettes`
    );
  }
});

test("no rule reads a custom property that nothing declares", () => {
  const declared = new Set(declarations(ALL_CSS).keys());
  /* The renderer sets these per element before it paints. */
  const runtime = new Set(["--plane-color"]);
  const missing = [...references(ALL_CSS)].filter(
    (name) => !declared.has(name) && !runtime.has(name)
  );
  assert.deepEqual(missing, [], "every var() resolves to a declaration");
});

test("the WebGL and canvas palette reads only names the stylesheets declare", () => {
  const theme = read("loom-theme.js");
  const declared = new Set(declarations(ALL_CSS).keys());
  const wanted = [...theme.matchAll(/read\("(--[a-z0-9-]+)"\)/g)].map(([, name]) => name);
  assert.ok(wanted.length > 10, "the palette adapter reads the shared names");
  for (const name of wanted) {
    assert.ok(declared.has(name), `${name}, read by loom-theme.js, is declared in CSS`);
  }
});

test("the type scale is the only source of font sizes outside the body rule", () => {
  const offenders = [];
  for (const [name, css] of [["loom.css", LOOM_CSS], ["loom-shell.css", SHELL_CSS]]) {
    for (const [match] of css.matchAll(/font(?:-size)?:\s*[^;]*?\b\d+px\b[^;]*/g)) {
      /* The body rule sets the one absolute root size the scale hangs off. */
      if (match.includes("14px/1.45")) continue;
      offenders.push(`${name}: ${match.trim()}`);
    }
  }
  assert.deepEqual(offenders, [], "font sizes come from --fs-*");
});

test("selection and hover are told apart without a weight change that reflows", () => {
  const rule = LOOM_CSS.match(/\.item-list li\.active\s*\{([^}]*)\}/);
  assert.ok(rule, "the selected row has its own rule");
  assert.ok(/border-left-color:\s*var\(--accent\)/.test(rule[1]), "selection carries an accent edge");
  assert.ok(!/font-weight/.test(rule[1]), "selection does not change the font weight");
  assert.match(LOOM_CSS, /\.item-list li:hover \{ background: var\(--surface-3\); \}/);
});

test("both themes keep a focus ring and honour reduced motion", () => {
  assert.match(SHELL_CSS, /:focus-visible[\s\S]*?outline:\s*2px solid var\(--teal\)/);
  assert.match(LOOM_CSS, /@media \(prefers-reduced-motion: reduce\)/);
  assert.match(LOADING_CSS, /@media \(prefers-reduced-motion: reduce\)/);
});

test("the shell declares no animation beyond the loading pulse", () => {
  assert.ok(!/@keyframes/.test(SHELL_CSS), "the shell adds no keyframes");
  assert.ok(!/@keyframes/.test(LOOM_CSS), "the evidence rules add no keyframes");
  assert.deepEqual(
    [...LOADING_CSS.matchAll(/@keyframes\s+([a-z-]+)/g)].map(([, name]) => name),
    ["loom-loading-pulse"]
  );
});

test("the page fetches no font and no other origin", () => {
  assert.ok(!/@import/.test(ALL_CSS), "no stylesheet imports another");
  assert.ok(!/url\(\s*["']?https?:/i.test(ALL_CSS), "no remote url() in CSS");
  assert.ok(!/https?:\/\//.test(INDEX_HTML.replace(/xmlns="[^"]*"/g, "")), "no remote asset in the page");
});

test("the clock axis and the camera mode are one segmented control each", () => {
  assert.match(INDEX_HTML, /<nav class="chips" id="clock-chips"[^>]*>.*?<span class="segmented">/s);
  assert.match(INDEX_HTML, /class="mode segmented"/);
  assert.match(SHELL_CSS, /\.segmented > button:last-child \{\s*box-shadow: none;/);
});

test("the window row groups its controls and names each group", () => {
  const groups = [...INDEX_HTML.matchAll(/<span class="nav-group" role="group" aria-label="([^"]+)"/g)];
  assert.equal(groups.length, 3, "move, aim and history are three groups");
  for (const [, label] of groups) assert.ok(label.length > 8, `"${label}" says what the group does`);
});

test("the camera pad keeps the shape of the move it makes", () => {
  const pad = SHELL_CSS.match(/\.camera-pad \{([^}]*)\}/);
  assert.ok(pad, "the pad has a rule");
  assert.match(pad[1], /display: grid/);
  for (const [direction, area] of [
    ["up", "1 / 2"], ["left", "2 / 1"], ["down", "2 / 2"], ["right", "2 / 3"],
  ]) {
    assert.match(
      SHELL_CSS,
      new RegExp(`\\.camera-pad \\[data-camera="${direction}"\\] \\{\\s*grid-area: ${area};`),
      `${direction} sits where it points`
    );
  }
});

test("an empty scene names what is empty and which control widens it", () => {
  const empty = INDEX_HTML.match(/<div id="scene-empty"[^>]*>([\s\S]*?)<\/div>/);
  assert.ok(empty, "the empty state is one region");
  assert.match(empty[0], /role="status"/, "the empty state reaches a screen reader");
  assert.match(empty[1], /<strong>[^<]+<\/strong>/, "it leads with what happened");
  assert.match(empty[1], /class="hint"/, "it follows with the way out");
  for (const control of ["time window", "clock", "label filters"]) {
    assert.ok(empty[1].toLowerCase().includes(control), `the hint names the ${control}`);
  }
});

test("an empty evidence pane says what will be in it", () => {
  const empty = INDEX_HTML.match(/<div id="detail-empty"[\s\S]*?<\/div>/);
  assert.ok(empty, "the empty pane is one region that renderDetail can hide");
  assert.match(empty[0], /Pick a memory/, "it names the action that fills it");
  const items = [...empty[0].matchAll(/<li><strong>([^<]+)<\/strong>/g)].map(([, name]) => name);
  assert.deepEqual(items, ["The record", "Its four clocks", "Its relations"]);
  assert.match(empty[0], /occurred, observed, ingested and validity/, "the four clocks are named");
});

/* ---- the presentation behaviour that is not CSS ---- */

/* loom-panels.js under node with a DOM small enough to watch. */
function panels() {
  const nodes = [];
  const element = (tag) => {
    const node = {
      tag, className: "", children: [], listeners: {},
      style: {}, attributes: {}, hidden: false, tabIndex: undefined,
      append(...items) { this.children.push(...items); },
      replaceChildren(...items) { this.children = items; },
      addEventListener(event, handler) { this.listeners[event] = handler; },
      setAttribute(name, value) { this.attributes[name] = value; },
      querySelectorAll: () => [],
    };
    /* Assigning textContent empties the node, the way a real one does —
       the panels clear a list with `list.textContent = ""`. */
    let text = "";
    Object.defineProperty(node, "textContent", {
      get: () => text,
      set(value) { text = value; node.children = []; },
    });
    nodes.push(node);
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
  app.scene = { kindColor: () => "#888", classColor: () => "#888", requestDraw() {}, palette: () => ({}) };
  app.catalogue = { renderAbouts() {}, renderLabels() {} };
  app.selection = { selectEntry: () => true };
  app.viewport = { centerOn() {} };
  app.sync = { reportView() {} };
  vm.runInContext(read("loom-panels.js"), context, { filename: "loom-panels.js" });
  return { app, el: (id) => registry.get(id) || context.document.getElementById(id), nodes };
}

test("a search that matches nothing says so instead of emptying the rail", () => {
  const { app, el } = panels();
  const model = app.state.model;
  model.entries = [{ ref: "a:one", text: "keep the axes", kind: "decision", coords: [] }];
  model.byRef = new Map(model.entries.map((entry) => [entry.ref, entry]));
  el("search").value = "zzzznothingmatchesthis";
  app.panels.runSearch();
  const results = el("search-results").children;
  assert.equal(results.length, 1, "one row, not zero");
  assert.equal(results[0].className, "empty-hint");
  assert.match(results[0].textContent, /No memory in this window matches/);
});

test("a search result is reachable and operable from the keyboard", () => {
  const { app, el } = panels();
  const model = app.state.model;
  model.entries = [{ ref: "a:one", text: "keep the axes", kind: "decision", coords: [] }];
  model.byRef = new Map(model.entries.map((entry) => [entry.ref, entry]));
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
  assert.deepEqual(picked, ["a:one"], "Enter selects the same entry a click would");
  row.listeners.keydown({ key: "Tab", preventDefault: () => { prevented = "tab"; } });
  assert.deepEqual(picked, ["a:one"], "Tab still moves on");
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
  assert.deepEqual(reached, ["a:two"]);
});

test("dimming a memory kind reports its state rather than only its colour", () => {
  const { app, el } = panels();
  app.state.model.projection = { by_kind: { decision: 3 }, entries: [], labels: [] };
  app.state.model.layerProjections = [];
  app.state.model.edges = [];
  app.state.model.currentLod = "moment";
  app.panels.renderRail();
  const [row] = el("kind-legend").children;
  assert.equal(row.attributes["aria-pressed"], "false", "an undimmed kind says so");
  row.listeners.click();
  const [after] = el("kind-legend").children;
  assert.equal(after.attributes["aria-pressed"], "true", "a dimmed kind says so");
});
