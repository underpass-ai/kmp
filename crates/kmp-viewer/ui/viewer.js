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
const nowIso = () => new Date().toISOString().replace(/\.\d+Z$/, "Z");

/* ---------------- theme ---------------- */

const THEMES = ["auto", "light", "dark"];
let themeIndex = 0;

/* The stylesheet holds each theme exactly once, keyed on data-theme; `auto`
   is resolved here, so the attribute is always stamped and the CSS never
   needs a duplicated media-query block. */
function applyTheme() {
  const choice = THEMES[themeIndex];
  const dark =
    choice === "dark" ||
    (choice === "auto" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  $("btn-theme").textContent = choice[0].toUpperCase() + choice.slice(1);
  invalidatePalette();
  requestDraw();
}

$("btn-theme").addEventListener("click", () => {
  themeIndex = (themeIndex + 1) % THEMES.length;
  applyTheme();
});
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyTheme);

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
      halo: read("--halo"),
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
        dimension: read("--kind-dimension"),
      },
      edgeClass: {
        causal: read("--edge-causal"),
        evidential: read("--edge-evidential"),
        temporal: read("--edge-temporal"),
      },
    };
  }
  return paletteCache;
}

function invalidatePalette() {
  paletteCache = null;
  resetLabelPool();
}

/* Kinds wear the wire's own names — memory_anchor, decision, success_path —
   each with a fixed ink in both themes. A kind the palette has never heard
   of wears neutral; the legend and the label still carry its identity. */
function kindColor(kind) {
  const p = palette();
  return p.kind[kind] || p.overflow;
}

/* Edges finally encode their class: causal wears the accent blue,
   evidential the green, temporal the violet — three stops of the mark's
   gradient — and structural scaffolding stays quiet grey. */
function edgeInk(edge, p) {
  return p.edgeClass[edge.class] || p.edge;
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

/* Fresh nodes land on a golden-angle spiral around their anchor — dense,
   round, and deterministic: the same store draws the same picture on every
   reload, because the layout is a picture of the memory, not of the dice. */
let spawnIndex = 0;

function placeNear(node, anchor) {
  const at = KMP_CORE.spiralPoint(
    spawnIndex,
    node.id,
    anchor ? anchor.x : 0,
    anchor ? anchor.y : 0
  );
  spawnIndex += 1;
  node.x = at.x;
  node.y = at.y;
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
  spawnIndex = 0;
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
  rebuildClusters(false);
  reheat(KMP_CORE.SIM.REHEAT_EXPAND);
  return inspect;
}

const centerNode = () => graph.nodes.get(graph.rootId) || graph.nodes.values().next().value;

/* ---------------- view model: clusters and folding ----------------
   The drawn graph is a view over the data: every node resolves to a proxy —
   itself, or the meta-mark of its folded dimension. Physics, drawing and hit
   testing all run on `view`, so folding is one rebuild, not a special case
   in every loop. The grouping itself (buildClusters) is pure and lives in
   graph-core.js. */

const clusterState = {
  of: new Map(), // nodeId -> clusterId
  clusters: new Map(), // clusterId -> {id, head, label, members, kindCounts}
  collapsed: new Set(),
  metas: new Map(), // clusterId -> meta body
};

const scene = { bodies: [], byId: new Map(), edges: [] };

/* Graphs at or under this many nodes start fully expanded; bigger ones fold
   their dimensions so the first sight is a map, not a hairball. Any cluster
   actually holding something folds — a dimension with one entry is still one
   mark instead of three. */
const FOLD_THRESHOLD = 120;
const FOLD_MIN_SIZE = 2;

function rebuildClusters(fresh) {
  const built = KMP_CORE.buildClusters(
    [...graph.nodes.values()],
    graph.edges,
    graph.rootId
  );
  clusterState.of = built.of;
  clusterState.clusters = built.clusters;
  for (const id of [...clusterState.collapsed]) {
    if (!built.clusters.has(id)) clusterState.collapsed.delete(id);
  }
  for (const id of [...clusterState.metas.keys()]) {
    if (!built.clusters.has(id)) clusterState.metas.delete(id);
  }
  if (fresh) {
    clusterState.collapsed.clear();
    clusterState.metas.clear();
    if (graph.nodes.size > FOLD_THRESHOLD) {
      for (const cluster of built.clusters.values()) {
        const size = cluster.members.length + (cluster.head ? 1 : 0);
        if (size >= FOLD_MIN_SIZE) clusterState.collapsed.add(cluster.id);
      }
    }
  }
  rebuildView();
  renderDimensions();
}

function metaFor(clusterId) {
  let meta = clusterState.metas.get(clusterId);
  const cluster = clusterState.clusters.get(clusterId);
  if (!meta) {
    // Seed where the fold happens: the head node, or the members' centroid.
    let x = 0;
    let y = 0;
    let n = 0;
    const head = cluster.head && graph.nodes.get(cluster.head);
    if (head) {
      x = head.x;
      y = head.y;
      n = 1;
    } else {
      for (const id of cluster.members) {
        const node = graph.nodes.get(id);
        if (!node) continue;
        x += node.x;
        y += node.y;
        n += 1;
      }
    }
    meta = {
      id: `meta:${clusterId}`,
      meta: true,
      kind: "memory_dimension",
      title: cluster.label,
      x: n ? x / n : 0,
      y: n ? y / n : 0,
      vx: 0,
      vy: 0,
      degree: 0,
      mass: 1,
    };
    clusterState.metas.set(clusterId, meta);
  }
  meta.cluster = cluster;
  meta.title = cluster.label;
  meta.mass = cluster.members.length + (cluster.head ? 1 : 0);
  return meta;
}

function proxyOf(nodeId) {
  const clusterId = clusterState.of.get(nodeId);
  if (clusterId && clusterState.collapsed.has(clusterId)) return metaFor(clusterId);
  return graph.nodes.get(nodeId) || null;
}

function rebuildView() {
  scene.bodies = [];
  scene.byId = new Map();
  for (const node of graph.nodes.values()) {
    const clusterId = clusterState.of.get(node.id);
    if (clusterId && clusterState.collapsed.has(clusterId)) continue;
    scene.bodies.push(node);
    scene.byId.set(node.id, node);
  }
  for (const clusterId of clusterState.collapsed) {
    const meta = metaFor(clusterId);
    scene.bodies.push(meta);
    scene.byId.set(meta.id, meta);
  }

  // Edges land on proxies; whatever folds together aggregates.
  const aggregated = new Map();
  for (const edge of graph.edges) {
    const s = proxyOf(edge.source);
    const t = proxyOf(edge.target);
    if (!s || !t || s === t) continue;
    const key = `${s.id} ${t.id}`;
    let entry = aggregated.get(key);
    if (!entry) {
      aggregated.set(
        key,
        (entry = { source: s.id, target: t.id, weight: 0, classCounts: new Map(), one: edge })
      );
    }
    entry.weight += 1;
    entry.classCounts.set(edge.class, (entry.classCounts.get(edge.class) || 0) + 1);
  }
  scene.edges = [];
  for (const entry of aggregated.values()) {
    const sMeta = scene.byId.get(entry.source);
    const tMeta = scene.byId.get(entry.target);
    if (entry.weight === 1 && sMeta && !sMeta.meta && tMeta && !tMeta.meta) {
      // A single edge between two real nodes keeps its true payload, so the
      // tooltip and the trace can still speak for it.
      scene.edges.push(entry.one);
      continue;
    }
    let major = null;
    let majorCount = -1;
    for (const [cls, count] of entry.classCounts) {
      if (count > majorCount) {
        major = cls;
        majorCount = count;
      }
    }
    scene.edges.push({
      source: entry.source,
      target: entry.target,
      rel: `${entry.weight} relations`,
      class: major,
      aggregate: true,
      weight: entry.weight,
    });
  }

  for (const body of scene.bodies) body.degree = 0;
  for (const edge of scene.edges) {
    const s = scene.byId.get(edge.source);
    const t = scene.byId.get(edge.target);
    if (s) s.degree += 1;
    if (t) t.degree += 1;
  }
  requestDraw();
}

function expandCluster(clusterId) {
  if (!clusterState.collapsed.has(clusterId)) return;
  const cluster = clusterState.clusters.get(clusterId);
  const meta = clusterState.metas.get(clusterId);
  clusterState.collapsed.delete(clusterId);
  const at = meta || centerNode() || { x: 0, y: 0 };
  const ids = cluster.head ? [cluster.head, ...cluster.members] : cluster.members;
  let i = 0;
  for (const id of ids) {
    const node = graph.nodes.get(id);
    if (!node) continue;
    const p = KMP_CORE.spiralPoint(i, id, at.x, at.y);
    i += 1;
    node.x = p.x;
    node.y = p.y;
    node.vx = 0;
    node.vy = 0;
  }
  if (selectedId === `meta:${clusterId}`) selectedId = null;
  rebuildView();
  renderDimensions();
  reheat(KMP_CORE.SIM.REHEAT_EXPAND);
}

function collapseCluster(clusterId) {
  const cluster = clusterState.clusters.get(clusterId);
  if (!cluster || clusterState.collapsed.has(clusterId)) return;
  clusterState.metas.delete(clusterId); // reseed at the current centroid
  clusterState.collapsed.add(clusterId);
  if (selectedId && clusterState.of.get(selectedId) === clusterId) {
    selectedId = null;
    renderDetailEmpty();
  }
  rebuildView();
  renderDimensions();
  reheat(KMP_CORE.SIM.REHEAT_DRAG);
}

/* ---------------- force simulation ----------------
   The physics lives in graph-core.js (KMP_CORE.simStep): degree-normalized
   links, Barnes-Hut repulsion, centering, a hard velocity cap. What stays
   here is the thermostat — alpha, reheat levels, and the one-shot fit that
   frames the graph when a fresh load settles. */

let alpha = 0;
let settleFitPending = false;

function reheat(level = KMP_CORE.SIM.REHEAT_LOAD) {
  alpha = Math.max(alpha, level);
  requestDraw();
}

function simTick() {
  KMP_CORE.simStep(
    scene.bodies,
    scene.edges,
    (id) => scene.byId.get(id),
    alpha,
    { pinned: dragNode, radiusOf: nodeRadius }
  );
  alpha -= alpha * KMP_CORE.SIM.ALPHA_DECAY;
  if (settleFitPending && alpha < 0.3) {
    settleFitPending = false;
    fitToView();
  }
  if (alpha < KMP_CORE.SIM.ALPHA_MIN) alpha = 0;
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
    if (tickCamera(performance.now())) dirty = true;
    if (playback.active) dirty = true;
    if (dirty) {
      dirty = false;
      syncScene();
    }
  });
}

/* ---------------- camera motion ----------------
   One tween at a time, cubic-out. Any hand on the camera — a pan, a wheel —
   cancels it: the machine never fights the person for the wheel. */

let camTween = null;

function animateCamera(target, ms = 400) {
  camTween = {
    fromX: camera.x,
    fromY: camera.y,
    fromK: camera.k,
    toX: target.x,
    toY: target.y,
    toK: target.k === undefined ? camera.k : target.k,
    start: performance.now(),
    ms,
  };
  requestDraw();
}

function tickCamera(now) {
  if (!camTween) return false;
  const t = Math.min(1, (now - camTween.start) / camTween.ms);
  const eased = 1 - Math.pow(1 - t, 3);
  camera.x = camTween.fromX + (camTween.toX - camTween.fromX) * eased;
  camera.y = camTween.fromY + (camTween.toY - camTween.fromY) * eased;
  camera.k = camTween.fromK + (camTween.toK - camTween.fromK) * eased;
  if (t >= 1) camTween = null;
  return true;
}

/* Frame everything drawn (or the bodies `include` admits) with breathing room. */
function fitToView(include) {
  const bounds = KMP_CORE.computeBounds(scene.bodies, include);
  const target = KMP_CORE.fitTransform(bounds, canvas.clientWidth, canvas.clientHeight);
  if (target) animateCamera(target);
}

const nodeRadius = (node) =>
  node.meta
    ? 10 + 4 * Math.log2(node.mass + 1)
    : 7 + Math.min(9, Math.sqrt(node.degree) * 2) + (node.id === graph.rootId ? 3 : 0);

const toScreen = (x, y) => ({
  x: (x - camera.x) * camera.k + canvas.clientWidth / 2,
  y: (y - camera.y) * camera.k + canvas.clientHeight / 2,
});
const toWorld = (sx, sy) => ({
  x: (sx - canvas.clientWidth / 2) / camera.k + camera.x,
  y: (sy - canvas.clientHeight / 2) / camera.k + camera.y,
});

function nodeAlpha(body) {
  if (playback.active) {
    if (body.meta) {
      // A folded dimension reveals in proportion to its revealed members.
      const revealed = playback.revealedSet;
      let seen = 0;
      for (const id of body.cluster.members) if (revealed.has(id)) seen += 1;
      if (body.cluster.head && revealed.has(body.cluster.head)) seen += 1;
      return seen ? 0.25 + 0.65 * (seen / body.mass) : 0;
    }
    if (body.id === playback.currentRef) return 1;
    if (!playback.revealedSet.has(body.id)) return 0; // the future is not there yet
    if (playback.scaffolding.has(body.id)) return 0.4;
    const born = playback.stepOf.get(body.id);
    return born !== undefined && playback.step - born <= RECENT_STEPS ? 0.9 : 0.55;
  }
  if (traceHighlight) return traceHighlight.nodes.has(body.id) ? 1 : 0.14;
  if (!body.meta && dimmedKinds.has(body.kind)) return 0.14;
  if (searchHits.size) {
    if (body.meta) return 0.25;
    return searchHits.has(body.id) ? 1 : 0.25;
  }
  return 1;
}

function edgeAlpha(edge) {
  if (playback.active && !edge.aggregate) {
    const revealed = playback.revealedSet;
    if (!revealed.has(edge.source) || !revealed.has(edge.target)) return 0;
    return edge.source === playback.currentRef || edge.target === playback.currentRef ? 1 : 0.55;
  }
  if (traceHighlight && !edge.aggregate) {
    return traceHighlight.edges.has(edgeKey(edge)) ? 1 : 0.08;
  }
  const s = scene.byId.get(edge.source);
  const t = scene.byId.get(edge.target);
  return Math.min(s ? nodeAlpha(s) : 1, t ? nodeAlpha(t) : 1);
}

/* The world-space rectangle the camera can see, padded so things entering
   the frame are already drawn. Everything outside is culled. */
function viewportRect(pad) {
  const halfW = (canvas.clientWidth / 2 + pad) / camera.k;
  const halfH = (canvas.clientHeight / 2 + pad) / camera.k;
  return {
    x0: camera.x - halfW,
    x1: camera.x + halfW,
    y0: camera.y - halfH,
    y1: camera.y + halfH,
  };
}

/* Rebuilds the scene in world coordinates; the root container carries the
   camera, and screen-constant sizes are divided by the zoom factor.
   Levels of detail: far out, arrowheads and self-loops go and member labels
   yield to meta labels; close in, everything speaks. */
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
  const rect = viewportRect(100);
  const drawDetail = k >= 0.3; // arrowheads, self-loops
  const farDim = k < 0.12 ? 0.6 : 1;

  edgesGfx.clear();
  for (const edge of scene.edges) {
    const s = scene.byId.get(edge.source);
    const t = scene.byId.get(edge.target);
    if (!s || !t) continue;
    if (
      Math.max(s.x, t.x) < rect.x0 ||
      Math.min(s.x, t.x) > rect.x1 ||
      Math.max(s.y, t.y) < rect.y0 ||
      Math.min(s.y, t.y) > rect.y1
    ) {
      continue;
    }
    const a = edgeAlpha(edge) * farDim;
    if (a <= 0.02) continue;
    const highlighted =
      edge === hoveredEdge ||
      (traceHighlight && traceHighlight.edges.has(edgeKey(edge))) ||
      (playback.active &&
        (edge.source === playback.currentRef || edge.target === playback.currentRef));
    const classed = edgeInk(edge, p);
    const structural = classed === p.edge;
    const color = highlighted ? p.accent : classed;
    const base = edge.aggregate ? 1 + Math.log2(edge.weight) : structural ? 0.9 : 1.3;
    const width = (highlighted ? Math.max(2.5, base) : base) / k;
    if (s === t) {
      if (!drawDetail) continue;
      const r = nodeRadius(s);
      edgesGfx
        .circle(s.x + r, s.y - r, r * 0.8)
        .stroke({ width, color, alpha: a });
      continue;
    }
    edgesGfx.moveTo(s.x, s.y).lineTo(t.x, t.y).stroke({ width, color, alpha: a });
    if (drawDetail) drawArrowhead(edge, s, t, color, a);
  }

  nodesGfx.clear();
  const wantedLabels = new Set();
  for (const body of scene.bodies) {
    if (body.x < rect.x0 || body.x > rect.x1 || body.y < rect.y0 || body.y > rect.y1) continue;
    const a = nodeAlpha(body);
    if (a <= 0.01) continue; // in travel, the future is culled, not greyed
    let r = Math.max(nodeRadius(body), 3 / k);
    const isCurrentStep = playback.active && body.id === playback.currentRef;
    if (isCurrentStep) {
      r += (1.8 + 1.6 * Math.sin(performance.now() / 160)) / k;
    }
    if (body.meta) {
      drawMeta(body, r, a, p, k);
    } else {
      nodesGfx
        .circle(body.x, body.y, r)
        .fill({ color: body.kind === "?" ? p.overflow : kindColor(body.kind), alpha: a })
        .stroke({ width: 2 / k, color: p.surface, alpha: a });
    }
    if (body.id === graph.rootId || body.id === selectedId || isCurrentStep) {
      const emphasized = body.id === selectedId || isCurrentStep;
      nodesGfx.circle(body.x, body.y, r + 3 / k).stroke({
        width: (emphasized ? 2.5 : 1.5) / k,
        color: emphasized ? p.accent : p.textMuted,
        alpha: a,
      });
      if (emphasized) {
        // The glow: one wide, faint ring — selection reads from across the room.
        nodesGfx.circle(body.x, body.y, r + 7 / k).stroke({
          width: 6 / k,
          color: p.accent,
          alpha: a * 0.18,
        });
      }
    }
    const emphasizedLabel =
      body === hoveredNode ||
      body.id === selectedId ||
      body.id === graph.rootId ||
      isCurrentStep ||
      searchHits.has(body.id);
    // A meta speaks when its mark is big enough on screen to own the words;
    // small folds wait for the zoom. Members wait for k > 0.55 as ever.
    const showLabel = body.meta ? r * k > 8 || emphasizedLabel : k > 0.55 || emphasizedLabel;
    if (showLabel && a > 0.2) {
      wantedLabels.add(body.id);
      syncLabel(body, r, a, p, k);
    }
  }
  for (const [id, label] of labelPool) {
    if (!wantedLabels.has(id)) label.visible = false;
  }
}

/* A folded dimension: a quiet disc wearing a donut of its members' kinds,
   sized by how much memory it holds. */
function drawMeta(body, r, a, p, k) {
  nodesGfx
    .circle(body.x, body.y, r)
    .fill({ color: p.surface, alpha: a })
    .stroke({ width: 1.5 / k, color: kindColor("memory_dimension"), alpha: a * 0.8 });
  const total = body.mass || 1;
  let angle = -Math.PI / 2;
  for (const [kind, count] of body.cluster.kindCounts) {
    const sweep = (count / total) * Math.PI * 2;
    nodesGfx
      .arc(body.x, body.y, r - 2.5 / k, angle, angle + sweep)
      .stroke({ width: 4 / k, color: kindColor(kind), alpha: a });
    angle += sweep;
  }
}

/* The label pool holds at most this many PIXI.Text objects; past it, the
   oldest unwanted label is destroyed. Text is the most expensive thing on
   the stage — a 962-entry memory must not become 962 baked textures. */
const LABEL_POOL_MAX = 400;

function syncLabel(node, radius, alphaValue, p, k) {
  let label = labelPool.get(node.id);
  const raw = node.meta ? `${node.title} · ${node.mass}` : node.title;
  const text = raw.length > 34 ? raw.slice(0, 33) + "…" : raw;
  if (!label && labelPool.size >= LABEL_POOL_MAX) {
    for (const [oldId, oldLabel] of labelPool) {
      if (!oldLabel.visible) {
        oldLabel.destroy();
        labelPool.delete(oldId);
        break;
      }
    }
  }
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

/* ---------------- time travel ----------------
   The whole timeline, walked page by page over the kernel's own temporal
   read, then scrubbed on a density strip: every entry is a tick in its
   kind's ink, the played region wears the gradient left to right, and the
   graph shows the memory as of the playhead — the future simply is not
   there yet. */

/// Nodes that belong to an entry rather than standing beside it in time:
/// the dimension it was written into, the evidence that supports it. They
/// carry no coordinate of their own and reveal with their entry.
const ATTACHING_RELATIONS = new Set([
  "has_dimension",
  "has_evidence",
  "contains_entry",
  "supports",
  "records",
]);

/* Fetch caps: pages of 256 by ref cursor, at most this many entries. */
const TRAVEL_MAX_ENTRIES = 2048;
const RECENT_STEPS = 12;

const playback = {
  active: false,
  entries: [],
  steps: [], // step -> ids first revealed at that step
  stepOf: new Map(), // id -> first step
  scaffolding: new Set(),
  revealedSet: new Set(),
  step: 0,
  currentRef: null,
  timer: null,
  stepMs: 1200,
  xs: [], // step -> 0..1 position on the strip
  placed: 0, // entries carrying a time
  total: 0, // what the kernel says the line holds
};

/* The whole line: one `near` window around now, then `forward` by cursor
   for the newer side and `rewind` from the earliest for the older side.
   Merged on ref, ordered by the entries' own coordinates. */
async function fetchWholeTimeline(about) {
  const byRef = new Map();
  const absorb = (entries) => {
    let fresh = 0;
    for (const entry of entries) {
      if (!byRef.has(entry.ref_id)) {
        byRef.set(entry.ref_id, entry);
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
  const total = first.page.total || first.entries.length;

  let cursor = first.page.next_cursor;
  for (let guard = 0; cursor && byRef.size < TRAVEL_MAX_ENTRIES && guard < 16; guard += 1) {
    const page = await api("/api/timeline", { about, direction: "forward", ref: cursor, after: 256 });
    if (!absorb(page.entries)) break;
    cursor = page.page.next_cursor;
  }
  for (let guard = 0; byRef.size < TRAVEL_MAX_ENTRIES && guard < 16; guard += 1) {
    const sorted = [...byRef.values()].sort(KMP_CORE.compareEntries);
    const earliest = sorted[0] && sorted[0].ref_id;
    if (!earliest) break;
    const page = await api("/api/timeline", { about, direction: "rewind", ref: earliest, before: 256 });
    if (!absorb(page.entries)) break;
  }
  return { entries: [...byRef.values()].sort(KMP_CORE.compareEntries), total: Math.max(total, byRef.size) };
}

async function startReplay(atRef) {
  if (!graph.about) return;
  try {
    const line = await fetchWholeTimeline(graph.about);
    if (!line.entries.length) {
      showError(`nothing to travel: ${graph.about} has no entries carrying a temporal coordinate`);
      return;
    }
    playback.entries = line.entries;
    playback.total = line.total;
    const built = KMP_CORE.buildRevealSteps(line.entries, graph.edges, ATTACHING_RELATIONS);
    playback.steps = built.steps;
    playback.stepOf = built.stepOf;
    playback.scaffolding = built.scaffolding;
    playback.revealedSet = new Set();
    playback.step = -1;

    // Strip positions: entries with a time land where their moment falls;
    // the timeless inherit their predecessor's spot.
    const times = line.entries.map((entry) => KMP_CORE.entryOrderKey(entry).time || null);
    const timed = times.filter(Boolean);
    playback.placed = timed.length;
    const denominator = Math.max(1, line.entries.length - 1);
    if (timed.length < line.entries.length / 2) {
      // Most entries carry order but no clock: spread by order, and say so.
      playback.xs = times.map((_, i) => i / denominator);
    } else {
      const t0 = Date.parse(timed[0]);
      const span = Math.max(1, Date.parse(timed[timed.length - 1]) - t0);
      let lastX = 0;
      playback.xs = times.map((time) => {
        if (time) lastX = timed.length > 1 ? (Date.parse(time) - t0) / span : 0.5;
        return lastX;
      });
    }

    playback.active = true;
    $("timebar").hidden = false;
    $("btn-replay").textContent = "■ Stop";
    const startStep = atRef && playback.stepOf.has(atRef) ? playback.stepOf.get(atRef) : 0;
    setReplayStep(startStep);
    setReplayPlaying(!atRef);
    showError("");
  } catch (error) {
    showError(error.message);
  }
}

function setReplayStep(step) {
  const clamped = Math.max(0, Math.min(step, playback.entries.length - 1));
  if (clamped > playback.step) {
    for (let i = playback.step + 1; i <= clamped; i += 1) {
      for (const id of playback.steps[i]) playback.revealedSet.add(id);
    }
  } else if (clamped < playback.step) {
    playback.revealedSet = new Set();
    for (let i = 0; i <= clamped; i += 1) {
      for (const id of playback.steps[i]) playback.revealedSet.add(id);
    }
  }
  playback.step = clamped;
  const entry = playback.entries[clamped];
  playback.currentRef = entry.ref_id;

  const key = KMP_CORE.entryOrderKey(entry);
  $("tb-label").textContent =
    `${clamped + 1}/${playback.entries.length}` +
    (key.time ? ` · ${key.time.slice(0, 16).replace("T", " ")}` : "") +
    (playback.placed < playback.entries.length
      ? ` · ${playback.placed} of ${playback.entries.length} placed in time`
      : "");
  $("tb-caption").textContent =
    entry.text.length > 160 ? entry.text.slice(0, 159) + "…" : entry.text;

  if (playback.timer) {
    const body = proxyOf(entry.ref_id);
    if (body) animateCamera({ x: body.x, y: body.y }, 600);
  }
  drawTimebar();
  requestDraw();
}

function setReplayPlaying(playing) {
  clearInterval(playback.timer);
  playback.timer = null;
  if (playing && playback.active) {
    playback.timer = setInterval(() => {
      if (playback.step >= playback.entries.length - 1) setReplayPlaying(false);
      else setReplayStep(playback.step + 1);
    }, playback.stepMs);
  }
  $("tb-toggle").textContent = playback.timer ? "⏸" : "▶";
}

function stopReplay() {
  if (!playback.active) return;
  setReplayPlaying(false);
  playback.active = false;
  playback.currentRef = null;
  $("timebar").hidden = true;
  $("btn-replay").textContent = "◈ Travel";
  requestDraw();
}

/* The strip itself: kind-colored ticks on a 2d canvas, the played region
   washed with the mark's gradient, the playhead in accent ink. */
function drawTimebar() {
  const strip = $("tb-canvas");
  const width = strip.clientWidth || 1;
  const height = 34;
  const dpr = devicePixelRatio || 1;
  if (strip.width !== Math.round(width * dpr)) strip.width = Math.round(width * dpr);
  if (strip.height !== Math.round(height * dpr)) strip.height = Math.round(height * dpr);
  const pen = strip.getContext("2d");
  pen.setTransform(dpr, 0, 0, dpr, 0, 0);
  pen.clearRect(0, 0, width, height);
  if (!playback.entries.length) return;

  const pad = 6;
  const span = width - pad * 2;
  const xAt = (i) => pad + playback.xs[i] * span;
  const headX = xAt(playback.step);

  const played = pen.createLinearGradient(pad, 0, pad + span, 0);
  played.addColorStop(0, "rgba(129, 91, 240, 0.30)");
  played.addColorStop(0.5, "rgba(72, 120, 224, 0.30)");
  played.addColorStop(1, "rgba(27, 175, 122, 0.30)");
  pen.fillStyle = played;
  pen.fillRect(pad, 4, Math.max(0, headX - pad), height - 8);

  for (let i = 0; i < playback.entries.length; i += 1) {
    pen.globalAlpha = i <= playback.step ? 0.95 : 0.35;
    pen.fillStyle = kindColor(playback.entries[i].kind);
    pen.fillRect(xAt(i) - 0.75, 7, 1.5, height - 14);
  }
  pen.globalAlpha = 1;
  pen.fillStyle = palette().accent;
  pen.fillRect(headX - 1, 2, 2, height - 4);
}

function scrubTo(offsetX) {
  const strip = $("tb-canvas");
  const width = strip.clientWidth || 1;
  const pad = 6;
  const x = Math.max(0, Math.min(1, (offsetX - pad) / (width - pad * 2)));
  let best = 0;
  let bestDist = Infinity;
  for (let i = 0; i < playback.xs.length; i += 1) {
    const dist = Math.abs(playback.xs[i] - x);
    if (dist < bestDist) {
      best = i;
      bestDist = dist;
    }
  }
  setReplayPlaying(false);
  setReplayStep(best);
}

$("btn-replay").addEventListener("click", () => {
  if (playback.active) stopReplay();
  else startReplay();
});
$("tb-toggle").addEventListener("click", () => setReplayPlaying(!playback.timer));
$("tb-close").addEventListener("click", stopReplay);
$("tb-now").addEventListener("click", stopReplay);
$("tb-prev").addEventListener("click", () => {
  setReplayPlaying(false);
  setReplayStep(playback.step - 1);
});
$("tb-next").addEventListener("click", () => {
  setReplayPlaying(false);
  setReplayStep(playback.step + 1);
});
$("tb-speed").addEventListener("change", (event) => {
  playback.stepMs = parseInt(event.target.value, 10) || 1200;
  if (playback.timer) setReplayPlaying(true);
});
let scrubbing = false;
$("tb-canvas").addEventListener("pointerdown", (event) => {
  scrubbing = true;
  $("tb-canvas").setPointerCapture(event.pointerId);
  scrubTo(event.offsetX);
});
$("tb-canvas").addEventListener("pointermove", (event) => {
  if (scrubbing) scrubTo(event.offsetX);
});
$("tb-canvas").addEventListener("pointerup", () => (scrubbing = false));
addEventListener("keydown", (event) => {
  if (!playback.active) return;
  const target = event.target;
  if (target && (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA")) {
    return;
  }
  if (event.key === " ") {
    event.preventDefault();
    setReplayPlaying(!playback.timer);
  } else if (event.key === "ArrowLeft") {
    setReplayPlaying(false);
    setReplayStep(playback.step - 1);
  } else if (event.key === "ArrowRight") {
    setReplayPlaying(false);
    setReplayStep(playback.step + 1);
  }
});

/* ---------------- hit testing ---------------- */

function nodeAt(sx, sy) {
  const w = toWorld(sx, sy);
  // Generous early-out: wider than the biggest meta plus the hit slack.
  const reach = 64 + 8 / camera.k;
  let best = null;
  let bestDist = Infinity;
  for (const body of scene.bodies) {
    if (Math.abs(body.x - w.x) > reach || Math.abs(body.y - w.y) > reach) continue;
    const dist = Math.hypot(body.x - w.x, body.y - w.y);
    const hitRadius = nodeRadius(body) + 5 / camera.k;
    if (dist < hitRadius && dist < bestDist) {
      best = body;
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
  for (const edge of scene.edges) {
    const s = scene.byId.get(edge.source);
    const t = scene.byId.get(edge.target);
    if (!s || !t || s === t) continue;
    if (
      w.x < Math.min(s.x, t.x) - threshold ||
      w.x > Math.max(s.x, t.x) + threshold ||
      w.y < Math.min(s.y, t.y) - threshold ||
      w.y > Math.max(s.y, t.y) + threshold
    ) {
      continue;
    }
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
    if (alpha < KMP_CORE.SIM.REHEAT_DRAG) alpha = KMP_CORE.SIM.REHEAT_DRAG;
    requestDraw();
    return;
  }
  if (panStart) {
    moved = true;
    camTween = null;
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
    if (draggedNode) {
      if (draggedNode.meta) selectMeta(draggedNode);
      else selectNode(draggedNode.id);
    } else {
      selectedId = null;
      renderDetailEmpty();
      requestDraw();
    }
  }
});

canvas.addEventListener("dblclick", (event) => {
  const body = nodeAt(event.offsetX, event.offsetY);
  if (!body) return;
  if (body.meta) expandCluster(body.cluster.id);
  else expandNode(body.id).catch((error) => showError(error.message));
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
    camTween = null;
    const factor = Math.exp(-event.deltaY * 0.0012);
    const before = toWorld(event.offsetX, event.offsetY);
    camera.k = Math.min(8, Math.max(0.05, camera.k * factor));
    const after = toWorld(event.offsetX, event.offsetY);
    camera.x += before.x - after.x;
    camera.y += before.y - after.y;
    requestDraw();
  },
  { passive: false }
);

/* `F` frames the whole graph — the escape hatch from any lost camera. */
addEventListener("keydown", (event) => {
  if (event.key !== "f" && event.key !== "F") return;
  const target = event.target;
  if (target && (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA")) {
    return;
  }
  fitToView();
});

function centerOn(id) {
  const body = scene.byId.get(id) || proxyOf(id);
  if (!body) return;
  animateCamera({ x: body.x, y: body.y, k: camera.k < 0.7 ? 0.9 : camera.k });
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
    if (node.meta) {
      tooltip.append(el("div", "tt-title", node.title));
      tooltip.append(
        el("div", "tt-sub", `folded dimension · ${node.mass} inside — double-click to expand`)
      );
      tooltip.hidden = false;
      positionTooltip(sx, sy);
      return;
    }
    tooltip.append(el("div", "tt-title", node.title));
    tooltip.append(el("div", "tt-sub", `${node.kind}${node.status ? " · " + node.status : ""}`));
    if (node.summary) tooltip.append(el("div", "tt-quote", node.summary));
  } else {
    const edge = hover.ref;
    tooltip.append(el("div", "tt-title", `${edge.rel} (${edge.class})`));
    tooltip.append(el("div", "tt-sub", `${edge.source} → ${edge.target}`));
    if (edge.why) tooltip.append(el("div", "tt-quote", edge.why));
    if (edge.motivation) tooltip.append(el("div", "tt-quote", `motivation: ${edge.motivation}`));
    if (edge.method) tooltip.append(el("div", "tt-sub", `method: ${edge.method}`));
    if (edge.evidence) tooltip.append(el("div", "tt-sub", `evidence: ${edge.evidence}`));
    if (edge.confidence) tooltip.append(el("div", "tt-sub", `confidence: ${edge.confidence}`));
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
  // Relation classes, now that edges wear them: a key, not a filter.
  const classes = new Map();
  for (const edge of graph.edges) {
    classes.set(edge.class, (classes.get(edge.class) || 0) + 1);
  }
  for (const [cls, count] of [...classes.entries()].sort((a, b) => b[1] - a[1])) {
    const item = el("li", "");
    const dash = el("span", "legend-dash");
    dash.style.background = palette().edgeClass[cls] || palette().edge;
    item.append(dash, el("span", "", `${cls} `), el("span", "muted", String(count)));
    item.title = "relation class";
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

/* ---------------- folded dimensions: card and sidebar ---------------- */

function selectMeta(meta) {
  selectedId = meta.id;
  renderClusterCard(meta);
  requestDraw();
}

function renderClusterCard(meta) {
  const cluster = meta.cluster;
  $("detail-empty").hidden = true;
  $("context-body").hidden = true;
  $("detail-body").hidden = false;

  const kindPill = $("d-kind");
  kindPill.textContent = "";
  const dot = el("span", "legend-dot");
  dot.style.background = kindColor("memory_dimension");
  kindPill.append(dot, document.createTextNode("folded dimension"));
  $("d-status").textContent = `${meta.mass} inside`;
  $("d-title").textContent = cluster.label;
  $("d-id").textContent = cluster.head || cluster.id;
  $("d-summary").textContent = "One mark holding a whole dimension. Expand it to walk inside.";

  const labels = $("d-labels");
  labels.textContent = "";
  for (const [kind, count] of [...cluster.kindCounts.entries()].sort((a, b) => b[1] - a[1])) {
    const pill = el("span", "pill pill-muted");
    const kindDot = el("span", "legend-dot");
    kindDot.style.background = kindColor(kind);
    pill.append(kindDot, document.createTextNode(`${kind} ${count}`));
    labels.append(pill);
  }
  const expand = el("button", "btn btn-primary", "Expand");
  expand.addEventListener("click", () => expandCluster(cluster.id));
  labels.append(expand);

  $("d-detail").textContent =
    cluster.members.slice(0, 30).join("\n") +
    (cluster.members.length > 30 ? `\n… ${cluster.members.length - 30} more` : "");
  $("d-props-body").textContent = "";
  $("d-coords").textContent = "";
  renderRelationList($("d-incoming"), [], "source");
  renderRelationList($("d-outgoing"), [], "target");
}

function renderDimensions() {
  const list = $("dim-list");
  if (!list) return;
  list.textContent = "";
  const entries = [...clusterState.clusters.values()]
    .map((cluster) => ({ cluster, size: cluster.members.length + (cluster.head ? 1 : 0) }))
    .filter((entry) => entry.size > 0)
    .sort((a, b) => b.size - a.size);
  for (const { cluster, size } of entries) {
    const folded = clusterState.collapsed.has(cluster.id);
    const item = el("li", "");
    const dot = el("span", "legend-dot");
    dot.style.background = kindColor("memory_dimension");
    dot.style.opacity = folded ? "1" : "0.35";
    item.append(
      dot,
      el("span", "", `${folded ? "▸" : "▾"} ${cluster.label} `),
      el("span", "muted", String(size))
    );
    item.title = folded ? "folded — click to expand" : "expanded — click to fold";
    item.addEventListener("click", () => {
      if (folded) expandCluster(cluster.id);
      else collapseCluster(cluster.id);
    });
    list.append(item);
  }
}

async function selectNode(id) {
  // Selecting a folded member unfolds its dimension first — a click from the
  // timeline or the search must always land on something visible.
  const clusterId = clusterState.of.get(id);
  if (clusterId && clusterState.collapsed.has(clusterId)) expandCluster(clusterId);
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
    if (relation.motivation) item.append(el("p", "rel-why", `motivation: ${relation.motivation}`));
    if (relation.method) item.append(el("p", "rel-evidence", `method: ${relation.method}`));
    if (relation.evidence) item.append(el("p", "rel-evidence", `evidence: ${relation.evidence}`));
    const anchors = el("p", "rel-evidence");
    if (relation.decision_id) {
      anchors.append(document.createTextNode("decision: "), nodeLink(relation.decision_id));
    }
    if (relation.caused_by) {
      if (anchors.childNodes.length) anchors.append(document.createTextNode(" · "));
      anchors.append(document.createTextNode("caused by: "), nodeLink(relation.caused_by));
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
    rebuildClusters(true);
    renderAbouts();
    renderLegend();
    renderStats(view);
    renderDetailEmpty();
    selectedId = null;
    camera.x = 0;
    camera.y = 0;
    camera.k = 0.6;
    settleFitPending = true;
    reheat(KMP_CORE.SIM.REHEAT_LOAD);
    showError("");
  } catch (error) {
    showError(error.message);
  }
}

$("btn-fit").addEventListener("click", () => fitToView());
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
      const asOf = el("button", "btn tl-asof", "as of here");
      asOf.title = "open the time strip at this instant";
      asOf.addEventListener("click", (click) => {
        click.stopPropagation();
        document.querySelector('.tab[data-tab="graph"]').click();
        if (playback.active && playback.stepOf.has(entry.ref_id)) {
          setReplayPlaying(false);
          setReplayStep(playback.stepOf.get(entry.ref_id));
        } else {
          startReplay(entry.ref_id);
        }
      });
      meta.append(asOf);
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
    rebuildClusters(false);
    // The path must be visible: unfold any dimension holding one of its nodes.
    for (const nodeView of view.nodes) {
      const clusterId = clusterState.of.get(nodeView.id);
      if (clusterId && clusterState.collapsed.has(clusterId)) expandCluster(clusterId);
    }
    reheat(KMP_CORE.SIM.REHEAT_EXPAND);
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
