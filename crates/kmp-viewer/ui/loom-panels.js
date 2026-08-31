/* KMP ChronoLoom — the DOM panels.
   Everything rendered in HTML rather than on the stage: the about list, the
   lane and legend rails, the detail and prism panes, the trace hops, the
   diff panel, the provenance chip, the stats line and the search results —
   plus the tiny DOM kit the other adapters share (KMP_APP.dom).
   Exposes KMP_APP.panels. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

/* ---------------- the shared DOM kit ---------------- */

KMP_APP.dom = (() => {
  const $ = (id) => document.getElementById(id);

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

  const fmtMs = (ms) => new Date(ms).toISOString().slice(5, 16).replace("T", " ");
  const fmtMsFull = (ms) => new Date(ms).toISOString().slice(0, 19).replace("T", " ");

  return { $, showError, el, fmtMs, fmtMsFull };
})();

KMP_APP.panels = (() => {
  const { model, view, tracePick } = KMP_APP.state;
  const { $, el, fmtMs, fmtMsFull } = KMP_APP.dom;
  const kindColor = (kind) => KMP_APP.scene.kindColor(kind);
  const classColor = (cls) => KMP_APP.scene.classColor(cls);

  /* ---------------- abouts, lanes, legends ---------------- */

  function renderAbouts() {
    const list = $("about-list");
    list.textContent = "";
    for (const about of model.abouts) {
      const item = el("li", about === model.about ? "active" : "", about);
      item.addEventListener("click", () => KMP_APP.data.loadAbout(about));
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
        KMP_APP.scene.requestDraw();
      });
      lanes.append(item);
    }

    const kinds = new Map();
    const projectedKinds = Object.entries((model.projection && model.projection.by_kind) || {});
    if (projectedKinds.length) {
      for (const [kind, count] of projectedKinds) kinds.set(kind, Number(count));
    } else if (model.entries.length) {
      for (const m of model.entries) kinds.set(m.kind, (kinds.get(m.kind) || 0) + 1);
    }
    const kindList = $("kind-legend");
    kindList.textContent = "";
    if (!kinds.size && model.currentLod !== "moment") {
      kindList.append(el("li", "legend-static muted", "kind counts available at Moment"));
    }
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
        KMP_APP.scene.requestDraw();
      });
      kindList.append(item);
    }

    const classes = new Map();
    for (const edge of model.edges) classes.set(edge.class, (classes.get(edge.class) || 0) + 1);
    const classList = $("class-legend");
    classList.textContent = "";
    if (model.currentLod !== "moment") {
      const unavailable = el("li", "legend-static muted", "relations available at Moment");
      classList.append(unavailable);
      return;
    }
    const DASH_CLASS = { evidential: "dashed", motivational: "dotted", constraint: "dashed", procedural: "dashed" };
    for (const [cls, count] of [...classes.entries()].sort((a, b) => b[1] - a[1])) {
      const item = el("li", "");
      const dash = el("span", `legend-dash ${DASH_CLASS[cls] || ""}`);
      dash.style.borderTopColor = classColor(cls);
      item.append(dash, el("span", "", cls), el("span", "muted legend-count", String(count)));
      classList.append(item);
    }
    const specialRelations = [
      {
        count: model.supersessions.length,
        label: "superseded — history, still true then",
        swatch: "superseded dotted",
        color: KMP_APP.scene.palette().textMuted,
      },
      {
        count: model.contradictions.length,
        label: "contradicts — both cannot hold now",
        swatch: "contradiction dashed",
        color: KMP_APP.scene.palette().danger,
      },
    ];
    for (const relation of specialRelations) {
      if (!relation.count) continue;
      const item = el("li", "legend-static");
      const dash = el("span", `legend-dash ${relation.swatch}`);
      dash.style.borderTopColor = relation.color;
      item.append(
        dash,
        el("span", "legend-description", relation.label),
        el("span", "muted legend-count", String(relation.count))
      );
      classList.append(item);
    }
  }

  /* ---------------- status line ---------------- */

  function renderStats() {
    $("s-entries").textContent = String(model.total);
    $("s-lanes").textContent = String(model.lanes.length);
    $("s-relations").textContent =
      model.currentLod === "moment"
        ? String(model.edges.length + model.supersessions.length + model.contradictions.length)
        : "—";
    const clocked = model.entries.filter((m) => KMP_LOOM.strictMs(m, view.clock) !== null).length;
    $("s-clocked").textContent =
      model.currentLod === "moment" ? `${clocked}/${model.total}` : `—/${model.total}`;
    $("s-window").textContent = view.full ? `${fmtMs(view.t0)} → ${fmtMs(view.t1)}` : "—";
  }

  function syncClockChips(clock) {
    for (const chip of document.querySelectorAll("#clock-chips .chip")) {
      chip.classList.toggle("active", chip.dataset.clock === clock);
    }
  }

  function syncFocusButton() {
    $("focus-context").textContent = view.focusRange ? "Clear focus" : "Focus + context";
  }

  /* ---------------- pulse legend ---------------- */

  function renderPulseLegend(atMillis = null) {
    const legend = $("pulse-legend");
    const series = model.observability.series || [];
    legend.textContent = "";
    if (!view.overlays.length || !series.length) {
      legend.hidden = true;
      return;
    }
    const colors = KMP_APP.scene.pulseColors();
    series.forEach((item, index) => {
      const points = (item.points || []).filter((point) =>
        Number.isFinite(Number(point.at_millis))
      );
      const current = points.length
        ? points.reduce((best, point) => {
            if (best === null) return point;
            if (atMillis === null) {
              return Number(point.at_millis) > Number(best.at_millis) ? point : best;
            }
            return Math.abs(Number(point.at_millis) - atMillis) <
              Math.abs(Number(best.at_millis) - atMillis)
              ? point
              : best;
          }, null)
        : null;
      const key = el("span", "pulse-key");
      const swatch = el("span", "pulse-swatch");
      swatch.style.background = colors[index % colors.length];
      key.append(swatch, el("span", "", item.name));
      const formatted = current
        ? KMP_LOOM.formatMetricValue(current.value, item.unit)
        : "—";
      key.append(
        el("span", "pulse-value", `${formatted} ${item.unit || ""}`.trim())
      );
      legend.append(key);
    });
    legend.hidden = false;
  }

  /* ---------------- diff panel ---------------- */

  function pinComparison(side) {
    const selected = view.selectedRef && model.byRef.get(view.selectedRef);
    const instant =
      (selected && KMP_LOOM.placedMs(selected, view.clock)) ?? (view.t0 + view.t1) / 2;
    const projection = KMP_LOOM.projectionAt(
      model.entries,
      model.proofEdges,
      view.clock,
      instant
    );
    if (side === "A") view.pinA = projection;
    else view.pinB = projection;
    view.diff = view.pinA && view.pinB ? KMP_LOOM.diffProjections(view.pinA, view.pinB) : null;
    renderDiffPanel();
    KMP_APP.scene.requestDraw();
  }

  function renderDiffPanel() {
    const panel = $("diff-panel");
    panel.textContent = "";
    panel.hidden = !view.pinA && !view.pinB;
    if (panel.hidden) return;
    const head = el("div", "diff-head");
    const side = (name, projection, className) => {
      const quantity = (count, singular, plural = `${singular}s`) =>
        `${count} ${count === 1 ? singular : plural}`;
      const node = el("div", `diff-side ${className || ""}`);
      node.append(
        el("strong", "", name),
        el("div", "mono", projection ? fmtMsFull(projection.instant) : "not pinned"),
        el(
          "div",
          "muted",
          projection
            ? `${quantity(projection.entries.length, "entry", "entries")} · ${quantity(projection.relations.length, "relation")}`
            : ""
        )
      );
      return node;
    };
    head.append(side("A", view.pinA, ""), side("B", view.pinB, "b"));
    panel.append(head);
    if (!view.diff) return;
    const grid = el("div", "diff-grid mono");
    grid.append(el("span", "", "facet"), el("span", "", "only A"), el("span", "", "only B"), el("span", "", "changed"));
    const compactSet = (values, label) => {
      if (!values.length) return el("span", "", "—");
      const cell = el("span", "diff-set");
      cell.append(el("strong", "", `${values.length} ${label}`));
      const sample = values.slice(0, 3).join(", ");
      cell.append(
        el(
          "small",
          "muted",
          values.length > 3 ? `${sample} · +${values.length - 3} more` : sample
        )
      );
      return cell;
    };
    for (const name of ["entries", "relations", "validity", "evidence"]) {
      const item = view.diff[name];
      grid.append(
        el("span", "", name),
        compactSet(item.onlyA, "only A"),
        compactSet(item.onlyB, "only B"),
        compactSet(item.changed, "changed")
      );
    }
    panel.append(grid);
  }

  /* ---------------- evidence: detail + prism ---------------- */

  function renderDetailEmpty() {
    $("detail-empty").hidden = false;
    $("detail-body").hidden = true;
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
          KMP_APP.selection.selectEntry(id);
          KMP_APP.viewport.centerOn(id);
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

  /* The polytemporal prism: reality, perception, persistence, order — with
     a gradient thread from occurred to ingested making late observation and
     backfill visible. Absent clocks stay visibly absent. */
  function renderPrism(m) {
    const box = $("prism");
    box.textContent = "";
    const prism = KMP_LOOM.prism(m);
    const palette = KMP_APP.scene.palette();
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
          name === "reality" ? palette.cls.causal : name === "perception" ? palette.cls.constraint : palette.cls.evidential;
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
      band.style.background = palette.accent;
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

  /* ---------------- trace box ---------------- */

  function renderTrace(trace) {
    $("trace-box").hidden = false;
    $("d-clear-trace").hidden = false;
    $("trace-status").textContent = `${trace.edges.length} hop${trace.edges.length === 1 ? "" : "s"} · ${trace.rendered.token_count} tokens rendered`;
    const list = $("trace-hops");
    list.textContent = "";
    for (const warning of trace.warnings || []) {
      list.append(el("li", "muted", warning));
    }
    for (const edge of trace.edges) {
      const item = el("li");
      const head = el("div", "rel-head");
      const dash = el("span", "legend-dash");
      dash.style.borderTopColor = classColor(edge.class);
      if (edge.hop) head.append(el("span", "mono muted", `#${edge.hop}`));
      head.append(dash, el("span", "rel-type", edge.rel), el("span", "pill pill-muted", edge.class));
      head.append(el("span", "mono muted", `${edge.source} → ${edge.target}`));
      item.append(head);
      if (edge.why) item.append(el("p", "rel-why", edge.why));
      list.append(item);
    }
  }

  /* ---------------- provenance chip ---------------- */

  function renderProvenance(state) {
    const chip = $("agent-chip");
    const undo = $("agent-undo");
    const change = state.last_change;
    if (!change || change.actor === "human") {
      chip.classList.add("human-owned");
      $("agent-chip-text").textContent = "human-controlled view";
      undo.hidden = true;
      chip.hidden = false;
      return;
    }
    chip.classList.remove("human-owned");
    undo.hidden = false;
    const why = change.explanation ? ` · ${change.explanation}` : "";
    $("agent-chip-text").textContent = `${change.actor} moved the loom${why}`;
    chip.hidden = false;
  }

  /* ---------------- search ---------------- */

  function runSearch() {
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
          KMP_APP.selection.selectEntry(ref);
          KMP_APP.viewport.centerOn(ref);
        });
        results.append(item);
      }
    }
    KMP_APP.scene.requestDraw();
    KMP_APP.sync.reportView();
  }

  /* The search box as an operation, so the sync use case never touches an
     element: setting re-runs the search, reading reports what is typed. */
  function setSearch(text) {
    $("search").value = text;
    $("search").dispatchEvent(new Event("input"));
  }

  function searchText() {
    return $("search").value.trim();
  }

  function hideTraceBox() {
    $("trace-box").hidden = true;
  }

  /* ---------------- control wiring ---------------- */

  function wire() {
    for (const chip of document.querySelectorAll("#clock-chips .chip")) {
      chip.addEventListener("click", () => KMP_APP.viewport.setClock(chip.dataset.clock, false));
    }

    $("lens-mode").addEventListener("change", (event) => {
      view.lensMode = event.target.value;
      KMP_APP.viewport.updateAxisLens();
      KMP_APP.scene.requestDraw();
      KMP_APP.scene.drawNavigator();
    });

    $("focus-context").addEventListener("click", () => {
      if (view.focusRange) {
        view.focusRange = null;
        syncFocusButton();
        KMP_APP.viewport.updateAxisLens();
        KMP_APP.scene.requestDraw();
        KMP_APP.scene.drawNavigator();
        return;
      }
      if (!view.full) return;
      const fullSpan = view.full.t1 - view.full.t0;
      const windowSpan = view.t1 - view.t0;
      if (windowSpan >= fullSpan * 0.999) {
        KMP_APP.dom.showError("zoom into the interval to expand before enabling focus + context");
        return;
      }
      view.focusRange = { from: view.t0, to: view.t1 };
      syncFocusButton();
      KMP_APP.viewport.setWindow(view.full.t0, view.full.t1);
    });

    $("pin-a").addEventListener("click", () => pinComparison("A"));
    $("pin-b").addEventListener("click", () => pinComparison("B"));
    $("clear-diff").addEventListener("click", () => {
      view.pinA = null;
      view.pinB = null;
      view.diff = null;
      renderDiffPanel();
      KMP_APP.scene.requestDraw();
    });

    $("d-trace-from").addEventListener("click", () => {
      tracePick.from = view.selectedRef;
      if (tracePick.from && tracePick.to) KMP_APP.selection.runTrace();
    });
    $("d-trace-to").addEventListener("click", () => {
      tracePick.to = view.selectedRef;
      if (tracePick.from && tracePick.to) KMP_APP.selection.runTrace();
    });
    $("d-clear-trace").addEventListener("click", () => {
      view.trace = null;
      tracePick.from = null;
      tracePick.to = null;
      $("trace-box").hidden = true;
      $("d-clear-trace").hidden = true;
      KMP_APP.scene.requestDraw();
    });

    $("search").addEventListener("input", runSearch);
    $("search").addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        $("search").value = "";
        view.searchHits = new Set();
        $("search-results").textContent = "";
        KMP_APP.scene.requestDraw();
        $("search").blur();
      }
    });

    $("agent-undo").addEventListener("click", () => KMP_APP.sync.undoAgentMove());
  }

  return {
    renderAbouts,
    renderRail,
    setSearch,
    searchText,
    hideTraceBox,
    renderStats,
    syncClockChips,
    syncFocusButton,
    renderPulseLegend,
    pinComparison,
    renderDiffPanel,
    renderDetailEmpty,
    renderDetail,
    renderPrism,
    renderTrace,
    renderProvenance,
    runSearch,
    wire,
  };
})();
