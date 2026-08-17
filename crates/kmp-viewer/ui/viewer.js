/* KMP Memory Viewer — vanilla JS + a vendored, hash-pinned pixi.js bundle
   (see ui/vendor/VENDOR.md; no npm, no CDN, nothing fetched at runtime).
   Talks only to the sibling /api/* routes; everything it shows is what the
   kernel's own read model returned, never an invention of the client. */
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
  if (!response.ok) {
    throw new Error(body.error || `${path} failed with ${response.status}`);
  }
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

const shortHash = (hash) => (hash && hash.length > 12 ? hash.slice(0, 12) + "…" : hash || "–");
const REPLAY_WINDOW = 256;
const nowIso = () => new Date().toISOString().replace(/\.\d+Z$/, "Z");

/* ---------------- theme ---------------- */

const THEMES = ["auto", "light", "dark"];
let themeIndex = 0;

function applyTheme() {
  const theme = THEMES[themeIndex];
  if (theme === "auto") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  $("btn-theme").textContent = theme[0].toUpperCase() + theme.slice(1);
  invalidatePalette();
  requestDraw();
}

$("btn-theme").addEventListener("click", () => {
  themeIndex = (themeIndex + 1) % THEMES.length;
  applyTheme();
});
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  invalidatePalette();
  requestDraw();
});

/* ---------------- palette ---------------- */

let paletteCache = null;

function palette() {
  if (!paletteCache) {
    const style = getComputedStyle(document.documentElement);
    const read = (name) => style.getPropertyValue(name).trim();
    paletteCache = {
      surface: read("--surface-1"),
      text: read("--text-primary"),
      textMuted: read("--text-muted"),
      accent: read("--accent"),
      edge: read("--edge"),
      edgeDim: read("--edge-dim"),
      halo: read("--halo"),
      trace: read("--series-2"),
      overflow: read("--series-overflow"),
      series: Array.from({ length: 8 }, (_, i) => read(`--series-${i + 1}`)),
    };
  }
  return paletteCache;
}

function invalidatePalette() {
  paletteCache = null;
  resetLabelPool();
}

/* Fixed slots for the kinds KMP itself talks about; unknown kinds take the
   remaining slots in first-seen order and never repaint (color follows the
   entity). Past eight kinds, overflow wears neutral — the legend and the
   node's own label still carry identity. */
const KIND_SLOTS = {
  about: 0,
  decision: 1,
  evidence: 2,
  question: 3,
  artifact: 4,
  session: 5,
  dimension: 6,
  incident: 7,
};
const kindAssignments = new Map();

function kindColor(kind) {
  const p = palette();
  if (kind in KIND_SLOTS) return p.series[KIND_SLOTS[kind]];
  if (!kindAssignments.has(kind)) {
    const taken = new Set([
      ...Object.values(KIND_SLOTS).filter((slot) =>
        [...graph.nodes.values()].some((n) => KIND_SLOTS[n.kind] === slot)
      ),
      ...kindAssignments.values(),
    ]);
    let slot = 0;
    while (taken.has(slot) && slot < 8) slot += 1;
    kindAssignments.set(kind, slot);
  }
  const slot = kindAssignments.get(kind);
  return slot < 8 ? p.series[slot] : p.overflow;
}

/* ---------------- state ---------------- */

const graph = {
  about: null,
  rootId: null,
  nodes: new Map(), // id -> {id, kind, title, summary, status, labels, properties, x, y, vx, vy, degree}
  edges: [], // {source, target, rel, class, why, evidence, ...}
  edgeKeys: new Set(),
  details: new Map(), // id -> detail text
  rendered: null,
};

let selectedId = null;
let hover = null; // {type: "node"|"edge", ref}
let dimmedKinds = new Set();
let traceHighlight = null; // {nodes: Set, edges: Set}
let searchHits = new Set();

/* ---------------- graph data ---------------- */

const edgeKey = (e) => `${e.source} ${e.rel} ${e.target}`;

function upsertNode(view) {
  const existing = graph.nodes.get(view.id);
  if (existing) {
    if (view.kind !== "?") Object.assign(existing, view);
    return existing;
  }
  const node = {
    ...view,
    x: 0,
    y: 0,
    vx: 0,
    vy: 0,
    degree: 0,
  };
  graph.nodes.set(view.id, node);
  return node;
}

function placeNear(node, anchor) {
  const angle = randomUnit() * 2 * Math.PI;
  const radius = 60 + randomUnit() * 60;
  node.x = (anchor ? anchor.x : 0) + Math.cos(angle) * radius;
  node.y = (anchor ? anchor.y : 0) + Math.sin(angle) * radius;
}

function randomUnit() {
  const sample = new Uint32Array(1);
  crypto.getRandomValues(sample);
  return sample[0] / 0x100000000;
}

function addEdges(edges) {
  let added = 0;
  for (const edge of edges) {
    const key = edgeKey(edge);
    if (graph.edgeKeys.has(key)) continue;
    graph.edgeKeys.add(key);
    graph.edges.push(edge);
    added += 1;
  }
  if (added) recomputeDegrees();
  return added;
}

function recomputeDegrees() {
  for (const node of graph.nodes.values()) node.degree = 0;
  for (const edge of graph.edges) {
    const s = graph.nodes.get(edge.source);
    const t = graph.nodes.get(edge.target);
    if (s) s.degree += 1;
    if (t) t.degree += 1;
  }
}

function resetGraph(view) {
  const previous = graph.nodes;
  graph.about = view.about;
  graph.rootId = view.root_id;
  graph.nodes = new Map();
  graph.edges = [];
  graph.edgeKeys = new Set();
  graph.details = new Map(view.details.map((d) => [d.id, d.detail]));
  graph.rendered = view.rendered;
  for (const nodeView of view.nodes) {
    const node = upsertNode(nodeView);
    const old = previous.get(node.id);
    if (old) {
      node.x = old.x;
      node.y = old.y;
    } else {
      placeNear(node, previous.get(view.root_id));
    }
  }
  addEdges(view.edges);
  traceHighlight = null;
  searchHits = new Set();
  resetLabelPool();
}

/* Merge one inspected node's surroundings into the drawn graph, labelling
   fresh neighbors through the batch endpoint. */
async function expandNode(id) {
  const inspect = await api("/api/node", { id });
  const anchorView = upsertNode(inspect.node);
  if (!anchorView.degree && !anchorView.x && !anchorView.y) placeNear(anchorView, centerNode());
  if (inspect.detail) graph.details.set(id, inspect.detail.detail);

  addEdges([...inspect.incoming, ...inspect.outgoing]);
  const unknown = new Set();
  for (const edge of graph.edges) {
    if (!graph.nodes.has(edge.source)) unknown.add(edge.source);
    if (!graph.nodes.has(edge.target)) unknown.add(edge.target);
  }
  for (const missingId of unknown) {
    const placeholder = upsertNode({
      id: missingId,
      kind: "?",
      title: missingId,
      summary: "",
      status: "",
      labels: [],
      properties: {},
    });
    placeNear(placeholder, graph.nodes.get(id));
  }
  if (unknown.size) {
    const ids = [...unknown];
    for (let i = 0; i < ids.length; i += 64) {
      const batch = await api("/api/nodes", { ids: ids.slice(i, i + 64).join(",") });
      for (const nodeView of batch.nodes) upsertNode(nodeView);
    }
  }
  recomputeDegrees();
  reheat();
  return inspect;
}

const centerNode = () => graph.nodes.get(graph.rootId) || graph.nodes.values().next().value;

/* ---------------- force simulation ---------------- */

let alpha = 0;
const SIM = { repulsion: 2600, spring: 0.055, restLength: 120, gravity: 0.012, damping: 0.6 };

function reheat() {
  alpha = 1;
  requestDraw();
}

function simTick() {
  const nodes = [...graph.nodes.values()];
  for (let i = 0; i < nodes.length; i += 1) {
    const a = nodes[i];
    for (let j = i + 1; j < nodes.length; j += 1) {
      const b = nodes[j];
      let dx = a.x - b.x;
      let dy = a.y - b.y;
      let d2 = dx * dx + dy * dy;
      if (d2 < 1) {
        dx = randomUnit() - 0.5;
        dy = randomUnit() - 0.5;
        d2 = 1;
      }
      const force = (SIM.repulsion * alpha) / d2;
      const dist = Math.sqrt(d2);
      const fx = (dx / dist) * force;
      const fy = (dy / dist) * force;
      a.vx += fx;
      a.vy += fy;
      b.vx -= fx;
      b.vy -= fy;
    }
  }
  for (const edge of graph.edges) {
    const s = graph.nodes.get(edge.source);
    const t = graph.nodes.get(edge.target);
    if (!s || !t || s === t) continue;
    const dx = t.x - s.x;
    const dy = t.y - s.y;
    const dist = Math.max(1, Math.hypot(dx, dy));
    const force = SIM.spring * alpha * (dist - SIM.restLength);
    const fx = (dx / dist) * force;
    const fy = (dy / dist) * force;
    s.vx += fx;
    s.vy += fy;
    t.vx -= fx;
    t.vy -= fy;
  }
  for (const node of nodes) {
    node.vx -= node.x * SIM.gravity * alpha;
    node.vy -= node.y * SIM.gravity * alpha;
    if (node !== dragNode) {
      node.x += node.vx;
      node.y += node.vy;
    }
    node.vx *= SIM.damping;
    node.vy *= SIM.damping;
  }
  alpha *= 0.97;
  if (alpha < 0.01) alpha = 0;
}

/* ---------------- renderer (pixi.js, WebGL) ---------------- */

const canvas = $("graph-canvas");
const camera = { x: 0, y: 0, k: 1 };
let dirty = true;

let app = null; // PIXI.Application
let root = null; // camera-transformed container
let edgesGfx = null; // PIXI.Graphics — all edges, rebuilt when dirty
let nodesGfx = null; // PIXI.Graphics — all node marks, rebuilt when dirty
let labelLayer = null; // PIXI.Container — pooled PIXI.Text labels
const labelPool = new Map(); // node id -> PIXI.Text

function requestDraw() {
  dirty = true;
}

function resetLabelPool() {
  if (!labelLayer) return;
  for (const label of labelPool.values()) label.destroy();
  labelPool.clear();
  labelLayer.removeChildren();
  dirty = true;
}

async function setupRenderer() {
  if (typeof PIXI === "undefined") {
    throw new Error("the vendored pixi.js bundle did not load");
  }
  app = new PIXI.Application();
  await app.init({
    canvas,
    resizeTo: $("stage"),
    backgroundAlpha: 0,
    antialias: true,
    autoDensity: true,
    resolution: devicePixelRatio || 1,
  });
  root = new PIXI.Container();
  edgesGfx = new PIXI.Graphics();
  nodesGfx = new PIXI.Graphics();
  labelLayer = new PIXI.Container();
  root.addChild(edgesGfx, nodesGfx, labelLayer);
  app.stage.addChild(root);
  app.renderer.on("resize", () => (dirty = true));
  app.ticker.add(() => {
    if (alpha > 0) {
      simTick();
      dirty = true;
    }
    if (playback.active) {
      if (playback.camTarget) {
        camera.x += (playback.camTarget.x - camera.x) * 0.06;
        camera.y += (playback.camTarget.y - camera.y) * 0.06;
      }
      dirty = true;
    }
    if (dirty) {
      dirty = false;
      syncScene();
    }
  });
}

const nodeRadius = (node) =>
  7 + Math.min(9, Math.sqrt(node.degree) * 2) + (node.id === graph.rootId ? 3 : 0);

const toScreen = (x, y) => ({
  x: (x - camera.x) * camera.k + canvas.clientWidth / 2,
  y: (y - camera.y) * camera.k + canvas.clientHeight / 2,
});
const toWorld = (sx, sy) => ({
  x: (sx - canvas.clientWidth / 2) / camera.k + camera.x,
  y: (sy - canvas.clientHeight / 2) / camera.k + camera.y,
});

function nodeAlpha(node) {
  if (playback.active) {
    if (node.id === playback.currentRef) return 1;
    return playback.revealed[playback.step].has(node.id) ? 0.8 : 0.07;
  }
  if (traceHighlight) return traceHighlight.nodes.has(node.id) ? 1 : 0.14;
  if (dimmedKinds.has(node.kind)) return 0.14;
  if (searchHits.size) return searchHits.has(node.id) ? 1 : 0.25;
  return 1;
}

function edgeAlpha(edge) {
  if (playback.active) {
    const revealed = playback.revealed[playback.step];
    if (!revealed.has(edge.source) || !revealed.has(edge.target)) return 0.04;
    return edge.source === playback.currentRef || edge.target === playback.currentRef ? 1 : 0.55;
  }
  if (traceHighlight) return traceHighlight.edges.has(edgeKey(edge)) ? 1 : 0.08;
  const s = graph.nodes.get(edge.source);
  const t = graph.nodes.get(edge.target);
  return Math.min(s ? nodeAlpha(s) : 1, t ? nodeAlpha(t) : 1);
}

/* Rebuilds the scene in world coordinates; the root container carries the
   camera, and screen-constant sizes are divided by the zoom factor. */
function syncScene() {
  if (!app) return;
  const p = palette();
  const k = camera.k;
  root.scale.set(k);
  root.position.set(
    canvas.clientWidth / 2 - camera.x * k,
    canvas.clientHeight / 2 - camera.y * k
  );

  const hoveredEdge = hover && hover.type === "edge" ? hover.ref : null;
  const hoveredNode = hover && hover.type === "node" ? hover.ref : null;

  edgesGfx.clear();
  for (const edge of graph.edges) {
    const s = graph.nodes.get(edge.source);
    const t = graph.nodes.get(edge.target);
    if (!s || !t) continue;
    const a = edgeAlpha(edge);
    if (a <= 0.02) continue;
    const highlighted =
      edge === hoveredEdge ||
      (traceHighlight && traceHighlight.edges.has(edgeKey(edge))) ||
      (playback.active &&
        (edge.source === playback.currentRef || edge.target === playback.currentRef));
    const color = highlighted ? p.trace : p.edge;
    const width = (highlighted ? 2.5 : 1.2) / k;
    if (s === t) {
      const r = nodeRadius(s);
      edgesGfx
        .circle(s.x + r, s.y - r, r * 0.8)
        .stroke({ width, color, alpha: a });
      continue;
    }
    edgesGfx.moveTo(s.x, s.y).lineTo(t.x, t.y).stroke({ width, color, alpha: a });
    drawArrowhead(edge, s, t, color, a);
  }

  nodesGfx.clear();
  const wantedLabels = new Set();
  for (const node of graph.nodes.values()) {
    const a = nodeAlpha(node);
    let r = Math.max(nodeRadius(node), 3 / k);
    const isCurrentStep = playback.active && node.id === playback.currentRef;
    if (isCurrentStep) {
      r += (1.8 + 1.6 * Math.sin(performance.now() / 160)) / k;
    }
    nodesGfx
      .circle(node.x, node.y, r)
      .fill({ color: node.kind === "?" ? p.overflow : kindColor(node.kind), alpha: a })
      .stroke({ width: 2 / k, color: p.surface, alpha: a });
    if (node.id === graph.rootId || node.id === selectedId || isCurrentStep) {
      nodesGfx.circle(node.x, node.y, r + 3 / k).stroke({
        width: (node.id === selectedId || isCurrentStep ? 2.5 : 1.5) / k,
        color: node.id === selectedId || isCurrentStep ? p.accent : p.textMuted,
        alpha: a,
      });
    }
    const showLabel =
      k > 0.55 ||
      node === hoveredNode ||
      node.id === selectedId ||
      node.id === graph.rootId ||
      isCurrentStep ||
      searchHits.has(node.id);
    if (showLabel && a > 0.2) {
      wantedLabels.add(node.id);
      syncLabel(node, r, a, p, k);
    }
  }
  for (const [id, label] of labelPool) {
    if (!wantedLabels.has(id)) label.visible = false;
  }
}

function syncLabel(node, radius, alphaValue, p, k) {
  let label = labelPool.get(node.id);
  const text = node.title.length > 34 ? node.title.slice(0, 33) + "…" : node.title;
  if (!label) {
    label = new PIXI.Text({
      text,
      style: {
        fontFamily: "system-ui, sans-serif",
        fontSize: 12,
        fill: p.text,
        stroke: { color: p.halo, width: 3, join: "round" },
      },
      resolution: (devicePixelRatio || 1) * 2,
    });
    label.anchor.set(0, 0.5);
    labelPool.set(node.id, label);
    labelLayer.addChild(label);
  } else if (label.text !== text) {
    label.text = text;
  }
  label.visible = true;
  label.alpha = alphaValue;
  label.scale.set(1 / k);
  label.position.set(node.x + radius + 5 / k, node.y);
}

function drawArrowhead(edge, s, t, color, alphaValue) {
  const angle = Math.atan2(t.y - s.y, t.x - s.x);
  const back = nodeRadius(t) + 2 / camera.k;
  const tipX = t.x - Math.cos(angle) * back;
  const tipY = t.y - Math.sin(angle) * back;
  const size = 6 / camera.k;
  edgesGfx
    .poly([
      tipX,
      tipY,
      tipX - size * Math.cos(angle - 0.45),
      tipY - size * Math.sin(angle - 0.45),
      tipX - size * Math.cos(angle + 0.45),
      tipY - size * Math.sin(angle + 0.45),
    ])
    .fill({ color, alpha: alphaValue });
}

/* ---------------- temporal replay ----------------
   Replays the about's timeline over the graph: entries reveal in sequence
   order, the current one pulses with the camera easing toward it, and its
   text plays as a caption. The order comes from the kernel's own temporal
   read, not from a guess over the drawn edges. */

const playback = {
  active: false,
  entries: [],
  revealed: [], // step -> Set of revealed ref_ids (cumulative)
  step: 0,
  currentRef: null,
  camTarget: null,
  timer: null,
};

function entryOrderKey(entry) {
  let sequence = Number.MAX_SAFE_INTEGER;
  let time = "";
  for (const c of entry.coordinates) {
    if (c.sequence !== undefined && c.sequence < sequence) sequence = c.sequence;
    if (c.occurred_at && (!time || c.occurred_at < time)) time = c.occurred_at;
  }
  return { sequence, time };
}

/// Nodes that belong to `id` rather than standing beside it in time.
///
/// Structural scaffolding: the dimension an entry sits in, and the evidence
/// that supports it. Both are reached by an edge whose other end is the
/// entry, and neither carries a coordinate of its own.
const ATTACHING_RELATIONS = new Set([
  "has_dimension",
  "has_evidence",
  "contains_entry",
  "supports",
  "records",
]);

function attachedTo(id) {
  const attached = [];
  for (const edge of graph.edges) {
    if (!ATTACHING_RELATIONS.has(edge.rel)) continue;
    if (edge.source === id) attached.push(edge.target);
    else if (edge.target === id) attached.push(edge.source);
  }
  return attached;
}

async function startReplay() {
  if (!graph.about) return;
  try {
    // `near` around now, wide in both directions: it is the only cursor that
    // needs nothing selected and still returns the whole line. `goto` with a
    // bare sequence resolves no dimension and no scope, so it always came
    // back empty and this button reported an empty memory that was not.
    const view = await api("/api/timeline", {
      about: graph.about,
      direction: "near",
      time: nowIso(),
      before: REPLAY_WINDOW,
      after: REPLAY_WINDOW,
    });
    const entries = [...view.entries].sort((a, b) => {
      const ka = entryOrderKey(a);
      const kb = entryOrderKey(b);
      if (ka.sequence !== kb.sequence) return ka.sequence - kb.sequence;
      return ka.time < kb.time ? -1 : ka.time > kb.time ? 1 : 0;
    });
    if (!entries.length) {
      showError(
        `nothing to replay: ${graph.about} has no entries carrying a temporal coordinate`
      );
      return;
    }
    playback.entries = entries;
    playback.revealed = [];
    const accumulated = new Set();
    for (const entry of entries) {
      accumulated.add(entry.ref_id);
      // What hangs off an entry has no time of its own — a dimension is the
      // scope the entry was written into, and evidence exists at the moment
      // the entry it supports does. They can never appear in a timeline
      // query, so a replay that only revealed entries finished with two
      // thirds of the graph still dark and looked unfinished.
      for (const attached of attachedTo(entry.ref_id)) accumulated.add(attached);
      playback.revealed.push(new Set(accumulated));
    }
    playback.active = true;
    $("playbar").hidden = false;
    $("pb-scrub").max = String(entries.length - 1);
    $("btn-replay").textContent = "■ Stop";
    if (camera.k < 0.8) camera.k = 0.95;
    setReplayStep(0);
    setReplayPlaying(true);
  } catch (error) {
    showError(error.message);
  }
}

function setReplayStep(step) {
  playback.step = Math.max(0, Math.min(step, playback.entries.length - 1));
  const entry = playback.entries[playback.step];
  playback.currentRef = entry.ref_id;
  $("pb-scrub").value = String(playback.step);
  const timed = entry.coordinates.find((c) => c.occurred_at);
  $("pb-label").textContent =
    `${playback.step + 1}/${playback.entries.length}` +
    (timed ? ` · ${timed.occurred_at.slice(11, 16)}` : "");
  const caption = $("playcaption");
  caption.hidden = false;
  caption.textContent = entry.text.length > 200 ? entry.text.slice(0, 199) + "…" : entry.text;
  const node = graph.nodes.get(entry.ref_id);
  if (node) playback.camTarget = node;
  requestDraw();
}

function setReplayPlaying(playing) {
  clearInterval(playback.timer);
  playback.timer = null;
  if (playing && playback.active) {
    playback.timer = setInterval(() => {
      if (playback.step >= playback.entries.length - 1) setReplayPlaying(false);
      else setReplayStep(playback.step + 1);
    }, 1800);
  }
  $("pb-toggle").textContent = playback.timer ? "⏸" : "▶";
}

function stopReplay() {
  if (!playback.active) return;
  setReplayPlaying(false);
  playback.active = false;
  playback.currentRef = null;
  playback.camTarget = null;
  $("playbar").hidden = true;
  $("playcaption").hidden = true;
  $("btn-replay").textContent = "▶ Replay";
  requestDraw();
}

$("btn-replay").addEventListener("click", () => {
  if (playback.active) stopReplay();
  else startReplay();
});
$("pb-toggle").addEventListener("click", () => setReplayPlaying(!playback.timer));
$("pb-close").addEventListener("click", stopReplay);
$("pb-prev").addEventListener("click", () => {
  setReplayPlaying(false);
  setReplayStep(playback.step - 1);
});
$("pb-next").addEventListener("click", () => {
  setReplayPlaying(false);
  setReplayStep(playback.step + 1);
});
$("pb-scrub").addEventListener("input", (event) => {
  setReplayPlaying(false);
  setReplayStep(parseInt(event.target.value, 10) || 0);
});

/* ---------------- hit testing ---------------- */

function nodeAt(sx, sy) {
  const w = toWorld(sx, sy);
  let best = null;
  let bestDist = Infinity;
  for (const node of graph.nodes.values()) {
    const dist = Math.hypot(node.x - w.x, node.y - w.y);
    const hitRadius = nodeRadius(node) + 5 / camera.k;
    if (dist < hitRadius && dist < bestDist) {
      best = node;
      bestDist = dist;
    }
  }
  return best;
}

function edgeAt(sx, sy) {
  const w = toWorld(sx, sy);
  const threshold = 6 / camera.k;
  let best = null;
  let bestDist = Infinity;
  for (const edge of graph.edges) {
    const s = graph.nodes.get(edge.source);
    const t = graph.nodes.get(edge.target);
    if (!s || !t || s === t) continue;
    const dist = pointToSegment(w.x, w.y, s.x, s.y, t.x, t.y);
    if (dist < threshold && dist < bestDist) {
      best = edge;
      bestDist = dist;
    }
  }
  return best;
}

function pointToSegment(px, py, x1, y1, x2, y2) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const lengthSq = dx * dx + dy * dy;
  const t = lengthSq ? Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / lengthSq)) : 0;
  return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
}

/* ---------------- pointer interaction ---------------- */

let dragNode = null;
let panStart = null;
let moved = false;

canvas.addEventListener("pointerdown", (event) => {
  canvas.setPointerCapture(event.pointerId);
  canvas.classList.add("dragging");
  moved = false;
  const node = nodeAt(event.offsetX, event.offsetY);
  if (node) {
    dragNode = node;
  } else {
    panStart = { x: event.offsetX, y: event.offsetY, camX: camera.x, camY: camera.y };
  }
});

canvas.addEventListener("pointermove", (event) => {
  if (dragNode) {
    moved = true;
    const w = toWorld(event.offsetX, event.offsetY);
    dragNode.x = w.x;
    dragNode.y = w.y;
    dragNode.vx = 0;
    dragNode.vy = 0;
    if (alpha < 0.1) alpha = 0.1;
    requestDraw();
    return;
  }
  if (panStart) {
    moved = true;
    camera.x = panStart.camX - (event.offsetX - panStart.x) / camera.k;
    camera.y = panStart.camY - (event.offsetY - panStart.y) / camera.k;
    requestDraw();
    return;
  }
  const node = nodeAt(event.offsetX, event.offsetY);
  const edge = node ? null : edgeAt(event.offsetX, event.offsetY);
  const next = node ? { type: "node", ref: node } : edge ? { type: "edge", ref: edge } : null;
  if ((next && next.ref) !== (hover && hover.ref)) {
    hover = next;
    updateTooltip(event.offsetX, event.offsetY);
    requestDraw();
  } else if (hover) {
    positionTooltip(event.offsetX, event.offsetY);
  }
});

canvas.addEventListener("pointerup", (event) => {
  canvas.classList.remove("dragging");
  const wasDrag = moved;
  const draggedNode = dragNode;
  dragNode = null;
  panStart = null;
  if (!wasDrag) {
    if (draggedNode) selectNode(draggedNode.id);
    else {
      selectedId = null;
      renderDetailEmpty();
      requestDraw();
    }
  }
});

canvas.addEventListener("dblclick", (event) => {
  const node = nodeAt(event.offsetX, event.offsetY);
  if (node) {
    expandNode(node.id).catch((error) => showError(error.message));
  }
});

canvas.addEventListener("pointerleave", () => {
  hover = null;
  $("tooltip").hidden = true;
  requestDraw();
});

canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    const factor = Math.exp(-event.deltaY * 0.0012);
    const before = toWorld(event.offsetX, event.offsetY);
    camera.k = Math.min(8, Math.max(0.08, camera.k * factor));
    const after = toWorld(event.offsetX, event.offsetY);
    camera.x += before.x - after.x;
    camera.y += before.y - after.y;
    requestDraw();
  },
  { passive: false }
);

function centerOn(id) {
  const node = graph.nodes.get(id);
  if (!node) return;
  camera.x = node.x;
  camera.y = node.y;
  if (camera.k < 0.7) camera.k = 0.9;
  requestDraw();
}

/* ---------------- tooltip ---------------- */

function updateTooltip(sx, sy) {
  const tooltip = $("tooltip");
  tooltip.textContent = "";
  if (!hover) {
    tooltip.hidden = true;
    return;
  }
  if (hover.type === "node") {
    const node = hover.ref;
    tooltip.append(el("div", "tt-title", node.title));
    tooltip.append(el("div", "tt-sub", `${node.kind}${node.status ? " · " + node.status : ""}`));
    if (node.summary) tooltip.append(el("div", "tt-quote", node.summary));
  } else {
    const edge = hover.ref;
    tooltip.append(el("div", "tt-title", `${edge.rel} (${edge.class})`));
    tooltip.append(el("div", "tt-sub", `${edge.source} → ${edge.target}`));
    if (edge.why) tooltip.append(el("div", "tt-quote", edge.why));
    if (edge.motivation) tooltip.append(el("div", "tt-quote", `motivación: ${edge.motivation}`));
    if (edge.method) tooltip.append(el("div", "tt-sub", `método: ${edge.method}`));
    if (edge.evidence) tooltip.append(el("div", "tt-sub", `evidencia: ${edge.evidence}`));
    if (edge.confidence) tooltip.append(el("div", "tt-sub", `confianza: ${edge.confidence}`));
  }
  tooltip.hidden = false;
  positionTooltip(sx, sy);
}

function positionTooltip(sx, sy) {
  const tooltip = $("tooltip");
  if (tooltip.hidden) return;
  const stage = $("stage").getBoundingClientRect();
  const x = Math.min(sx + 14, stage.width - tooltip.offsetWidth - 8);
  const y = Math.min(sy + 14, stage.height - tooltip.offsetHeight - 8);
  tooltip.style.left = `${Math.max(4, x)}px`;
  tooltip.style.top = `${Math.max(4, y)}px`;
}

/* ---------------- sidebar: abouts, legend, search ---------------- */

let abouts = [];

function renderAbouts() {
  const list = $("about-list");
  list.textContent = "";
  const filter = $("search").value.trim().toLowerCase();
  for (const about of abouts) {
    if (filter && !about.toLowerCase().includes(filter)) continue;
    const item = el("li", about === graph.about ? "active" : "", about);
    item.addEventListener("click", () => loadGraph(about));
    list.append(item);
  }
}

function renderLegend() {
  const kinds = new Map();
  for (const node of graph.nodes.values()) {
    kinds.set(node.kind, (kinds.get(node.kind) || 0) + 1);
  }
  const list = $("legend");
  list.textContent = "";
  for (const [kind, count] of [...kinds.entries()].sort((a, b) => b[1] - a[1])) {
    const item = el("li", dimmedKinds.has(kind) ? "dimmed" : "");
    const dot = el("span", "legend-dot");
    dot.style.background = kind === "?" ? palette().overflow : kindColor(kind);
    item.append(dot, el("span", "", `${kind} `), el("span", "muted", String(count)));
    item.title = "click to dim/undim this kind";
    item.addEventListener("click", () => {
      if (dimmedKinds.has(kind)) dimmedKinds.delete(kind);
      else dimmedKinds.add(kind);
      renderLegend();
      requestDraw();
    });
    list.append(item);
  }
}

$("search").addEventListener("input", () => {
  renderAbouts();
  const query = $("search").value.trim().toLowerCase();
  const results = $("search-results");
  results.textContent = "";
  searchHits = new Set();
  if (query.length >= 2) {
    for (const node of graph.nodes.values()) {
      if (
        node.title.toLowerCase().includes(query) ||
        node.summary.toLowerCase().includes(query) ||
        node.id.toLowerCase().includes(query)
      ) {
        searchHits.add(node.id);
      }
    }
    for (const id of [...searchHits].slice(0, 20)) {
      const node = graph.nodes.get(id);
      const item = el("li", "", node.title);
      item.append(el("span", "sub mono", node.id));
      item.addEventListener("click", () => {
        selectNode(id);
        centerOn(id);
      });
      results.append(item);
    }
  }
  requestDraw();
});

/* ---------------- detail panel ---------------- */

function renderDetailEmpty() {
  $("detail-empty").hidden = false;
  $("detail-body").hidden = true;
  $("context-body").hidden = true;
}

async function selectNode(id) {
  selectedId = id;
  requestDraw();
  try {
    const inspect = graph.nodes.has(id)
      ? await api("/api/node", { id, raw: "1" })
      : await expandNode(id);
    renderDetail(inspect);
  } catch (error) {
    showError(error.message);
  }
}

function renderDetail(inspect) {
  const { node } = inspect;
  $("detail-empty").hidden = true;
  $("context-body").hidden = true;
  $("detail-body").hidden = false;

  const kindPill = $("d-kind");
  kindPill.textContent = "";
  const dot = el("span", "legend-dot");
  dot.style.background = kindColor(node.kind);
  kindPill.append(dot, document.createTextNode(node.kind));
  $("d-status").textContent = node.status || "no status";
  $("d-title").textContent = node.title;
  $("d-id").textContent = node.id;
  $("d-summary").textContent = node.summary;

  const labels = $("d-labels");
  labels.textContent = "";
  for (const label of node.labels) labels.append(el("span", "pill pill-muted", label));

  $("d-detail").textContent =
    (inspect.detail && inspect.detail.detail) || graph.details.get(node.id) || "(no detail recorded)";

  const props = $("d-props-body");
  props.textContent = "";
  for (const [key, value] of Object.entries(node.properties)) {
    const row = el("tr");
    row.append(el("td", "mono", key), el("td", "", value));
    props.append(row);
  }
  if (!props.children.length) props.append(el("tr")).append(el("td", "muted", "none"));

  const coords = $("d-coords");
  coords.textContent = "";
  for (const c of inspect.raw_coordinates || []) {
    coords.append(
      el(
        "li",
        "",
        `${c.dimension} / ${c.scope_id}` +
          (c.sequence !== undefined ? ` · #${c.sequence}` : "") +
          (c.occurred_at ? ` · ${c.occurred_at}` : "")
      )
    );
  }
  if (!coords.children.length) coords.append(el("li", "muted", "none surfaced"));

  renderRelationList($("d-incoming"), inspect.incoming, "source");
  renderRelationList($("d-outgoing"), inspect.outgoing, "target");
}

/* The relation is where KMP carries the why: render the WHOLE explanation
   the recorder gave — rationale, evidence, confidence, method, motivation,
   causal anchors and the relation's own temporal coordinate. */
function renderRelationList(list, relations, counterpart) {
  list.textContent = "";
  if (!relations.length) {
    list.append(el("li", "muted", "none"));
    return;
  }
  const nodeLink = (id) => {
    const link = el("a", "rel-target mono", id);
    link.addEventListener("click", () => {
      selectNode(id).then(() => centerOn(id));
    });
    return link;
  };
  for (const relation of relations) {
    const item = el("li");
    const head = el("div", "rel-head");
    head.append(el("span", "rel-type", relation.rel), el("span", "pill pill-muted", relation.class));
    if (relation.confidence) {
      head.append(el("span", "pill pill-muted", `confidence: ${relation.confidence}`));
    }
    head.append(nodeLink(relation[counterpart]));
    if (relation.coordinate || relation.dimension) {
      const c = relation.coordinate || relation;
      head.append(
        el(
          "span",
          "pill pill-muted mono",
          `${c.dimension}${c.sequence !== undefined ? " #" + c.sequence : ""}${
            c.occurred_at ? " " + c.occurred_at.slice(11, 16) : ""
          }`
        )
      );
    }
    item.append(head);
    if (relation.why) item.append(el("p", "rel-why", relation.why));
    if (relation.motivation) item.append(el("p", "rel-why", `motivación: ${relation.motivation}`));
    if (relation.method) item.append(el("p", "rel-evidence", `método: ${relation.method}`));
    if (relation.evidence) item.append(el("p", "rel-evidence", `evidencia: ${relation.evidence}`));
    const anchors = el("p", "rel-evidence");
    if (relation.decision_id) {
      anchors.append(document.createTextNode("decisión: "), nodeLink(relation.decision_id));
    }
    if (relation.caused_by) {
      if (anchors.childNodes.length) anchors.append(document.createTextNode(" · "));
      anchors.append(document.createTextNode("causado por: "), nodeLink(relation.caused_by));
    }
    if (anchors.childNodes.length) item.append(anchors);
    list.append(item);
  }
}

$("btn-context").addEventListener("click", () => {
  if (!graph.rendered) return;
  $("detail-empty").hidden = true;
  $("detail-body").hidden = true;
  $("context-body").hidden = false;
  $("ctx-hash").textContent = graph.rendered.content_hash
    ? `sha256 ${graph.rendered.content_hash}`
    : "no snapshot hash for this recall";
  $("ctx-content").textContent = graph.rendered.content;
});

/* ---------------- status bar ---------------- */

function renderStats(view) {
  $("s-revision").textContent = String(view.revision);
  // No hash means none has been computed — the field is hidden rather than
  // filled with a word that looks like a state it will never leave.
  $("s-hash").textContent = shortHash(view.content_hash);
  $("s-hash").closest(".stat").hidden = !view.content_hash;
  $("s-nodes").textContent = String(graph.nodes.size);
  $("s-edges").textContent = String(graph.edges.length);
  $("s-tokens").textContent = String(view.rendered.token_count);
  const q = view.quality;
  $("s-compression").textContent = `×${q.compression_ratio.toFixed(1)}`;
  $("s-causal").textContent = `${Math.round(q.causal_density * 100)}%`;
  $("s-noise").textContent = `${Math.round(q.noise_ratio * 100)}%`;
}

/* ---------------- graph loading ---------------- */

async function loadGraph(about) {
  stopReplay();
  try {
    const view = await api("/api/graph", {
      about,
      depth: $("ctl-depth").value,
      budget: $("ctl-budget").value,
      scope: $("ctl-scope").value,
    });
    resetGraph(view);
    renderAbouts();
    renderLegend();
    renderStats(view);
    renderDetailEmpty();
    selectedId = null;
    camera.x = 0;
    camera.y = 0;
    camera.k = Math.min(1, 26 / Math.max(26, graph.nodes.size));
    reheat();
    showError("");
  } catch (error) {
    showError(error.message);
  }
}

$("btn-reload").addEventListener("click", () => graph.about && loadGraph(graph.about));
for (const id of ["ctl-depth", "ctl-budget", "ctl-scope"]) {
  $(id).addEventListener("change", () => graph.about && loadGraph(graph.about));
}

/* ---------------- tabs ---------------- */

const panels = { graph: null, timeline: $("timeline-panel"), trace: $("trace-panel") };

for (const tab of document.querySelectorAll(".tab")) {
  tab.addEventListener("click", () => {
    for (const other of document.querySelectorAll(".tab")) {
      other.classList.toggle("active", other === tab);
      other.setAttribute("aria-selected", other === tab ? "true" : "false");
    }
    for (const [name, panel] of Object.entries(panels)) {
      if (panel) panel.hidden = name !== tab.dataset.tab;
    }
    if (tab.dataset.tab === "graph") requestDraw();
  });
}

/* ---------------- timeline ---------------- */

$("tl-now").addEventListener("click", () => {
  $("tl-time").value = nowIso();
});

// `goto` and `rewind` walk by temporal position. Given a ref they resolve the
// position from that entry and always work; given only a timestamp they need
// the entries themselves to carry one, and `sequence` is optional at ingest —
// so on memory written without it they answer 0/0 and said nothing at all.
// A selected entry is therefore the reliable cursor, and its absence is worth
// explaining rather than silently answering nothing.
const POSITION_DIRECTIONS = new Set(["goto", "rewind"]);

$("tl-apply").addEventListener("click", () => {
  if (!graph.about) return;
  const direction = $("tl-direction").value;
  const params = {
    about: graph.about,
    direction,
    before: $("tl-before").value,
    after: $("tl-after").value,
  };
  if (POSITION_DIRECTIONS.has(direction) && selectedId) {
    params.ref = selectedId;
  } else {
    params.time = $("tl-time").value.trim() || nowIso();
  }
  runTimeline(params, { direction, positioned: Boolean(params.ref) });
});

/// Why an empty answer is empty, in the reader's terms.
///
/// The distinction that matters: a window with nothing in it is ordinary, but
/// a walk-by-position direction over entries that carry no position is a
/// property of how the memory was written, and no widening of the window will
/// ever help. Saying "no entries" to both is how this panel looked broken.
function emptyTimelineReason(view, context) {
  if (POSITION_DIRECTIONS.has(context.direction) && !context.positioned) {
    return (
      `“${directionLabel(context.direction)}” walks from a temporal position, and nothing here ` +
      `carries one — these entries were written without a sequence. Select an entry in the ` +
      `graph to walk from it, or use “around a time”.`
    );
  }
  if (view.missing.length) {
    return `The cursor resolved nothing (${view.missing.join(", ")}).`;
  }
  return "No entry sits in that window. Try widening it, or another direction.";
}

function directionLabel(value) {
  const option = $("tl-direction").querySelector(`option[value="${value}"]`);
  return option ? option.textContent : value;
}

async function runTimeline(params, context = {}) {
  try {
    const view = await api("/api/timeline", params);
    const cursor = view.resolved_cursor;
    const cursorBits = [
      `cursor: ${cursor.dimension}/${cursor.scope_id}`,
      cursor.sequence !== undefined ? `#${cursor.sequence}` : null,
      cursor.occurred_at || null,
      `· ${view.page.returned}/${view.page.total} entries`,
      view.missing_dimensions.length ? `· missing dims: ${view.missing_dimensions.join(", ")}` : null,
      view.missing.length ? `· missing: ${view.missing.join(", ")}` : null,
    ].filter(Boolean);
    $("tl-status").textContent = cursorBits.join(" ");
    const list = $("tl-entries");
    list.textContent = "";
    if (!view.entries.length) {
      list.append(el("li", "tl-notice", emptyTimelineReason(view, context)));
      return;
    }
    for (const entry of view.entries) {
      const item = el("li");
      const meta = el("div", "tl-meta");
      const kindPill = el("span", "pill");
      const dot = el("span", "legend-dot");
      dot.style.background = kindColor(entry.kind);
      kindPill.append(dot, document.createTextNode(entry.kind));
      meta.append(kindPill);
      for (const c of entry.coordinates) {
        meta.append(
          el(
            "span",
            "pill pill-muted mono",
            `${c.dimension}${c.sequence !== undefined ? " #" + c.sequence : ""}${
              c.occurred_at ? " " + c.occurred_at : ""
            }`
          )
        );
      }
      meta.append(el("span", "muted mono", entry.ref_id));
      item.append(meta, el("div", "tl-text", entry.text));
      if (entry.ref_id === selectedId) item.classList.add("cursor");
      item.addEventListener("click", () => selectNode(entry.ref_id));
      item.addEventListener("dblclick", () => {
        selectNode(entry.ref_id).then(() => {
          document.querySelector('.tab[data-tab="graph"]').click();
          centerOn(entry.ref_id);
        });
      });
      list.append(item);
    }
    if (!view.entries.length) {
      list.append(el("li", "muted", "No entries in this window."));
    }
  } catch (error) {
    showError(error.message);
  }
}

/* ---------------- trace ---------------- */

$("tr-use").addEventListener("click", () => {
  if (!selectedId) return;
  if (!$("tr-from").value.trim()) $("tr-from").value = selectedId;
  else $("tr-to").value = selectedId;
});

$("tr-clear").addEventListener("click", () => {
  traceHighlight = null;
  $("tr-path").textContent = "";
  $("tr-status").textContent = "";
  $("tr-rendered").textContent = "";
  requestDraw();
});

$("tr-apply").addEventListener("click", async () => {
  const from = $("tr-from").value.trim();
  const to = $("tr-to").value.trim();
  if (!from || !to) {
    showError("trace needs both `from` and `to` node ids");
    return;
  }
  try {
    const view = await api("/api/trace", { from, to, budget: $("ctl-budget").value });
    for (const nodeView of view.nodes) {
      if (!graph.nodes.has(nodeView.id)) placeNear(upsertNode(nodeView), centerNode());
      else upsertNode(nodeView);
    }
    addEdges(view.edges);
    traceHighlight = {
      nodes: new Set(view.nodes.map((n) => n.id)),
      edges: new Set(view.edges.map(edgeKey)),
    };
    reheat();
    renderLegend();
    $("tr-status").textContent =
      `${view.nodes.length} nodes, ${view.edges.length} relations · ` +
      `${view.rendered.token_count} tokens rendered`;
    const list = $("tr-path");
    list.textContent = "";
    for (const edge of orderPath(view, from)) {
      const item = el("li");
      const head = el("div", "rel-head");
      const source = el("a", "rel-target mono", edge.source);
      source.addEventListener("click", () => selectNode(edge.source));
      const target = el("a", "rel-target mono", edge.target);
      target.addEventListener("click", () => selectNode(edge.target));
      head.append(
        source,
        el("span", "rel-type", ` —${edge.rel}→ `),
        target,
        el("span", "pill pill-muted", edge.class)
      );
      item.append(head);
      if (edge.why) item.append(el("p", "rel-why", edge.why));
      list.append(item);
    }
    $("tr-rendered").textContent = view.rendered.content;
    showError("");
  } catch (error) {
    showError(error.message);
  }
});

/* Order path edges by walking from `from`; edges that do not chain are kept
   at the end rather than hidden. */
function orderPath(view, from) {
  const remaining = [...view.edges];
  const ordered = [];
  let cursor = from;
  let progressing = true;
  while (progressing) {
    progressing = false;
    for (let i = 0; i < remaining.length; i += 1) {
      const edge = remaining[i];
      if (edge.source === cursor || edge.target === cursor) {
        ordered.push(edge);
        cursor = edge.source === cursor ? edge.target : edge.source;
        remaining.splice(i, 1);
        progressing = true;
        break;
      }
    }
  }
  return [...ordered, ...remaining];
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
    const info = await api("/api/info");
    if (info.data_dir) $("data-dir").textContent = info.data_dir;
    const aboutsView = await api("/api/abouts");
    abouts = aboutsView.abouts;
    renderAbouts();
    if (abouts.length) {
      await loadGraph(abouts[0]);
    } else {
      showError("the kernel holds no abouts yet — ingest some memory first");
    }
  } catch (error) {
    showError(error.message);
  }
}

init();
