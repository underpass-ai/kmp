/* KMP ChronoLoom — the loom itself.
   Time is the primary geometry: a horizontal clock you choose (occurred,
   observed, ingested, validity), memory dimensions as stable lanes, entries
   as marks joining lanes with braids, explanatory relations as class-styled
   arcs, and a semantic-zoom ladder — atlas, episode, moment — that changes
   representation, not just size. Evidence is a selection, not a zoom.

   Vanilla JS + the vendored pixi bundle. Pure logic lives in loom-core.js;
   this file is state, scene and gesture. Talks only to the sibling /api/*
   routes; nothing here invents data the kernel did not return. */
"use strict";

/* ---------------- helpers ---------------- */

const $ = (id) => document.getElementById(id);

async function api(path, params) {
  const query = params
    ? "?" +
      Object.entries(params)
        .filter(([, v]) => v !== undefined && v !== null && v !== "")
        .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
        .join("&")
    : "";
  const response = await fetch(path + query);
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error || `${path} failed with ${response.status}`);
  return body;
}

function showError(message) {
  $("error-slot").textContent = message || "";
  if (message) {
    clearTimeout(showError._timer);
    showError._timer = setTimeout(() => ($("error-slot").textContent = ""), 8000);
  }
}

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

const nowIso = () => new Date().toISOString().replace(/\.\d+Z$/, "Z");
const fmtMs = (ms) => new Date(ms).toISOString().slice(5, 16).replace("T", " ");
const fmtMsFull = (ms) => new Date(ms).toISOString().slice(0, 19).replace("T", " ");

/* ---------------- theme & palette ---------------- */

const THEMES = ["auto", "light", "dark"];
let themeIndex = 0;

function applyTheme() {
  const choice = THEMES[themeIndex];
  const dark =
    choice === "dark" || (choice === "auto" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  $("btn-theme").textContent = choice[0].toUpperCase() + choice.slice(1);
  paletteCache = null;
  resetTextPools();
  requestDraw();
}

$("btn-theme").addEventListener("click", () => {
  themeIndex = (themeIndex + 1) % THEMES.length;
  applyTheme();
});
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyTheme);

let paletteCache = null;

function palette() {
  if (!paletteCache) {
    const style = getComputedStyle(document.documentElement);
    const read = (name) => style.getPropertyValue(name).trim();
    paletteCache = {
      surface: read("--surface-1"),
      surface2: read("--surface-2"),
      text: read("--text-primary"),
      textMuted: read("--text-muted"),
      accent: read("--accent"),
      laneLine: read("--lane-line"),
      halo: read("--halo"),
      danger: read("--danger"),
      overflow: read("--kind-overflow"),
      kind: {
        memory_anchor: read("--kind-anchor"),
        about: read("--kind-anchor"),
        decision: read("--kind-decision"),
        memory_evidence: read("--kind-evidence"),
        evidence: read("--kind-evidence"),
        success_path: read("--kind-success"),
        error_path: read("--kind-error"),
        constraint: read("--kind-constraint"),
        observation: read("--kind-observation"),
        semantic_delta: read("--kind-delta"),
        preference: read("--kind-preference"),
        feedback: read("--kind-feedback"),
        memory_dimension: read("--kind-dimension"),
      },
      cls: {
        causal: read("--class-causal"),
        evidential: read("--class-evidential"),
        motivational: read("--class-motivational"),
        procedural: read("--class-procedural"),
        constraint: read("--class-constraint"),
        structural: read("--class-structural"),
      },
    };
  }
  return paletteCache;
}

const kindColor = (kind) => palette().kind[kind] || palette().overflow;
const classColor = (cls) => palette().cls[cls] || palette().cls.structural;

/* ---------------- state ---------------- */

let abouts = [];
const model = {
  about: null,
  entries: [], // entryModels, ordered
  byRef: new Map(),
  lanes: [], // [{name, index, count, scopes}]
  laneIndex: new Map(),
  edges: [], // explanatory arcs (classified)
  supersessions: [],
  contradictions: [],
  supersededRefs: new Set(),
  contradictedRefs: new Set(),
};

const view = {
  clock: "occurred",
  full: null, // {t0, t1} extent on the current clock
  t0: 0, // window
  t1: 1,
  windowStack: [],
  selectedRef: null,
  hiddenLanes: new Set(),
  dimmedKinds: new Set(),
  searchHits: new Set(),
  trace: null, // {from, to, refs:Set, hops:[]}
};

/* ---------------- data ---------------- */

const TRAVEL_MAX_ENTRIES = 2048;

async function fetchWholeLine(about) {
  const byRef = new Map();
  const absorb = (entries) => {
    let fresh = 0;
    for (const entry of entries) {
      if (!byRef.has(entry.ref_id)) {
        byRef.set(entry.ref_id, KMP_LOOM.entryModel(entry));
        fresh += 1;
      }
    }
    return fresh;
  };
  const first = await api("/api/timeline", {
    about,
    direction: "near",
    time: nowIso(),
    before: 256,
    after: 256,
  });
  absorb(first.entries);
  let cursor = first.page.next_cursor;
  for (let guard = 0; cursor && byRef.size < TRAVEL_MAX_ENTRIES && guard < 16; guard += 1) {
    const page = await api("/api/timeline", { about, direction: "forward", ref: cursor, after: 256 });
    if (!absorb(page.entries)) break;
    cursor = page.page.next_cursor;
  }
  for (let guard = 0; byRef.size < TRAVEL_MAX_ENTRIES && guard < 16; guard += 1) {
    const sorted = [...byRef.values()].sort(KMP_LOOM.compareModels);
    const earliest = sorted[0] && sorted[0].ref;
    if (!earliest) break;
    const page = await api("/api/timeline", { about, direction: "rewind", ref: earliest, before: 256 });
    if (!absorb(page.entries)) break;
  }
  return [...byRef.values()].sort(KMP_LOOM.compareModels);
}

async function loadAbout(about) {
  try {
    const [entries, graph] = await Promise.all([
      fetchWholeLine(about),
      api("/api/graph", { about, depth: 4 }),
    ]);
    model.about = about;
    model.entries = entries;
    model.byRef = new Map(entries.map((m) => [m.ref, m]));
    model.lanes = KMP_LOOM.buildLanes(entries);
    model.laneIndex = new Map(model.lanes.map((lane) => [lane.name, lane.index]));
    const classified = KMP_LOOM.classifyEdges(graph.edges || [], (ref) => model.byRef.has(ref));
    model.edges = classified.arcs;
    model.supersessions = classified.supersessions;
    model.contradictions = classified.contradictions;
    // supersedes: source replaces target — the target is history.
    model.supersededRefs = new Set(classified.supersessions.map((e) => e.target));
    model.contradictedRefs = new Set(
      classified.contradictions.flatMap((e) => [e.source, e.target])
    );
    view.selectedRef = null;
    view.trace = null;
    view.searchHits = new Set();
    view.hiddenLanes = new Set();
    $("trace-box").hidden = true;
    renderDetailEmpty();
    setClock(view.clock, true);
    renderAbouts();
    renderRail();
    renderStats();
    showError("");
    if (!sync.applying) viewOpen();
  } catch (error) {
    showError(error.message);
  }
}

/* ---------------- clock & window ---------------- */

function setClock(clock, reset) {
  view.clock = clock;
  for (const chip of document.querySelectorAll("#clock-chips .chip")) {
    chip.classList.toggle("active", chip.dataset.clock === clock);
  }
  view.full = KMP_LOOM.extent(model.entries, clock);
  if (!view.full) {
    showError("no entry carries any clock — the loom has no axis to weave on");
    return;
  }
  const pad = Math.max(1, (view.full.t1 - view.full.t0) * 0.02);
  view.full = { t0: view.full.t0 - pad, t1: view.full.t1 + pad };
  if (reset || view.t0 >= view.t1) {
    view.t0 = view.full.t0;
    view.t1 = view.full.t1;
    view.windowStack = [];
  } else {
    view.t0 = Math.max(view.t0, view.full.t0);
    view.t1 = Math.min(view.t1, view.full.t1);
  }
  renderStats();
  requestDraw();
  drawNavigator();
  reportView();
}

for (const chip of document.querySelectorAll("#clock-chips .chip")) {
  chip.addEventListener("click", () => setClock(chip.dataset.clock, false));
}

function setWindow(t0, t1, remember = true) {
  const minSpan = 1000; // one second floor
  const clamped0 = Math.max(view.full.t0, Math.min(t0, view.full.t1 - minSpan));
  const clamped1 = Math.min(view.full.t1, Math.max(t1, clamped0 + minSpan));
  if (remember) view.windowStack.push([view.t0, view.t1]);
  view.t0 = clamped0;
  view.t1 = clamped1;
  renderStats();
  requestDraw();
  drawNavigator();
  reportView();
}

$("nav-all").addEventListener("click", () => setWindow(view.full.t0, view.full.t1));
$("nav-back").addEventListener("click", () => {
  const previous = view.windowStack.pop();
  if (previous) setWindow(previous[0], previous[1], false);
});
$("btn-fit").addEventListener("click", () => setWindow(view.full.t0, view.full.t1));

/* ---------------- renderer ---------------- */

const canvas = $("loom-canvas");
let app = null;
let laneGfx = null;
let validityGfx = null;
let arcsGfx = null;
let braidGfx = null;
let marksGfx = null;
let selectGfx = null;
let textLayer = null;
const textPools = { lane: new Map(), axis: [], mark: new Map(), bubble: new Map() };
let dirty = true;
let hitList = []; // {x, y, r, kind: "entry"|"cluster", ref?, refs?, t0?, t1?}

function requestDraw() {
  dirty = true;
}

function resetTextPools() {
  if (!textLayer) return;
  for (const pool of [textPools.lane, textPools.mark, textPools.bubble]) {
    for (const label of pool.values()) label.destroy();
    pool.clear();
  }
  for (const label of textPools.axis) label.destroy();
  textPools.axis = [];
  textLayer.removeChildren();
  dirty = true;
}

async function setupRenderer() {
  if (typeof PIXI === "undefined") throw new Error("the vendored pixi.js bundle did not load");
  app = new PIXI.Application();
  await app.init({
    canvas,
    resizeTo: $("stage"),
    backgroundAlpha: 0,
    antialias: true,
    autoDensity: true,
    resolution: devicePixelRatio || 1,
  });
  laneGfx = new PIXI.Graphics();
  validityGfx = new PIXI.Graphics();
  arcsGfx = new PIXI.Graphics();
  braidGfx = new PIXI.Graphics();
  marksGfx = new PIXI.Graphics();
  selectGfx = new PIXI.Graphics();
  textLayer = new PIXI.Container();
  app.stage.addChild(laneGfx, validityGfx, arcsGfx, braidGfx, marksGfx, selectGfx, textLayer);
  app.renderer.on("resize", () => {
    dirty = true;
    drawNavigator();
  });
  app.ticker.add(() => {
    if (dirty) {
      dirty = false;
      renderLoom();
    }
  });
}

/* Layout: an axis strip on top, lanes below it, the navigator floating at
   the bottom of the stage (its height is reserved). */
const AXIS_H = 30;
const NAV_RESERVED = 96;
/// How many entry labels the scene keeps baked. Text is the most expensive
/// thing on the stage, so the pool is bounded — and evicts rather than
/// simply refusing to grow.
const LABEL_POOL_MAX = 400;

function visibleLanes() {
  return model.lanes.filter((lane) => !view.hiddenLanes.has(lane.name));
}

function laneGeometry() {
  const lanes = visibleLanes();
  const height = canvas.clientHeight - AXIS_H - NAV_RESERVED;
  const laneH = lanes.length ? Math.max(44, Math.min(150, height / lanes.length)) : height;
  const tops = new Map();
  lanes.forEach((lane, i) => tops.set(lane.name, AXIS_H + i * laneH));
  return { lanes, laneH, tops };
}

const xOf = (t) => ((t - view.t0) / (view.t1 - view.t0)) * canvas.clientWidth;
const tOf = (x) => view.t0 + (x / canvas.clientWidth) * (view.t1 - view.t0);

function laneText(key, text, size, color, alpha) {
  let label = textPools.lane.get(key);
  if (!label) {
    label = new PIXI.Text({
      text,
      style: {
        fontFamily: "ui-monospace, monospace",
        fontSize: size,
        fill: color,
        stroke: { color: palette().halo, width: 3, join: "round" },
      },
      resolution: (devicePixelRatio || 1) * 2,
    });
    textPools.lane.set(key, label);
    textLayer.addChild(label);
  } else if (label.text !== text) {
    label.text = text;
  }
  label.alpha = alpha;
  label.visible = true;
  return label;
}

/* Class-styled arc between two points: a quadratic bow, dashed by pattern.
   Direction is a small arrowhead at the target end. */
function drawArc(gfx, x1, y1, x2, y2, color, style, alphaScale) {
  const mx = (x1 + x2) / 2;
  const my = (y1 + y2) / 2;
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len = Math.max(1, Math.hypot(dx, dy));
  const lift = Math.min(60, len * 0.25);
  const cx = mx - (dy / len) * lift;
  const cy = my + (dx / len) * lift;
  const alpha = style.alpha * alphaScale;
  if (alpha <= 0.02) return;

  const steps = Math.max(8, Math.min(48, Math.floor(len / 12)));
  const points = [];
  for (let i = 0; i <= steps; i += 1) {
    const t = i / steps;
    const a = 1 - t;
    points.push([
      a * a * x1 + 2 * a * t * cx + t * t * x2,
      a * a * y1 + 2 * a * t * cy + t * t * y2,
    ]);
  }
  if (!style.dash) {
    gfx.moveTo(points[0][0], points[0][1]);
    for (let i = 1; i < points.length; i += 1) gfx.lineTo(points[i][0], points[i][1]);
    gfx.stroke({ width: style.width, color, alpha });
  } else {
    // Walk the sampled curve, alternating pen-down segments per the pattern.
    let patternIndex = 0;
    let remaining = style.dash[0];
    let penDown = true;
    let prev = points[0];
    for (let i = 1; i < points.length; i += 1) {
      let current = prev;
      let target = points[i];
      let segLen = Math.hypot(target[0] - current[0], target[1] - current[1]);
      while (segLen > 0.01) {
        const take = Math.min(segLen, remaining);
        const ratio = take / segLen;
        const nx = current[0] + (target[0] - current[0]) * ratio;
        const ny = current[1] + (target[1] - current[1]) * ratio;
        if (penDown) {
          gfx.moveTo(current[0], current[1]).lineTo(nx, ny);
        }
        current = [nx, ny];
        segLen -= take;
        remaining -= take;
        if (remaining <= 0.01) {
          penDown = !penDown;
          patternIndex = (patternIndex + 1) % style.dash.length;
          remaining = style.dash[patternIndex];
        }
      }
      prev = target;
    }
    gfx.stroke({ width: style.width, color, alpha });
  }
  if (style.arrow) {
    const [px, py] = points[points.length - 2];
    const angle = Math.atan2(y2 - py, x2 - px);
    const size = 5 + style.width;
    gfx
      .poly([
        x2,
        y2,
        x2 - size * Math.cos(angle - 0.4),
        y2 - size * Math.sin(angle - 0.4),
        x2 - size * Math.cos(angle + 0.4),
        y2 - size * Math.sin(angle + 0.4),
      ])
      .fill({ color, alpha });
  }
}

function entryAlpha(m) {
  let a = 1;
  if (model.supersededRefs.has(m.ref)) a = 0.35; // history, not garbage
  if (view.dimmedKinds.has(m.kind)) a = Math.min(a, 0.15);
  if (view.searchHits.size) a = view.searchHits.has(m.ref) ? 1 : Math.min(a, 0.15);
  if (view.trace) a = view.trace.refs.has(m.ref) ? 1 : Math.min(a, 0.12);
  return a;
}

function renderLoom() {
  if (!app || !view.full) return;
  const p = palette();
  const width = canvas.clientWidth;
  const geometry = laneGeometry();
  const msPerPx = (view.t1 - view.t0) / Math.max(1, width);
  const lod = KMP_LOOM.lodFor(msPerPx);
  $("lod-chip").textContent =
    lod === "atlas" ? "Atlas · density" : lod === "episode" ? "Episode · braids" : "Moment · entries";

  laneGfx.clear();
  validityGfx.clear();
  arcsGfx.clear();
  braidGfx.clear();
  marksGfx.clear();
  selectGfx.clear();
  hitList = [];
  const wanted = new Set();

  // Lanes: alternating quiet bands + name.
  geometry.lanes.forEach((lane, i) => {
    const top = geometry.tops.get(lane.name);
    if (i % 2 === 1) {
      laneGfx.rect(0, top, width, geometry.laneH).fill({ color: p.surface2, alpha: 0.35 });
    }
    laneGfx
      .moveTo(0, top + geometry.laneH)
      .lineTo(width, top + geometry.laneH)
      .stroke({ width: 1, color: p.laneLine, alpha: 0.8 });
    const key = `lane:${lane.name}`;
    wanted.add(key);
    const label = laneText(key, lane.name, 11, p.textMuted, 0.9);
    label.position.set(8, top + 6);
  });

  // Axis: gridlines through the lanes, labels in the strip above.
  const axis = KMP_LOOM.axisTicks(view.t0, view.t1, Math.max(3, Math.floor(width / 110)));
  let axisIndex = 0;
  for (const t of axis.ticks) {
    const x = xOf(t);
    laneGfx
      .moveTo(x, AXIS_H)
      .lineTo(x, canvas.clientHeight - NAV_RESERVED)
      .stroke({ width: 1, color: p.laneLine, alpha: 0.6 });
    let label = textPools.axis[axisIndex];
    if (!label) {
      label = new PIXI.Text({
        text: "",
        style: { fontFamily: "ui-monospace, monospace", fontSize: 10, fill: p.textMuted },
        resolution: (devicePixelRatio || 1) * 2,
      });
      textPools.axis.push(label);
      textLayer.addChild(label);
    }
    label.text = KMP_LOOM.tickLabel(t, axis.step);
    label.style.fill = p.textMuted;
    label.visible = true;
    label.anchor.set(0.5, 0);
    label.position.set(x, 8);
    axisIndex += 1;
  }
  for (let i = axisIndex; i < textPools.axis.length; i += 1) textPools.axis[i].visible = false;

  const laneMid = (name) => {
    const top = geometry.tops.get(name);
    return top === undefined ? null : top + geometry.laneH / 2;
  };

  if (lod === "atlas") {
    renderAtlas(p, geometry, width);
  } else {
    renderWeave(p, geometry, width, lod, msPerPx, laneMid);
  }

  // Hide labels not used this frame.
  for (const [key, label] of textPools.lane) {
    if (!wanted.has(key) && key.startsWith("lane:")) label.visible = false;
  }
}

/* Atlas: density ribbons per lane — the shape of the memory, no nodes. */
function renderAtlas(p, geometry, width) {
  const buckets = Math.max(24, Math.floor(width / 7));
  const bins = KMP_LOOM.laneBins(
    model.entries,
    view.clock,
    (dim) => model.laneIndex.get(dim),
    model.lanes.length,
    view.t0,
    view.t1,
    buckets
  );
  const barW = width / buckets;
  for (const lane of geometry.lanes) {
    const row = bins.grid[lane.index];
    const base = geometry.tops.get(lane.name) + geometry.laneH - 6;
    const maxH = geometry.laneH - 16;
    for (let b = 0; b < buckets; b += 1) {
      const cell = row[b];
      if (!cell.total) continue;
      const barH = Math.max(2, (cell.total / bins.max) * maxH);
      let y = base;
      const slices = [...cell.byKind.entries()].sort((a, c) => c[1] - a[1]);
      for (const [kind, count] of slices) {
        const sliceH = (count / cell.total) * barH;
        marksGfx
          .rect(b * barW + 0.5, y - sliceH, Math.max(1, barW - 1), sliceH)
          .fill({ color: kindColor(kind), alpha: 0.8 });
        y -= sliceH;
      }
      hitList.push({
        x: b * barW + barW / 2,
        y: base - barH / 2,
        r: Math.max(barW, 10),
        kind: "cluster",
        refs: null,
        count: cell.total,
        t0: view.t0 + (b / buckets) * (view.t1 - view.t0),
        t1: view.t0 + ((b + 1) / buckets) * (view.t1 - view.t0),
      });
    }
  }
}

/* Episode & moment: entries (or tight clusters) on their lanes, braided. */
function renderWeave(p, geometry, width, lod, msPerPx, laneMid) {
  const minGap = lod === "episode" ? 18 * msPerPx : 0;
  const clusters = KMP_LOOM.laneClusters(model.entries, view.clock, view.t0, view.t1, minGap);
  const drawnAt = new Map(); // ref -> [{x, y}]
  const wantedMarks = new Set();
  const wantedBubbles = new Set();

  for (const lane of geometry.lanes) {
    const laneClusters = clusters.get(lane.name) || [];
    const y = laneMid(lane.name);
    if (y === null) continue;
    for (const cluster of laneClusters) {
      const x = xOf(cluster.t);
      if (x < -40 || x > width + 40) continue;
      if (cluster.refs.length > 1) {
        // A bundle of memory too tight to split at this zoom.
        const r = Math.min(18, 7 + 2.5 * Math.log2(cluster.refs.length + 1));
        const slices = [...cluster.byKind.entries()].sort((a, c) => c[1] - a[1]);
        marksGfx.circle(x, y, r).fill({ color: p.surface, alpha: 0.9 });
        let angle = -Math.PI / 2;
        const total = cluster.refs.length;
        for (const [kind, count] of slices) {
          const sweep = (count / total) * Math.PI * 2;
          // moveTo first: a bare arc() drags a line in from the pen's last
          // resting place, which after a stroke is the origin.
          marksGfx
            .moveTo(x + (r - 1.5) * Math.cos(angle), y + (r - 1.5) * Math.sin(angle))
            .arc(x, y, r - 1.5, angle, angle + sweep)
            .stroke({ width: 3, color: kindColor(kind), alpha: 0.95 });
          angle += sweep;
        }
        const key = `bubble:${lane.name}:${Math.round(x)}`;
        wantedBubbles.add(key);
        let label = textPools.bubble.get(key);
        if (!label) {
          label = new PIXI.Text({
            text: "",
            style: { fontFamily: "ui-monospace, monospace", fontSize: 10, fill: p.text },
            resolution: (devicePixelRatio || 1) * 2,
          });
          textPools.bubble.set(key, label);
          textLayer.addChild(label);
        }
        label.text = String(total);
        label.style.fill = p.text;
        label.anchor.set(0.5, 0.5);
        label.position.set(x, y);
        label.visible = true;
        hitList.push({
          x,
          y,
          r: r + 4,
          kind: "cluster",
          refs: cluster.refs,
          count: total,
          t0: cluster.t - Math.max(minGap, 1) * 2,
          t1: cluster.t + Math.max(minGap, 1) * 2,
        });
      } else {
        const ref = cluster.refs[0];
        const m = model.byRef.get(ref);
        const a = entryAlpha(m);
        if (a <= 0.02) continue;
        const strict = KMP_LOOM.strictMs(m, view.clock) !== null;
        const r = view.selectedRef === ref ? 7 : 5.5;
        if (strict) {
          marksGfx.circle(x, y, r).fill({ color: kindColor(m.kind), alpha: a });
        } else {
          // Hollow: sitting at its precedence fallback, not its own clock.
          marksGfx.circle(x, y, r).stroke({ width: 2, color: kindColor(m.kind), alpha: a });
        }
        if (model.contradictedRefs.has(m.ref)) {
          marksGfx
            .poly([x - 4, y - r - 3, x + 4, y - r - 3, x, y - r - 9])
            .fill({ color: p.danger, alpha: a });
        }
        let bucket = drawnAt.get(ref);
        if (!bucket) drawnAt.set(ref, (bucket = []));
        bucket.push({ x, y });
        hitList.push({ x, y, r: r + 4, kind: "entry", ref });
      }
    }
  }

  // Braids: one entry woven through several lanes — a vertical thread.
  for (const [ref, spots] of drawnAt) {
    if (spots.length < 2) continue;
    const a = entryAlpha(model.byRef.get(ref)) * 0.7;
    const sorted = [...spots].sort((s, t) => s.y - t.y);
    for (let i = 1; i < sorted.length; i += 1) {
      braidGfx
        .moveTo(sorted[i - 1].x, sorted[i - 1].y)
        .lineTo(sorted[i].x, sorted[i].y)
        .stroke({ width: 1.2, color: p.accent, alpha: a * 0.5 });
    }
  }

  // Validity: on the validity clock, facts stretch for as long as they held.
  if (view.clock === "validity") {
    for (const m of model.entries) {
      const from = m.clocks.validFrom;
      if (from === null) continue;
      const until = m.clocks.validUntil ?? view.t1;
      const seen = new Set();
      for (const coord of m.coords) {
        if (seen.has(coord.dimension)) continue;
        seen.add(coord.dimension);
        const y = laneMid(coord.dimension);
        if (y === null) continue;
        validityGfx
          .rect(xOf(from), y - 2, Math.max(2, xOf(until) - xOf(from)), 4)
          .fill({ color: kindColor(m.kind), alpha: 0.25 * entryAlpha(m) });
      }
    }
  }

  // Arcs: explanatory relations between drawn entries, styled by class.
  if (lod === "moment" || view.trace) {
    for (const edge of model.edges) {
      const from = drawnAt.get(edge.source);
      const to = drawnAt.get(edge.target);
      if (!from || !to) continue;
      const inTrace = view.trace && view.trace.edgeKeys.has(`${edge.source} ${edge.rel} ${edge.target}`);
      if (lod !== "moment" && !inTrace) continue;
      const style = KMP_LOOM.arcStyle(edge.class);
      const aScale = view.trace ? (inTrace ? 1.4 : 0.15) : 1;
      drawArc(
        arcsGfx,
        from[0].x,
        from[0].y,
        to[0].x,
        to[0].y,
        inTrace ? p.accent : classColor(edge.class),
        style,
        Math.min(entryAlpha(model.byRef.get(edge.source)), entryAlpha(model.byRef.get(edge.target))) * aScale
      );
    }
    // Supersession: a quiet grey tie from the replacement to its history.
    for (const edge of model.supersessions) {
      const from = drawnAt.get(edge.source);
      const to = drawnAt.get(edge.target);
      if (!from || !to) continue;
      drawArc(arcsGfx, from[0].x, from[0].y, to[0].x, to[0].y, p.textMuted, { dash: [2, 4], width: 1, alpha: 0.4, arrow: false }, 1);
    }
    // Contradiction: both claims alive — a danger-colored zigzag between them.
    for (const edge of model.contradictions) {
      const from = drawnAt.get(edge.source);
      const to = drawnAt.get(edge.target);
      if (!from || !to) continue;
      drawArc(arcsGfx, from[0].x, from[0].y, to[0].x, to[0].y, p.danger, { dash: [4, 3], width: 1.8, alpha: 0.9, arrow: false }, 1);
    }
  }

  // Selection ring + glow.
  if (view.selectedRef) {
    const spots = drawnAt.get(view.selectedRef);
    if (spots) {
      for (const spot of spots) {
        selectGfx.circle(spot.x, spot.y, 10).stroke({ width: 2.5, color: p.accent, alpha: 1 });
        selectGfx.circle(spot.x, spot.y, 14).stroke({ width: 6, color: p.accent, alpha: 0.18 });
      }
    }
  }

  // Entry labels, only when there is room to own the words.
  const showLabels = msPerPx < 4000;
  for (const [key, label] of textPools.mark) label.visible = false;
  if (showLabels) {
    for (const item of hitList) {
      if (item.kind !== "entry") continue;
      const m = model.byRef.get(item.ref);
      if (entryAlpha(m) < 0.2) continue;
      const key = `mark:${item.ref}`;
      let label = textPools.mark.get(key);
      const text = m.text.length > 34 ? m.text.slice(0, 33) + "…" : m.text;
      if (!label && textPools.mark.size >= LABEL_POOL_MAX) {
        // Evict, do not stop labelling: a bare cap meant that once the pool
        // filled, entries stopped getting names for the rest of the session.
        for (const [oldKey, oldLabel] of textPools.mark) {
          if (oldLabel.visible) continue;
          oldLabel.destroy();
          textPools.mark.delete(oldKey);
          break;
        }
      }
      if (!label) {
        label = new PIXI.Text({
          text,
          style: {
            fontFamily: "system-ui, sans-serif",
            fontSize: 11,
            fill: palette().text,
            stroke: { color: palette().halo, width: 3, join: "round" },
          },
          resolution: (devicePixelRatio || 1) * 2,
        });
        textPools.mark.set(key, label);
        textLayer.addChild(label);
      }
      label.alpha = entryAlpha(m);
      label.visible = true;
      label.anchor.set(0, 0.5);
      label.position.set(item.x + 10, item.y - 12);
    }
  }

  for (const [key, label] of textPools.bubble) {
    if (!wantedBubbles.has(key)) label.visible = false;
  }
}

/* ---------------- the navigator ---------------- */

function drawNavigator() {
  const strip = $("nav-canvas");
  if (!view.full || !model.entries.length) return;
  const width = strip.clientWidth || 1;
  const height = 42;
  const dpr = devicePixelRatio || 1;
  if (strip.width !== Math.round(width * dpr)) strip.width = Math.round(width * dpr);
  if (strip.height !== Math.round(height * dpr)) strip.height = Math.round(height * dpr);
  const pen = strip.getContext("2d");
  pen.setTransform(dpr, 0, 0, dpr, 0, 0);
  pen.clearRect(0, 0, width, height);
  const p = palette();
  const span = view.full.t1 - view.full.t0;
  const xAt = (t) => ((t - view.full.t0) / span) * width;

  // Density over the whole line, stacked by kind.
  const buckets = Math.max(40, Math.floor(width / 6));
  const counts = Array.from({ length: buckets }, () => new Map());
  let max = 1;
  for (const m of model.entries) {
    const t = KMP_LOOM.placedMs(m, view.clock);
    if (t === null) continue;
    const b = Math.max(0, Math.min(buckets - 1, Math.floor(((t - view.full.t0) / span) * buckets)));
    const cell = counts[b];
    cell.set(m.kind, (cell.get(m.kind) || 0) + 1);
    const total = [...cell.values()].reduce((a, c) => a + c, 0);
    if (total > max) max = total;
  }
  const barW = width / buckets;
  for (let b = 0; b < buckets; b += 1) {
    const cell = counts[b];
    if (!cell.size) continue;
    const total = [...cell.values()].reduce((a, c) => a + c, 0);
    const barH = Math.max(2, (total / max) * (height - 8));
    let y = height - 3;
    for (const [kind, count] of [...cell.entries()].sort((a, c) => c[1] - a[1])) {
      const sliceH = (count / total) * barH;
      pen.fillStyle = kindColor(kind);
      pen.globalAlpha = 0.85;
      pen.fillRect(b * barW + 0.5, y - sliceH, Math.max(1, barW - 1), sliceH);
      y -= sliceH;
    }
  }
  pen.globalAlpha = 1;

  // The windowpane.
  const a = xAt(view.t0);
  const b = xAt(view.t1);
  pen.fillStyle = "rgba(72, 120, 224, 0.16)";
  pen.fillRect(a, 0, Math.max(2, b - a), height);
  pen.fillStyle = p.accent;
  pen.fillRect(a - 1, 0, 2, height);
  pen.fillRect(b - 1, 0, 2, height);
  pen.fillRect(a - 1, height / 2 - 5, 3, 10);
  pen.fillRect(b - 2, height / 2 - 5, 3, 10);

  $("nav-range").textContent = `${fmtMs(view.t0)} → ${fmtMs(view.t1)}`;
  $("nav-note").textContent = view.clock;
}

/* Navigator gestures: pan the pane, resize its edges, cut a fresh window,
   click to center. */
let navDrag = null;
const NAV_HANDLE = 6;

$("nav-canvas").addEventListener("pointerdown", (event) => {
  $("nav-canvas").setPointerCapture(event.pointerId);
  const width = $("nav-canvas").clientWidth || 1;
  const span = view.full.t1 - view.full.t0;
  const xAt = (t) => ((t - view.full.t0) / span) * width;
  const x = event.offsetX;
  const a = xAt(view.t0);
  const b = xAt(view.t1);
  const windowed = view.t0 > view.full.t0 || view.t1 < view.full.t1;
  let mode = "select";
  if (windowed && Math.abs(x - a) <= NAV_HANDLE) mode = "resize-l";
  else if (windowed && Math.abs(x - b) <= NAV_HANDLE) mode = "resize-r";
  else if (windowed && x > a && x < b) mode = "pan";
  navDrag = { mode, x0: x, moved: false, t0: view.t0, t1: view.t1, remembered: false };
});

$("nav-canvas").addEventListener("pointermove", (event) => {
  if (!navDrag) return;
  const width = $("nav-canvas").clientWidth || 1;
  const span = view.full.t1 - view.full.t0;
  const x = event.offsetX;
  if (!navDrag.moved && Math.abs(x - navDrag.x0) <= 4) return;
  navDrag.moved = true;
  const msOf = (px) => (px / width) * span;
  if (!navDrag.remembered && navDrag.mode !== "select") {
    view.windowStack.push([navDrag.t0, navDrag.t1]);
    navDrag.remembered = true;
  }
  if (navDrag.mode === "pan") {
    const delta = msOf(x - navDrag.x0);
    const size = navDrag.t1 - navDrag.t0;
    let t0 = navDrag.t0 + delta;
    t0 = Math.max(view.full.t0, Math.min(view.full.t1 - size, t0));
    setWindow(t0, t0 + size, false);
  } else if (navDrag.mode === "resize-l") {
    setWindow(view.full.t0 + msOf(x), navDrag.t1, false);
  } else if (navDrag.mode === "resize-r") {
    setWindow(navDrag.t0, view.full.t0 + msOf(x), false);
  } else {
    // Painting a fresh selection — live preview via the window itself.
    if (!navDrag.remembered) {
      view.windowStack.push([navDrag.t0, navDrag.t1]);
      navDrag.remembered = true;
    }
    const lo = view.full.t0 + msOf(Math.min(navDrag.x0, x));
    const hi = view.full.t0 + msOf(Math.max(navDrag.x0, x));
    setWindow(lo, hi, false);
  }
});

$("nav-canvas").addEventListener("pointerup", (event) => {
  if (!navDrag) return;
  const drag = navDrag;
  navDrag = null;
  if (!drag.moved && drag.mode === "select") {
    // A click on the open line: carry the window there, centered.
    const width = $("nav-canvas").clientWidth || 1;
    const span = view.full.t1 - view.full.t0;
    const center = view.full.t0 + (event.offsetX / width) * span;
    const size = drag.t1 - drag.t0;
    let t0 = center - size / 2;
    t0 = Math.max(view.full.t0, Math.min(view.full.t1 - size, t0));
    setWindow(t0, t0 + size);
  }
});
$("nav-canvas").addEventListener("pointercancel", () => (navDrag = null));

/* ---------------- loom gestures ---------------- */

let loomDrag = null;

canvas.addEventListener("pointerdown", (event) => {
  canvas.setPointerCapture(event.pointerId);
  canvas.classList.add("dragging");
  loomDrag = { x0: event.offsetX, t0: view.t0, t1: view.t1, moved: false };
});

canvas.addEventListener("pointermove", (event) => {
  if (loomDrag) {
    const dx = event.offsetX - loomDrag.x0;
    if (loomDrag.moved || Math.abs(dx) > 4) {
      loomDrag.moved = true;
      const span = loomDrag.t1 - loomDrag.t0;
      const delta = (-dx / Math.max(1, canvas.clientWidth)) * span;
      let t0 = loomDrag.t0 + delta;
      t0 = Math.max(view.full.t0, Math.min(view.full.t1 - span, t0));
      setWindow(t0, t0 + span, false);
    }
    return;
  }
  const hit = hitAt(event.offsetX, event.offsetY);
  updateTooltip(hit, event.offsetX, event.offsetY);
});

canvas.addEventListener("pointerup", (event) => {
  canvas.classList.remove("dragging");
  if (!loomDrag) return;
  const drag = loomDrag;
  loomDrag = null;
  if (drag.moved) return;
  const hit = hitAt(event.offsetX, event.offsetY);
  if (!hit) {
    view.selectedRef = null;
    renderDetailEmpty();
    requestDraw();
    return;
  }
  if (hit.kind === "cluster") {
    // Open the weave: zoom into the bundle's span.
    const pad = Math.max(1000, (hit.t1 - hit.t0) * 0.35);
    setWindow(hit.t0 - pad, hit.t1 + pad);
    return;
  }
  selectEntry(hit.ref);
});

canvas.addEventListener("pointerleave", () => {
  $("tooltip").hidden = true;
});

canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    const factor = Math.exp(event.deltaY * 0.0014);
    const span = (view.t1 - view.t0) * factor;
    const fullSpan = view.full.t1 - view.full.t0;
    const clampedSpan = Math.max(1000, Math.min(fullSpan, span));
    const anchor = tOf(event.offsetX);
    const ratio = (anchor - view.t0) / (view.t1 - view.t0);
    let t0 = anchor - clampedSpan * ratio;
    t0 = Math.max(view.full.t0, Math.min(view.full.t1 - clampedSpan, t0));
    setWindow(t0, t0 + clampedSpan, false);
  },
  { passive: false }
);

addEventListener("keydown", (event) => {
  const target = event.target;
  if (target && (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA")) return;
  const span = view.t1 - view.t0;
  if (event.key === "f" || event.key === "F") setWindow(view.full.t0, view.full.t1);
  else if (event.key === "ArrowLeft") setWindow(view.t0 - span * 0.25, view.t1 - span * 0.25, false);
  else if (event.key === "ArrowRight") setWindow(view.t0 + span * 0.25, view.t1 + span * 0.25, false);
  else if (event.key === "+" || event.key === "=") setWindow(view.t0 + span * 0.2, view.t1 - span * 0.2);
  else if (event.key === "-") setWindow(view.t0 - span * 0.3, view.t1 + span * 0.3);
});

function hitAt(x, y) {
  let best = null;
  let bestDist = Infinity;
  for (const item of hitList) {
    const dist = Math.hypot(item.x - x, item.y - y);
    if (dist <= item.r + 3 && dist < bestDist) {
      best = item;
      bestDist = dist;
    }
  }
  return best;
}

function updateTooltip(hit, sx, sy) {
  const tooltip = $("tooltip");
  tooltip.textContent = "";
  if (!hit) {
    tooltip.hidden = true;
    return;
  }
  if (hit.kind === "cluster") {
    tooltip.append(el("div", "tt-title", `${hit.count} entries`));
    tooltip.append(el("div", "tt-sub", "click to open this stretch of the weave"));
  } else {
    const m = model.byRef.get(hit.ref);
    tooltip.append(el("div", "tt-title", m.text.length > 90 ? m.text.slice(0, 89) + "…" : m.text));
    const strict = KMP_LOOM.strictMs(m, view.clock) !== null;
    tooltip.append(
      el("div", "tt-sub", `${m.kind}${strict ? "" : " · placed by fallback — no " + view.clock + " clock"}`)
    );
    const t = KMP_LOOM.placedMs(m, view.clock);
    if (t !== null) tooltip.append(el("div", "tt-sub", fmtMsFull(t)));
    if (model.supersededRefs.has(m.ref)) tooltip.append(el("div", "tt-quote", "superseded — history, still true then"));
    if (model.contradictedRefs.has(m.ref)) tooltip.append(el("div", "tt-quote", "contradicted — both claims still alive"));
  }
  tooltip.hidden = false;
  const stage = $("stage").getBoundingClientRect();
  tooltip.style.left = `${Math.max(4, Math.min(sx + 14, stage.width - tooltip.offsetWidth - 8))}px`;
  tooltip.style.top = `${Math.max(4, Math.min(sy + 14, stage.height - tooltip.offsetHeight - 8))}px`;
}

/* ---------------- evidence: detail + prism ---------------- */

function renderDetailEmpty() {
  $("detail-empty").hidden = false;
  $("detail-body").hidden = true;
}

async function selectEntry(ref) {
  view.selectedRef = ref;
  requestDraw();
  reportView();
  const m = model.byRef.get(ref);
  renderPrism(m);
  try {
    const inspect = await api("/api/node", { id: ref, raw: "1" });
    renderDetail(inspect, m);
  } catch (error) {
    showError(error.message);
  }
}

function renderDetail(inspect, m) {
  const { node } = inspect;
  $("detail-empty").hidden = true;
  $("detail-body").hidden = false;
  const kindPill = $("d-kind");
  kindPill.textContent = "";
  const dot = el("span", "legend-dot");
  dot.style.background = kindColor(node.kind);
  kindPill.append(dot, document.createTextNode(node.kind));
  $("d-status").textContent = node.status || "no status";
  $("d-title").textContent = node.title;
  $("d-id").textContent = node.id;
  $("d-summary").textContent = node.summary || "";
  $("d-detail").textContent =
    (inspect.detail && inspect.detail.detail) || "(no detail recorded)";

  const coords = $("d-coords");
  coords.textContent = "";
  for (const c of m.coords) {
    coords.append(
      el(
        "li",
        "",
        `${c.dimension} / ${c.scope}` +
          (c.sequence !== null ? ` · #${c.sequence}` : "") +
          (c.rank !== null ? ` · rank ${c.rank}` : "")
      )
    );
  }

  renderRelationList($("d-incoming"), inspect.incoming, "source");
  renderRelationList($("d-outgoing"), inspect.outgoing, "target");
}

function renderRelationList(list, relations, counterpart) {
  list.textContent = "";
  if (!relations.length) {
    list.append(el("li", "muted", "none"));
    return;
  }
  const link = (id) => {
    const anchor = el("a", "rel-target mono", id);
    anchor.addEventListener("click", () => {
      if (model.byRef.has(id)) {
        selectEntry(id);
        centerOn(id);
      }
    });
    return anchor;
  };
  for (const relation of relations) {
    const item = el("li");
    const head = el("div", "rel-head");
    const dash = el("span", "legend-dash");
    dash.style.borderTopColor = classColor(relation.class);
    head.append(dash, el("span", "rel-type", relation.rel), el("span", "pill pill-muted", relation.class));
    if (relation.confidence) head.append(el("span", "pill pill-muted", relation.confidence));
    head.append(link(relation[counterpart]));
    item.append(head);
    if (relation.why) item.append(el("p", "rel-why", relation.why));
    if (relation.evidence) item.append(el("p", "rel-evidence", `evidence: ${relation.evidence}`));
    list.append(item);
  }
}

/* The polytemporal prism: reality, perception, persistence, order — with a
   gradient thread from occurred to ingested making late observation and
   backfill visible. Absent clocks stay visibly absent. */
function renderPrism(m) {
  const box = $("prism");
  box.textContent = "";
  const prism = KMP_LOOM.prism(m);
  const rails = [
    ["reality", prism.rails.occurred, "occurred"],
    ["perception", prism.rails.observed, "observed"],
    ["persistence", prism.rails.ingested, "ingested"],
  ];
  const span = prism.span;
  const posOf = (t) => (span ? ((t - span.t0) / (span.t1 - span.t0)) * 100 : 50);
  const dots = [];
  for (const [name, t] of rails) {
    const row = el("div", "prism-rail");
    row.append(el("span", "prism-name", name));
    const track = el("span", "prism-track");
    track.append(el("span", "prism-line"));
    if (t !== null) {
      const dot = el("span", "prism-dot");
      dot.style.left = `${posOf(t)}%`;
      dot.style.background =
        name === "reality" ? palette().cls.causal : name === "perception" ? palette().cls.constraint : palette().cls.evidential;
      track.append(dot);
      dots.push(posOf(t));
      row.append(track, el("span", "prism-when mono", fmtMsFull(t).slice(5)));
    } else {
      row.append(track, el("span", "prism-absent", "not recorded"));
    }
    box.append(row);
  }
  // The thread: occurred → ingested, the distance between "it was true"
  // and "KMP knew it".
  if (dots.length >= 2) {
    const thread = el("div", "prism-rail");
    thread.append(el("span", "prism-name", "thread"));
    const track = el("span", "prism-track");
    const line = el("span", "prism-thread");
    const lo = Math.min(...dots);
    const hi = Math.max(...dots);
    line.style.left = `${lo}%`;
    line.style.width = `${Math.max(1, hi - lo)}%`;
    track.append(line);
    thread.append(track, el("span", "prism-when", ""));
    box.append(thread);
  }
  if (prism.rails.validity) {
    const row = el("div", "prism-rail");
    row.append(el("span", "prism-name", "validity"));
    const track = el("span", "prism-track");
    track.append(el("span", "prism-line"));
    const band = el("span", "prism-band");
    const from = prism.rails.validity.from;
    const until = prism.rails.validity.until;
    band.style.left = `${from !== null ? posOf(from) : 0}%`;
    band.style.width = `${Math.max(4, (until !== null ? posOf(until) : 100) - (from !== null ? posOf(from) : 0))}%`;
    band.style.background = palette().accent;
    track.append(band);
    row.append(track, el("span", "prism-when mono", until === null ? "open" : fmtMsFull(until).slice(5)));
    box.append(row);
  }
  for (const order of prism.order) {
    const row = el("div", "prism-rail");
    row.append(el("span", "prism-name", "order"));
    row.append(
      el(
        "span",
        "mono muted",
        `${order.dimension}${order.sequence !== null ? " #" + order.sequence : ""}${order.rank !== null ? " · rank " + order.rank : ""}`
      )
    );
    box.append(row);
  }
}

function centerOn(ref) {
  const m = model.byRef.get(ref);
  if (!m) return;
  const t = KMP_LOOM.placedMs(m, view.clock);
  if (t === null) return;
  const span = view.t1 - view.t0;
  if (t < view.t0 || t > view.t1) setWindow(t - span / 2, t + span / 2);
}

/* ---------------- trace: the audit path ---------------- */

let tracePick = { from: null, to: null };

$("d-trace-from").addEventListener("click", () => {
  tracePick.from = view.selectedRef;
  if (tracePick.from && tracePick.to) runTrace();
});
$("d-trace-to").addEventListener("click", () => {
  tracePick.to = view.selectedRef;
  if (tracePick.from && tracePick.to) runTrace();
});
$("d-clear-trace").addEventListener("click", () => {
  view.trace = null;
  tracePick = { from: null, to: null };
  $("trace-box").hidden = true;
  $("d-clear-trace").hidden = true;
  requestDraw();
});

async function runTrace() {
  try {
    const trace = await api("/api/trace", { from: tracePick.from, to: tracePick.to });
    view.trace = {
      refs: new Set(trace.nodes.map((n) => n.id)),
      edgeKeys: new Set(trace.edges.map((e) => `${e.source} ${e.rel} ${e.target}`)),
    };
    $("trace-box").hidden = false;
    $("d-clear-trace").hidden = false;
    $("trace-status").textContent = `${trace.edges.length} hop${trace.edges.length === 1 ? "" : "s"} · ${trace.rendered.token_count} tokens rendered`;
    const list = $("trace-hops");
    list.textContent = "";
    for (const edge of trace.edges) {
      const item = el("li");
      const head = el("div", "rel-head");
      const dash = el("span", "legend-dash");
      dash.style.borderTopColor = classColor(edge.class);
      head.append(dash, el("span", "rel-type", edge.rel), el("span", "pill pill-muted", edge.class));
      head.append(el("span", "mono muted", `${edge.source} → ${edge.target}`));
      item.append(head);
      if (edge.why) item.append(el("p", "rel-why", edge.why));
      list.append(item);
    }
    requestDraw();
    reportView();
    showError("");
  } catch (error) {
    showError(error.message);
  }
}

/* ---------------- rail: abouts, lanes, legends, search ---------------- */

function renderAbouts() {
  const list = $("about-list");
  list.textContent = "";
  for (const about of abouts) {
    const item = el("li", about === model.about ? "active" : "", about);
    item.addEventListener("click", () => loadAbout(about));
    list.append(item);
  }
}

function renderRail() {
  const lanes = $("lane-list");
  lanes.textContent = "";
  for (const lane of model.lanes) {
    const hidden = view.hiddenLanes.has(lane.name);
    const item = el("li", hidden ? "dimmed" : "");
    const dot = el("span", "legend-dot");
    dot.style.background = kindColor("memory_dimension");
    item.append(dot, el("span", "", `${lane.name} `), el("span", "muted", String(lane.count)));
    item.title = hidden ? "hidden — click to show this lane" : "click to hide this lane";
    item.addEventListener("click", () => {
      if (hidden) view.hiddenLanes.delete(lane.name);
      else view.hiddenLanes.add(lane.name);
      renderRail();
      requestDraw();
    });
    lanes.append(item);
  }

  const kinds = new Map();
  for (const m of model.entries) kinds.set(m.kind, (kinds.get(m.kind) || 0) + 1);
  const kindList = $("kind-legend");
  kindList.textContent = "";
  for (const [kind, count] of [...kinds.entries()].sort((a, b) => b[1] - a[1])) {
    const item = el("li", view.dimmedKinds.has(kind) ? "dimmed" : "");
    const dot = el("span", "legend-dot");
    dot.style.background = kindColor(kind);
    item.append(dot, el("span", "", `${kind} `), el("span", "muted", String(count)));
    item.title = "click to dim/undim this kind";
    item.addEventListener("click", () => {
      if (view.dimmedKinds.has(kind)) view.dimmedKinds.delete(kind);
      else view.dimmedKinds.add(kind);
      renderRail();
      requestDraw();
    });
    kindList.append(item);
  }

  const classes = new Map();
  for (const edge of model.edges) classes.set(edge.class, (classes.get(edge.class) || 0) + 1);
  const classList = $("class-legend");
  classList.textContent = "";
  const DASH_CLASS = { evidential: "dashed", motivational: "dotted", constraint: "dashed", procedural: "dashed" };
  for (const [cls, count] of [...classes.entries()].sort((a, b) => b[1] - a[1])) {
    const item = el("li", "");
    const dash = el("span", `legend-dash ${DASH_CLASS[cls] || ""}`);
    dash.style.borderTopColor = classColor(cls);
    item.append(dash, el("span", "", `${cls} `), el("span", "muted", String(count)));
    classList.append(item);
  }
}

$("search").addEventListener("input", () => {
  const raw = $("search").value;
  const results = $("search-results");
  results.textContent = "";
  view.searchHits = new Set();
  const query = KMP_LOOM.parseQuery(raw);
  if (!query.empty && raw.trim().length >= 2) {
    for (const m of model.entries) {
      const fields = {
        text: m.text.toLowerCase(),
        id: m.ref.toLowerCase(),
        kind: m.kind.toLowerCase(),
        dim: m.coords.map((c) => c.dimension + " " + c.scope).join(" ").toLowerCase(),
      };
      if (KMP_LOOM.matchesQuery(query, fields)) view.searchHits.add(m.ref);
    }
    if (view.searchHits.size > 50) {
      results.append(el("li", "muted", `${view.searchHits.size} hits · showing 50`));
    }
    for (const ref of [...view.searchHits].slice(0, 50)) {
      const m = model.byRef.get(ref);
      const item = el("li", "", m.text.length > 60 ? m.text.slice(0, 59) + "…" : m.text);
      item.append(el("span", "sub mono", ref));
      item.addEventListener("click", () => {
        selectEntry(ref);
        centerOn(ref);
      });
      results.append(item);
    }
  }
  requestDraw();
  reportView();
});
$("search").addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    $("search").value = "";
    view.searchHits = new Set();
    $("search-results").textContent = "";
    requestDraw();
    $("search").blur();
  }
});

/* ---------------- status ---------------- */

function renderStats() {
  $("s-entries").textContent = String(model.entries.length);
  $("s-lanes").textContent = String(model.lanes.length);
  $("s-relations").textContent = String(
    model.edges.length + model.supersessions.length + model.contradictions.length
  );
  const clocked = model.entries.filter((m) => KMP_LOOM.strictMs(m, view.clock) !== null).length;
  $("s-clocked").textContent = `${clocked}/${model.entries.length}`;
  if (view.full) $("s-window").textContent = `${fmtMs(view.t0)} → ${fmtMs(view.t1)}`;
}


/* ---------------- the agent's hand on the loom ----------------
   The view is a shared aggregate with its own revision. An agent moves it by
   declaring intent through kmp_view_apply_intent; the browser follows by long
   poll, and reports back where the human is looking so the agent can see it
   and rebase rather than yanking the loom away mid-gesture. Every agent move
   arrives named, explained and undoable. */

const VIEW_ID = "default";
const sync = { revision: 0, applying: false, reportTimer: null, lastReport: "", polling: false };

async function viewOpen() {
  try {
    const response = await fetch(
      `/api/view/open?id=${VIEW_ID}&about=${encodeURIComponent(model.about || "")}`,
      { method: "POST" }
    );
    const state = await response.json();
    sync.revision = state.view_revision || 0;
    renderProvenance(state);
    if (!sync.polling) {
      sync.polling = true;
      pollView();
    }
  } catch (error) {
    // A viewer that cannot reach its own view still draws the memory.
    showError(`view sync unavailable: ${error.message}`);
  }
}

async function pollView() {
  for (;;) {
    try {
      const state = await api("/api/view", { id: VIEW_ID, since: sync.revision });
      // Any revision that is not ours is news — including a *lower* one,
      // which means the view server restarted and began counting again.
      // Waiting only for a higher number left the browser deaf for good.
      if (state.view_revision !== sync.revision) {
        const restarted = state.view_revision < sync.revision;
        sync.revision = state.view_revision;
        const actor = state.last_change && state.last_change.actor;
        if (restarted || (actor && actor !== "human")) await applyAgentState(state);
        renderProvenance(state);
      }
    } catch (error) {
      await new Promise((resolve) => setTimeout(resolve, 2000));
    }
  }
}

/* An intent is meaning, not geometry: this is where meaning becomes a
   window, a clock, a set of lanes. */
async function applyAgentState(state) {
  sync.applying = true;
  try {
    if (state.about && state.about !== model.about) {
      await loadAbout(state.about);
    }
    if (state.clock && state.clock !== view.clock) setClock(state.clock, false);

    const projection = state.projection || {};
    if (projection.dimensions) {
      const keep = new Set(projection.dimensions);
      view.hiddenLanes = new Set(
        model.lanes.map((lane) => lane.name).filter((name) => !keep.has(name))
      );
      renderRail();
    }

    const range = state.focus && state.focus.time_range;
    const refs = (state.focus && state.focus.refs) || [];
    let framed = false;
    if (range && range.from && range.to) {
      const from = Date.parse(range.from);
      const to = Date.parse(range.to);
      if (Number.isFinite(from) && Number.isFinite(to)) {
        setWindow(from, to);
        framed = true;
      }
    } else if (refs.length) {
      framed = frameRefs(refs);
    }
    // A rung is a density to fall back on, not an override: an intent that
    // named its own window asked for that window.
    if (projection.semantic_zoom && !framed) applyZoomRung(projection.semantic_zoom);

    if (state.search !== undefined) {
      $("search").value = state.search || "";
      $("search").dispatchEvent(new Event("input"));
    }
    if (state.trace) {
      tracePick = { from: state.trace.from, to: state.trace.to };
      await runTrace();
    }
    if (state.selection) {
      await selectEntry(state.selection);
      centerOn(state.selection);
    }
  } finally {
    sync.applying = false;
  }
}

/* "Frame these refs" — the canonical intent. The window becomes the span
   they occupy on the current clock, with room to breathe. */
function frameRefs(refs) {
  const stamps = refs
    .map((ref) => model.byRef.get(ref))
    .filter(Boolean)
    .map((entry) => KMP_LOOM.placedMs(entry, view.clock))
    .filter((t) => t !== null);
  if (!stamps.length) return false;
  const lo = Math.min(...stamps);
  const hi = Math.max(...stamps);
  const pad = Math.max(60000, (hi - lo) * 0.4);
  setWindow(lo - pad, hi + pad);
  return true;
}

/* A rung of the ladder is a density of time per pixel; asking for "moment"
   asks for a window fine enough to show entries. */
function applyZoomRung(rung) {
  const width = Math.max(1, canvas.clientWidth);
  const target = { atlas: 1.2e6, episode: 120e3, moment: 8e3, evidence: 2e3 }[rung];
  if (!target) return;
  const span = Math.max(1000, target * width);
  const centre = (view.t0 + view.t1) / 2;
  setWindow(centre - span / 2, centre + span / 2);
}

function renderProvenance(state) {
  const chip = $("agent-chip");
  const change = state.last_change;
  if (!change || change.actor === "human") {
    chip.hidden = true;
    return;
  }
  const why = change.explanation ? ` · ${change.explanation}` : "";
  $("agent-chip-text").textContent = `${change.actor} moved the loom${why}`;
  chip.hidden = false;
}

$("agent-undo").addEventListener("click", async () => {
  try {
    const response = await fetch(`/api/view/undo?id=${VIEW_ID}`, { method: "POST" });
    if (!response.ok) return;
    const state = await response.json();
    sync.revision = state.view_revision;
    await applyAgentState(state);
    $("agent-chip").hidden = true;
  } catch (error) {
    showError(error.message);
  }
});

/* Where the human is looking, reported so the agent's read of the view is
   the truth rather than whatever it last asked for. Debounced, and silent
   while the loom is busy obeying an intent — otherwise the two would echo. */
function reportView() {
  if (sync.applying || !model.about) return;
  clearTimeout(sync.reportTimer);
  sync.reportTimer = setTimeout(async () => {
    const params = new URLSearchParams({
      id: VIEW_ID,
      about: model.about,
      clock: view.clock,
      from: new Date(Math.round(view.t0)).toISOString(),
      to: new Date(Math.round(view.t1)).toISOString(),
    });
    if (view.selectedRef) params.set("selection", view.selectedRef);
    if ($("search").value.trim()) params.set("search", $("search").value.trim());
    if (tracePick.from && tracePick.to) {
      params.set("trace_from", tracePick.from);
      params.set("trace_to", tracePick.to);
    }
    const signature = params.toString();
    if (signature === sync.lastReport) return;
    sync.lastReport = signature;
    try {
      const response = await fetch(`/api/view/report?${signature}`, { method: "POST" });
      const state = await response.json();
      if (state.view_revision) sync.revision = state.view_revision;
      $("agent-chip").hidden = true;
    } catch (error) {
      // The loom keeps working even when nobody is listening to it.
    }
  }, 400);
}

/* ---------------- init ---------------- */

async function init() {
  applyTheme();
  try {
    await setupRenderer();
  } catch (error) {
    showError(`renderer failed to start: ${error.message}`);
    return;
  }
  try {
    await api("/api/info");
    const aboutsView = await api("/api/abouts");
    abouts = aboutsView.abouts;
    renderAbouts();
    if (abouts.length) await loadAbout(abouts[0]);
    else showError("the kernel holds no abouts yet — ingest some memory first");
  } catch (error) {
    showError(error.message);
  }
}

init();
