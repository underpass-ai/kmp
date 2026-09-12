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

  /* A list row that acts like a button acts like one for the keyboard too.
     These rows are <li> so the list keeps its structure for a reader; this
     gives them the role, the tab stop and the two keys that go with it. */
  function activateOn(node, run) {
    node.tabIndex = 0;
    node.setAttribute("role", "button");
    node.addEventListener("click", run);
    node.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      run();
    });
    return node;
  }

  /* ---------------- abouts, lanes, legends ---------------- */

  function renderAbouts() { KMP_APP.catalogue.renderAbouts(); }

  function renderRail() {
    KMP_APP.catalogue.renderLabels();

    const kinds = new Map();
    for (const projection of [model.projection || {}, ...model.layerProjections.map((layer) => layer.projection)]) {
      const projectedKinds = Object.entries(projection.by_kind || {});
      if (projectedKinds.length) {
        for (const [kind, count] of projectedKinds) kinds.set(kind, (kinds.get(kind) || 0) + Number(count));
      } else {
        for (const entry of projection.entries || []) kinds.set(entry.kind, (kinds.get(entry.kind) || 0) + 1);
      }
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
      activateOn(item, () => {
        if (view.dimmedKinds.has(kind)) view.dimmedKinds.delete(kind);
        else view.dimmedKinds.add(kind);
        renderRail();
        KMP_APP.scene.requestDraw();
      });
      item.setAttribute("aria-pressed", view.dimmedKinds.has(kind) ? "true" : "false");
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

  /* ---------------- label chips ----------------
     The predicates the kernel filters the projection by, as the person and
     the agent both see them. A chip reads `key op values`; its cross takes
     it off and asks the projection again. */

  function renderChips() {
    const strip = $("label-chips");
    strip.textContent = "";
    const selectors = KMP_LOOM.normalizeSelectors(view.selectors);
    strip.hidden = !selectors.length;
    for (const selector of selectors) {
      const chip = el("li", "label-chip");
      chip.append(el("span", "chip-key", selector.key), el("span", "chip-op", selector.op));
      if (selector.values.length) chip.append(el("span", "chip-values mono", selector.values.join(" | ")));
      const remove = el("button", "chip-remove", "×");
      remove.title = "take this filter off";
      remove.addEventListener("click", () => {
        KMP_APP.data.setSelectors(
          view.selectors.filter((s) => !(s.key === selector.key && s.op === selector.op))
        );
      });
      chip.append(remove);
      strip.append(chip);
    }
  }

  /* ---------------- status line ---------------- */

  function renderStats() {
    $("s-entries").textContent = String(model.total);
    $("s-lanes").textContent = String((model.projection?.labels || []).length);
    KMP_APP.evidence?.renderPicker();
    $("s-relations").textContent =
      model.currentLod === "moment"
        ? String(model.edges.length + model.supersessions.length + model.contradictions.length)
        : "—";
    const clocked = model.entries.filter((m) => KMP_LOOM.strictMs(m, view.clock) !== null).length;
    $("s-clocked").textContent =
      model.currentLod === "moment" ? `${clocked}/${model.total}` : `—/${model.total}`;
    const missingClock = $("clock-missing");
    const missingAxisEntries = Number(
      (model.projection?.metrics || []).find((metric) => metric.name === "missing_axis_entries")?.value || 0
    );
    const sourceTruncated = (model.projection?.missing || []).includes("visual_source_entries");
    missingClock.hidden = missingAxisEntries === 0;
    missingClock.textContent = missingAxisEntries ? `${missingAxisEntries} without ${view.clock} time` : "";
    missingClock.title = sourceTruncated
      ? "At least this many reviewed source entries lack the selected clock; the source is truncated"
      : "Entries omitted because they have no position on the selected clock";
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
    KMP_APP.observability?.render();
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
    if (model.currentLod !== "moment" || model.projection?.truncated ||
        model.layerProjections.some(layer => layer.error || layer.projection.truncated)) {
      KMP_APP.dom.showError("Choose a complete Memories window before pinning a comparison."); return;
    }
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
    if ($("memory-picker")) $("memory-picker").value = "";
    $("detail-empty").hidden = false;
    $("detail-body").hidden = true;
  }

  function renderDetail(inspect, m) {
    if ($("memory-picker")) $("memory-picker").value = m.ref;
    KMP_APP.timeControls?.refresh();
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
          `${c.dimension}=${c.value} / ${c.scope}` +
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
    const link = (id) =>
      activateOn(el("a", "rel-target mono", id), async () => {
        if (await KMP_APP.selection.selectEntry(id)) {
          KMP_APP.viewport.centerOn(id);
        }
      });
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
      renderRelationClocks(item, relation);
      list.append(item);
    }
  }

  /* The polytemporal prism: reality, perception, persistence, order — with
     a gradient thread from occurred to ingested making late observation and
     backfill visible. Absent clocks stay visibly absent. */
  function renderPrism(m) { KMP_APP.evidence.renderPrism(m); }

  function renderRelationClocks(item, relation) {
    const fields = [["Occurred", "occurred_at"], ["Observed", "observed_at"],
      ["Ingested", "ingested_at"], ["Valid from", "valid_from"], ["Valid until", "valid_until"]];
    const clocks = relation.clocks || relation;
    const values = fields.filter(([, key]) => clocks[key])
      .map(([label, key]) => `${label}: ${clocks[key]}`);
    if (values.length) item.append(el("p", "muted relation-clocks", values.join(" · ")));
    else if (relation.class !== "structural") item.append(el("p", "muted relation-clocks", "Relation clocks unknown"));
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
      renderRelationClocks(item, edge);
      list.append(item);
    }
  }

  /* ---------------- provenance chip ---------------- */

  function renderProvenance(state) { KMP_APP.control.render(state); }

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
      /* A search that matches nothing is a result too. Saying so beats an
         empty rail, which reads as "still working" or "nothing loaded". */
      if (!view.searchHits.size) {
        results.append(
          el("li", "empty-hint", "No memory in this window matches that query.")
        );
      }
      if (view.searchHits.size > 50) {
        results.append(el("li", "empty-hint", `${view.searchHits.size} hits · showing 50`));
      }
      for (const ref of [...view.searchHits].slice(0, 50)) {
        const m = model.byRef.get(ref);
        const item = el("li", "", m.text.length > 60 ? m.text.slice(0, 59) + "…" : m.text);
        item.append(el("span", "sub mono", ref));
        activateOn(item, () => {
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
    renderChips,
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
