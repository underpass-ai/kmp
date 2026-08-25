const DATA = window.MOBILE_LOGIN_REPLAY;

const GRAPH = {
  width: 1240,
  height: 700,
  nodeWidth: 156,
  nodeHeight: 66,
};

const byNodeId = new Map(DATA.nodes.map((node) => [node.id, node]));
const byEdgeId = new Map(DATA.edges.map((edge) => [edge.id, edge]));
const bySessionId = new Map(DATA.sessions.map((session) => [session.id, session]));
const byViewId = new Map(DATA.views.map((view) => [view.id, view]));

const state = {
  viewId: initialViewId(),
  enabledSessions: new Set(DATA.sessions.map((session) => session.id)),
  selection: { type: "node", id: "hypothesis" },
  viewBox: initialViewBox(),
  drag: null,
};

const els = {};

document.addEventListener("DOMContentLoaded", () => {
  els.viewButtons = document.querySelector("#view-buttons");
  els.filters = document.querySelector("#session-filters");
  els.graph = document.querySelector("#incident-graph");
  els.inspector = document.querySelector("#inspector");
  els.timeline = document.querySelector("#timeline");
  els.metrics = document.querySelector("#metrics");
  els.runMeta = document.querySelector("#run-meta");

  renderStaticMeta();
  renderViewButtons();
  renderFilters();
  selectPreferredForView();
  renderAll();
  bindInteractions();
});

function renderStaticMeta() {
  els.runMeta.textContent = `${DATA.metadata.runId} | ${DATA.metadata.source}`;
}

function renderViewButtons() {
  els.viewButtons.innerHTML = "";
  for (const view of DATA.views) {
    const button = document.createElement("button");
    button.className = "view-button";
    button.type = "button";
    button.dataset.view = view.id;
    button.textContent = view.label;
    button.title = view.result;
    if (view.id === state.viewId) {
      button.classList.add("is-active");
    }
    els.viewButtons.append(button);
  }
}

function renderFilters() {
  const activeView = getActiveView();
  const viewSessions = new Set(activeView.abouts);
  els.filters.innerHTML = "";

  for (const session of DATA.sessions) {
    const button = document.createElement("button");
    button.className = "filter-button";
    button.type = "button";
    button.dataset.session = session.id;
    button.style.setProperty("--session-color", session.color);
    button.textContent = session.label;

    if (state.enabledSessions.has(session.id)) {
      button.classList.add("is-active");
    }
    if (!viewSessions.has(session.id)) {
      button.classList.add("is-muted");
    }

    els.filters.append(button);
  }
}

function renderAll() {
  repairSelection();
  renderMetrics();
  renderGraph();
  renderTimeline();
  renderInspector();
}

function renderMetrics() {
  const view = getActiveView();
  const visibleNodes = getVisibleNodes();
  const visibleEdges = getVisibleEdges(visibleNodes);

  els.metrics.innerHTML = "";
  els.metrics.append(
    metric("View", view.result),
    metric("Nodes", String(visibleNodes.length)),
    metric("Edges", String(visibleEdges.length)),
    metric("Inspect", inspectLabel()),
  );
}

function metric(label, value) {
  const node = document.createElement("div");
  node.className = "metric";
  node.innerHTML = `<span>${escapeHtml(label)}</span><strong>${escapeHtml(value)}</strong>`;
  return node;
}

function inspectLabel() {
  if (state.selection.type === "edge") {
    const edge = byEdgeId.get(state.selection.id);
    return edge ? edge.rel : "none";
  }
  const node = byNodeId.get(state.selection.id);
  return node ? node.session : "none";
}

function renderGraph() {
  const svg = els.graph;
  svg.innerHTML = "";
  applyViewBox();

  const visibleNodes = getVisibleNodes();
  const visibleNodeIds = new Set(visibleNodes.map((node) => node.id));
  const visibleEdges = getVisibleEdges(visibleNodes);
  const visibleEdgeIds = new Set(visibleEdges.map((edge) => edge.id));
  const highlight = getHighlightSets();

  svg.append(renderDefs());
  svg.append(renderLanes());

  const edgesLayer = svgEl("g", "edges");
  for (const edge of visibleEdges) {
    edgesLayer.append(renderEdge(edge, highlight, visibleEdgeIds));
  }
  svg.append(edgesLayer);

  const nodesLayer = svgEl("g", "nodes");
  for (const node of visibleNodes) {
    nodesLayer.append(renderNode(node, highlight, visibleNodeIds));
  }
  svg.append(nodesLayer);
}

function renderDefs() {
  const defs = svgEl("defs");
  const marker = svgEl("marker");
  marker.setAttribute("id", "arrow");
  marker.setAttribute("viewBox", "0 0 10 10");
  marker.setAttribute("refX", "9");
  marker.setAttribute("refY", "5");
  marker.setAttribute("markerWidth", "6");
  marker.setAttribute("markerHeight", "6");
  marker.setAttribute("orient", "auto-start-reverse");

  const path = svgEl("path");
  path.setAttribute("d", "M 0 0 L 10 5 L 0 10 z");
  path.setAttribute("class", "arrow-head");
  marker.append(path);
  defs.append(marker);
  return defs;
}

function renderLanes() {
  const group = svgEl("g", "lanes");
  const activeView = getActiveView();
  const viewSessions = new Set(activeView.abouts);

  for (const session of DATA.sessions) {
    if (!viewSessions.has(session.id)) {
      continue;
    }

    const lane = svgEl("g", "lane");
    if (!state.enabledSessions.has(session.id)) {
      lane.classList.add("is-muted");
    }

    const line = svgEl("line", "lane-line");
    line.setAttribute("x1", "24");
    line.setAttribute("x2", String(GRAPH.width - 28));
    line.setAttribute("y1", String(session.y));
    line.setAttribute("y2", String(session.y));
    line.style.setProperty("--session-color", session.color);

    const label = svgEl("text", "lane-label");
    label.setAttribute("x", "28");
    label.setAttribute("y", String(session.y - 42));
    label.textContent = session.label;

    lane.append(line, label);
    group.append(lane);
  }
  return group;
}

function renderEdge(edge, highlight) {
  const from = byNodeId.get(edge.from);
  const to = byNodeId.get(edge.to);
  const group = svgEl("g", `edge edge-${edge.class}`);
  const selected = state.selection.type === "edge" && state.selection.id === edge.id;

  if (selected) {
    group.classList.add("is-selected");
  }
  if (highlight.active && !highlight.edges.has(edge.id)) {
    group.classList.add("is-dimmed");
  }
  if (highlight.edges.has(edge.id)) {
    group.classList.add("is-highlighted");
  }

  group.dataset.edge = edge.id;
  group.setAttribute("tabindex", "0");

  const path = svgEl("path", "edge-path");
  path.setAttribute("d", edgePath(from, to));
  path.setAttribute("marker-end", "url(#arrow)");

  const labelPoint = edgeLabelPoint(from, to);
  const label = svgEl("text", "edge-label");
  label.setAttribute("x", String(labelPoint.x));
  label.setAttribute("y", String(labelPoint.y));
  label.textContent = edge.rel;

  const title = svgEl("title");
  title.textContent = `${edge.rel}: ${edge.why}`;

  group.append(title, path, label);
  return group;
}

function renderNode(node, highlight) {
  const session = bySessionId.get(node.session);
  const group = svgEl("g", `node node-${node.status}`);
  const selected = state.selection.type === "node" && state.selection.id === node.id;

  group.dataset.node = node.id;
  group.setAttribute("tabindex", "0");
  group.setAttribute(
    "transform",
    `translate(${node.x - GRAPH.nodeWidth / 2}, ${node.y - GRAPH.nodeHeight / 2})`,
  );
  group.style.setProperty("--session-color", session.color);

  if (selected) {
    group.classList.add("is-selected");
  }
  if (highlight.active && !highlight.nodes.has(node.id)) {
    group.classList.add("is-dimmed");
  }
  if (highlight.nodes.has(node.id)) {
    group.classList.add("is-highlighted");
  }

  const rect = svgEl("rect", "node-box");
  rect.setAttribute("width", String(GRAPH.nodeWidth));
  rect.setAttribute("height", String(GRAPH.nodeHeight));
  rect.setAttribute("rx", "8");

  const stripe = svgEl("rect", "node-stripe");
  stripe.setAttribute("width", "6");
  stripe.setAttribute("height", String(GRAPH.nodeHeight));
  stripe.setAttribute("rx", "3");

  const time = svgEl("text", "node-time");
  time.setAttribute("x", "16");
  time.setAttribute("y", "20");
  time.textContent = node.time;

  const status = svgEl("text", "node-kind");
  status.setAttribute("x", String(GRAPH.nodeWidth - 12));
  status.setAttribute("y", "20");
  status.textContent = node.status;

  const title = svgEl("text", "node-title");
  title.setAttribute("x", "16");
  title.setAttribute("y", "40");
  for (const line of wrapText(node.title, 21, 2)) {
    const tspan = svgEl("tspan");
    tspan.setAttribute("x", "16");
    tspan.setAttribute("dy", title.childNodes.length === 0 ? "0" : "15");
    tspan.textContent = line;
    title.append(tspan);
  }

  const tooltip = svgEl("title");
  tooltip.textContent = node.text;

  group.append(tooltip, rect, stripe, time, status, title);
  return group;
}

function renderTimeline() {
  const visibleNodes = getVisibleNodes().slice().sort(compareTimeline);
  els.timeline.innerHTML = "";

  for (const node of visibleNodes) {
    const session = bySessionId.get(node.session);
    const item = document.createElement("button");
    item.className = "timeline-item";
    item.type = "button";
    item.dataset.node = node.id;
    item.style.setProperty("--session-color", session.color);
    if (state.selection.type === "node" && state.selection.id === node.id) {
      item.classList.add("is-active");
    }

    item.innerHTML = `
      <span class="timeline-time">${escapeHtml(node.time)}</span>
      <span class="timeline-title">${escapeHtml(node.title)}</span>
      <span class="timeline-agent">${escapeHtml(session.label)}</span>
    `;
    els.timeline.append(item);
  }
}

function renderInspector() {
  if (state.selection.type === "edge") {
    renderEdgeInspector();
    return;
  }
  renderNodeInspector();
}

function renderNodeInspector() {
  const node = byNodeId.get(state.selection.id);
  const session = bySessionId.get(node.session);
  const incoming = DATA.edges.filter((edge) => edge.to === node.id);
  const outgoing = DATA.edges.filter((edge) => edge.from === node.id);

  els.inspector.innerHTML = `
    <div class="inspector-header" style="--session-color: ${escapeAttr(session.color)}">
      <span>${escapeHtml(session.label)}</span>
      <strong>${escapeHtml(node.time)}</strong>
    </div>
    <h2>${escapeHtml(node.title)}</h2>
    <p class="object-ref">${escapeHtml(node.ref)}</p>
    <p>${escapeHtml(node.text)}</p>
    <dl class="detail-grid">
      <div><dt>Kind</dt><dd>${escapeHtml(node.kind)}</dd></div>
      <div><dt>Status</dt><dd>${escapeHtml(node.status)}</dd></div>
      <div><dt>Sequence</dt><dd>${escapeHtml(String(node.sequence))}</dd></div>
      <div><dt>About</dt><dd>${escapeHtml(session.agent)}</dd></div>
    </dl>
    ${renderEvidenceList(node.evidence)}
    ${renderEdgeList("Incoming", incoming)}
    ${renderEdgeList("Outgoing", outgoing)}
  `;
}

function renderEdgeInspector() {
  const edge = byEdgeId.get(state.selection.id);
  const from = byNodeId.get(edge.from);
  const to = byNodeId.get(edge.to);

  els.inspector.innerHTML = `
    <div class="inspector-header">
      <span>${escapeHtml(edge.class)}</span>
      <strong>${escapeHtml(edge.confidence)}</strong>
    </div>
    <h2>${escapeHtml(edge.rel)}</h2>
    <p>${escapeHtml(edge.why)}</p>
    <dl class="detail-grid">
      <div><dt>From</dt><dd><button class="link-button" data-node="${escapeAttr(from.id)}">${escapeHtml(from.title)}</button></dd></div>
      <div><dt>To</dt><dd><button class="link-button" data-node="${escapeAttr(to.id)}">${escapeHtml(to.title)}</button></dd></div>
      <div><dt>Class</dt><dd>${escapeHtml(edge.class)}</dd></div>
      <div><dt>Path</dt><dd>${escapeHtml(edge.path)}</dd></div>
    </dl>
  `;
}

function renderEvidenceList(evidence) {
  if (!evidence || evidence.length === 0) {
    return "";
  }
  return `
    <section class="inspector-section">
      <h3>Evidence</h3>
      <ul>
        ${evidence.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}
      </ul>
    </section>
  `;
}

function renderEdgeList(title, edges) {
  if (edges.length === 0) {
    return `
      <section class="inspector-section">
        <h3>${escapeHtml(title)}</h3>
        <p class="muted">none</p>
      </section>
    `;
  }

  return `
    <section class="inspector-section">
      <h3>${escapeHtml(title)}</h3>
      <div class="edge-list">
        ${edges
          .map((edge) => {
            const other = byNodeId.get(title === "Incoming" ? edge.from : edge.to);
            return `
              <button type="button" data-edge="${escapeAttr(edge.id)}">
                <span>${escapeHtml(edge.rel)}</span>
                <strong>${escapeHtml(other.title)}</strong>
              </button>
            `;
          })
          .join("")}
      </div>
    </section>
  `;
}

function bindInteractions() {
  els.viewButtons.addEventListener("click", (event) => {
    const button = event.target.closest("[data-view]");
    if (!button) {
      return;
    }
    state.viewId = button.dataset.view;
    writeViewHash(state.viewId);
    selectPreferredForView();
    renderViewButtons();
    renderFilters();
    renderAll();
  });

  els.filters.addEventListener("click", (event) => {
    const button = event.target.closest("[data-session]");
    if (!button) {
      return;
    }
    const session = button.dataset.session;
    const next = new Set(state.enabledSessions);
    if (next.has(session)) {
      next.delete(session);
    } else {
      next.add(session);
    }
    if (getVisibleNodes(next).length === 0) {
      return;
    }
    state.enabledSessions = next;
    renderFilters();
    renderAll();
  });

  els.graph.addEventListener("click", (event) => {
    const node = event.target.closest("[data-node]");
    const edge = event.target.closest("[data-edge]");
    if (node) {
      state.selection = { type: "node", id: node.dataset.node };
      renderAll();
      return;
    }
    if (edge) {
      state.selection = { type: "edge", id: edge.dataset.edge };
      renderAll();
    }
  });

  els.graph.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    const node = event.target.closest("[data-node]");
    const edge = event.target.closest("[data-edge]");
    if (node) {
      state.selection = { type: "node", id: node.dataset.node };
      renderAll();
    } else if (edge) {
      state.selection = { type: "edge", id: edge.dataset.edge };
      renderAll();
    }
  });

  els.timeline.addEventListener("click", (event) => {
    const item = event.target.closest("[data-node]");
    if (!item) {
      return;
    }
    state.selection = { type: "node", id: item.dataset.node };
    renderAll();
  });

  els.inspector.addEventListener("click", (event) => {
    const node = event.target.closest("[data-node]");
    const edge = event.target.closest("[data-edge]");
    if (node) {
      state.selection = { type: "node", id: node.dataset.node };
      renderAll();
    } else if (edge) {
      state.selection = { type: "edge", id: edge.dataset.edge };
      renderAll();
    }
  });

  document.querySelector("[data-action='zoom-in']").addEventListener("click", () => zoomAtCenter(0.82));
  document.querySelector("[data-action='zoom-out']").addEventListener("click", () => zoomAtCenter(1.18));
  document.querySelector("[data-action='fit']").addEventListener("click", () => {
    state.viewBox = { x: 0, y: 20, width: GRAPH.width, height: 660 };
    applyViewBox();
  });

  els.graph.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      zoomAt(event.deltaY < 0 ? 0.88 : 1.12, clientToSvg(event.clientX, event.clientY));
    },
    { passive: false },
  );

  els.graph.addEventListener("pointerdown", (event) => {
    if (event.target.closest("[data-node]") || event.target.closest("[data-edge]")) {
      return;
    }
    state.drag = {
      x: event.clientX,
      y: event.clientY,
      viewBox: { ...state.viewBox },
      pointerId: event.pointerId,
    };
    els.graph.setPointerCapture(event.pointerId);
  });

  els.graph.addEventListener("pointermove", (event) => {
    if (!state.drag) {
      return;
    }
    const rect = els.graph.getBoundingClientRect();
    const dx = ((event.clientX - state.drag.x) / rect.width) * state.drag.viewBox.width;
    const dy = ((event.clientY - state.drag.y) / rect.height) * state.drag.viewBox.height;
    state.viewBox.x = state.drag.viewBox.x - dx;
    state.viewBox.y = state.drag.viewBox.y - dy;
    applyViewBox();
  });

  els.graph.addEventListener("pointerup", endDrag);
  els.graph.addEventListener("pointercancel", endDrag);

  window.addEventListener("hashchange", () => {
    const nextView = initialViewId();
    if (nextView === state.viewId) {
      return;
    }
    state.viewId = nextView;
    selectPreferredForView();
    renderViewButtons();
    renderFilters();
    renderAll();
  });
}

function getActiveView() {
  return byViewId.get(state.viewId);
}

function initialViewId() {
  const value = window.location.hash.slice(1).trim();
  return byViewId.has(value) ? value : "core";
}

function initialViewBox() {
  if (window.innerWidth <= 680) {
    return { x: 0, y: 58, width: 760, height: 580 };
  }
  if (window.innerWidth <= 1040) {
    return { x: 0, y: 30, width: 980, height: 660 };
  }
  return { x: 0, y: 20, width: GRAPH.width, height: 660 };
}

function writeViewHash(viewId) {
  const next = `${window.location.pathname}${window.location.search}#${viewId}`;
  window.history.replaceState(null, "", next);
}

function getVisibleNodes(enabledSessions = state.enabledSessions) {
  const activeView = getActiveView();
  const viewSessions = new Set(activeView.abouts);
  return DATA.nodes.filter((node) => viewSessions.has(node.session) && enabledSessions.has(node.session));
}

function getVisibleEdges(visibleNodes = getVisibleNodes()) {
  const visibleNodeIds = new Set(visibleNodes.map((node) => node.id));
  return DATA.edges.filter((edge) => visibleNodeIds.has(edge.from) && visibleNodeIds.has(edge.to));
}

function selectPreferredForView() {
  const visibleNodes = getVisibleNodes();
  const highlight = getHighlightSets();
  const preferred = visibleNodes.find((node) => highlight.nodes.has(node.id));
  if (preferred) {
    state.selection = { type: "node", id: preferred.id };
  }
}

function getHighlightSets() {
  const activeView = getActiveView();
  const nodes = new Set(activeView.highlightNodes);
  const edges = new Set(activeView.highlightEdges);
  for (const edgeId of edges) {
    const edge = byEdgeId.get(edgeId);
    if (edge) {
      nodes.add(edge.from);
      nodes.add(edge.to);
    }
  }
  return { active: nodes.size > 0 || edges.size > 0, nodes, edges };
}

function repairSelection() {
  const visibleNodes = getVisibleNodes();
  const visibleNodeIds = new Set(visibleNodes.map((node) => node.id));
  const visibleEdges = getVisibleEdges(visibleNodes);
  const visibleEdgeIds = new Set(visibleEdges.map((edge) => edge.id));

  if (state.selection.type === "node" && visibleNodeIds.has(state.selection.id)) {
    return;
  }
  if (state.selection.type === "edge" && visibleEdgeIds.has(state.selection.id)) {
    return;
  }

  const highlight = getHighlightSets();
  const preferred = visibleNodes.find((node) => highlight.nodes.has(node.id)) || visibleNodes[0];
  if (preferred) {
    state.selection = { type: "node", id: preferred.id };
  }
}

function compareTimeline(left, right) {
  if (left.time === right.time) {
    return left.sequence - right.sequence;
  }
  return left.time.localeCompare(right.time);
}

function edgePath(from, to) {
  const sx = from.x;
  const sy = from.y;
  const tx = to.x;
  const ty = to.y;
  const direction = tx >= sx ? 1 : -1;
  const curve = Math.max(90, Math.abs(tx - sx) * 0.52);
  return `M ${sx} ${sy} C ${sx + curve * direction} ${sy}, ${tx - curve * direction} ${ty}, ${tx} ${ty}`;
}

function edgeLabelPoint(from, to) {
  return {
    x: (from.x + to.x) / 2,
    y: (from.y + to.y) / 2 - 8,
  };
}

function zoomAtCenter(factor) {
  zoomAt(factor, {
    x: state.viewBox.x + state.viewBox.width / 2,
    y: state.viewBox.y + state.viewBox.height / 2,
  });
}

function zoomAt(factor, point) {
  const nextWidth = clamp(state.viewBox.width * factor, 430, GRAPH.width * 1.25);
  const nextHeight = clamp(state.viewBox.height * factor, 260, GRAPH.height * 1.25);
  const rx = (point.x - state.viewBox.x) / state.viewBox.width;
  const ry = (point.y - state.viewBox.y) / state.viewBox.height;
  state.viewBox.x = point.x - nextWidth * rx;
  state.viewBox.y = point.y - nextHeight * ry;
  state.viewBox.width = nextWidth;
  state.viewBox.height = nextHeight;
  applyViewBox();
}

function clientToSvg(clientX, clientY) {
  const rect = els.graph.getBoundingClientRect();
  return {
    x: state.viewBox.x + ((clientX - rect.left) / rect.width) * state.viewBox.width,
    y: state.viewBox.y + ((clientY - rect.top) / rect.height) * state.viewBox.height,
  };
}

function endDrag(event) {
  if (!state.drag) {
    return;
  }
  if (event && event.pointerId === state.drag.pointerId) {
    els.graph.releasePointerCapture(event.pointerId);
  }
  state.drag = null;
}

function applyViewBox() {
  els.graph.setAttribute(
    "viewBox",
    `${state.viewBox.x} ${state.viewBox.y} ${state.viewBox.width} ${state.viewBox.height}`,
  );
}

function wrapText(text, maxChars, maxLines) {
  const words = text.split(/\s+/);
  const lines = [];
  let current = "";

  for (const word of words) {
    const next = current ? `${current} ${word}` : word;
    if (next.length <= maxChars) {
      current = next;
      continue;
    }
    if (current) {
      lines.push(current);
    }
    current = word;
    if (lines.length === maxLines - 1) {
      break;
    }
  }

  if (current && lines.length < maxLines) {
    lines.push(current);
  }

  if (words.join(" ").length > lines.join(" ").length && lines.length > 0) {
    lines[lines.length - 1] = `${lines[lines.length - 1].replace(/\.*$/, "")}...`;
  }

  return lines;
}

function svgEl(name, className) {
  const node = document.createElementNS("http://www.w3.org/2000/svg", name);
  if (className) {
    node.setAttribute("class", className);
  }
  return node;
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function escapeAttr(value) {
  return escapeHtml(value);
}
