/* KMP ChronoLoom — the data use cases.
   Loading an about, its projection at the right rung, and the telemetry
   aligned over the window. These orchestrate the backend port and hand the
   results to the state; rendering is the scene's and the panels' business.
   Exposes KMP_APP.data. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.data = (() => {
  const { model, view, sync } = KMP_APP.state;
  const fetchProjection = (...args) => KMP_APP.api.fetchProjection(...args);
  const api = (...args) => KMP_APP.api.call(...args);
  const showError = (message) => KMP_APP.dom.showError(message);

  function lanesFromProjection(projection, entries) {
    if (entries.length) return KMP_LOOM.buildLanes(entries);
    const counts = new Map();
    const source = (projection.clusters || []).length
      ? projection.clusters
      : projection.bins || [];
    for (const item of source) {
      counts.set(item.dimension, (counts.get(item.dimension) || 0) + Number(item.total || 0));
    }
    for (const dimension of projection.included_dimensions || []) {
      if (!counts.has(dimension)) counts.set(dimension, 0);
    }
    return [...counts].map(([name, count], index) => ({
      name,
      index,
      count,
      scopes: new Map(),
    }));
  }

  function applyProjection(projection, lod) {
    const entries = (projection.entries || [])
      .map(KMP_LOOM.entryModel)
      .sort(KMP_LOOM.compareModels);
    model.projection = projection;
    model.currentLod = lod;
    model.maxMarksPerLane = KMP_LOOM.maxMarksPerLane(projection);
    model.total = Number((projection.page && projection.page.total) || 0);
    model.bins = projection.bins || [];
    model.clusters = projection.clusters || [];
    model.entries = entries;
    model.byRef = new Map(entries.map((entry) => [entry.ref, entry]));
    model.lanes = lanesFromProjection(projection, entries);
    model.laneIndex = new Map(model.lanes.map((lane) => [lane.name, lane.index]));
    model.proofEdges = (projection.relations || []).map((edge) => ({
      ...edge,
      source: edge.from,
      target: edge.to,
    }));
    const classified = KMP_LOOM.classifyEdges(model.proofEdges, (ref) => model.byRef.has(ref));
    model.edges = classified.arcs;
    model.supersessions = classified.supersessions;
    model.contradictions = classified.contradictions;
    model.supersededRefs = new Set(classified.supersessions.map((edge) => edge.target));
    model.contradictedRefs = new Set(
      classified.contradictions.flatMap((edge) => [edge.source, edge.target])
    );
    const fullSpan = view.full && view.full.t1 - view.full.t0;
    if (fullSpan && view.t1 - view.t0 >= fullSpan * 0.999) {
      model.overviewBins = model.bins;
    }
    KMP_APP.viewport.updateAxisLens();
    KMP_APP.panels.renderRail();
    KMP_APP.panels.renderStats();
    KMP_APP.scene.requestDraw();
    KMP_APP.scene.drawNavigator();
  }

  async function loadProjection() {
    if (!model.about || !view.full) return;
    const generation = ++model.loadGeneration;
    const width = Math.max(1, KMP_APP.scene.canvas().clientWidth || 1);
    const msPerPx = (view.t1 - view.t0) / width;
    let lod = KMP_LOOM.lodFor(msPerPx, width, model.maxMarksPerLane);
    const bins = Math.max(24, Math.min(512, Math.floor(width / 7)));
    const fetchAt = (level) =>
      fetchProjection(
        model.about,
        view.clock,
        new Date(Math.round(view.t0)).toISOString(),
        new Date(Math.round(view.t1)).toISOString(),
        level,
        bins
      );
    try {
      let projection = await fetchAt(lod);
      if (generation !== model.loadGeneration) return;
      const resolvedLod = KMP_LOOM.lodFor(
        msPerPx,
        width,
        KMP_LOOM.maxMarksPerLane(projection)
      );
      if (resolvedLod !== lod) {
        lod = resolvedLod;
        projection = await fetchAt(lod);
        if (generation !== model.loadGeneration) return;
      }
      applyProjection(projection, lod);
      if (projection.truncated) {
        showError(
          `projection is partial (${projection.page.returned}/${projection.page.total}); zoom into a smaller range for detail`
        );
      } else {
        showError("");
      }
    } catch (error) {
      if (generation === model.loadGeneration) showError(error.message);
    }
  }

  function scheduleProjection() {
    clearTimeout(scheduleProjection.timer);
    scheduleProjection.timer = setTimeout(() => loadProjection(), 120);
  }

  /* An agent intent is atomic: its framed projection must load before its
     trace or selection applies, so the debounce is cancelled outright. */
  function cancelScheduledProjection() {
    clearTimeout(scheduleProjection.timer);
  }

  async function loadObservability(series = view.overlays) {
    view.overlays = [...new Set(series || [])];
    if (!view.overlays.length || !view.full) {
      model.observability = { series: [], exemplars: [] };
      KMP_APP.panels.renderPulseLegend();
      KMP_APP.scene.requestDraw();
      return;
    }
    try {
      model.observability = await api("/api/observability", {
        about: model.about,
        from_ms: Math.max(0, Math.floor(view.t0)),
        to_ms: Math.max(0, Math.ceil(view.t1)),
        series: view.overlays.join(","),
        limit: 4096,
      });
      if (model.observability.missing && model.observability.missing.length) {
        showError(`telemetry series unavailable: ${model.observability.missing.join(", ")}`);
      }
      KMP_APP.panels.renderPulseLegend();
      KMP_APP.scene.requestDraw();
    } catch (error) {
      model.observability = { series: [], exemplars: [] };
      // Never reserve a permanent blank pulse band. The error remains
      // visible in the status slot long enough to diagnose; the lanes
      // immediately get their space back.
      view.overlays = [];
      KMP_APP.panels.renderPulseLegend();
      showError(error.message);
      KMP_APP.scene.requestDraw();
    }
  }

  function scheduleObservability() {
    if (!view.overlays.length) return;
    clearTimeout(scheduleObservability.timer);
    scheduleObservability.timer = setTimeout(() => loadObservability(), 120);
  }

  async function loadAbout(about, announce = true) {
    const previouslyApplying = sync.applying;
    sync.applying = true;
    try {
      const generation = ++model.loadGeneration;
      // Episode carries exact cluster endpoints without downloading entry
      // bodies. It is the cheap extent probe; the next request uses that
      // exact visible range and the rung the screen can actually display.
      const probe = await fetchProjection(
        about,
        view.clock,
        KMP_APP.api.EXTENT_FROM,
        KMP_APP.api.EXTENT_TO,
        "episode",
        128
      );
      if (generation !== model.loadGeneration) return;
      const extent = KMP_LOOM.projectionExtent(probe);
      // The clicked about becomes the current about even when this clock is
      // empty: keeping the previous about's picture on screen presented
      // stale data as if it belonged to the one the user selected (#421).
      model.about = about;
      model.maxMarksPerLane = KMP_LOOM.maxMarksPerLane(probe);
      model.overviewBins = probe.bins || [];
      if (extent) {
        const pad = Math.max(1, (extent.t1 - extent.t0) * 0.02);
        view.full = { t0: extent.t0 - pad, t1: extent.t1 + pad };
        view.t0 = view.full.t0;
        view.t1 = view.full.t1;
        view.windowStack = [];
      }
      model.observability = { series: [], exemplars: [] };
      view.selectedRef = null;
      view.trace = null;
      view.searchHits = new Set();
      view.hiddenLanes = new Set();
      view.pinA = null;
      view.pinB = null;
      view.diff = null;
      view.focusRange = null;
      KMP_APP.panels.syncFocusButton();
      KMP_APP.panels.renderDiffPanel();
      KMP_APP.panels.hideTraceBox();
      KMP_APP.panels.renderDetailEmpty();
      if (!extent) {
        // An explicit empty state for the active clock: zero entries, lanes
        // and relations, a cleared canvas and navigator, and the
        // empty-clock explanation — nothing announced, because there is no
        // range to share.
        view.full = null;
        view.windowStack = [];
        applyProjection({ entries: [], page: { total: 0 }, bins: [], clusters: [], relations: [] }, "atlas");
        KMP_APP.viewport.setClock(view.clock, true, false);
        KMP_APP.panels.renderAbouts();
        KMP_APP.panels.renderRail();
        KMP_APP.panels.renderStats();
        KMP_APP.scene.requestDraw();
        KMP_APP.scene.drawNavigator();
        return;
      }
      KMP_APP.viewport.setClock(view.clock, true, false);
      await loadProjection();
      KMP_APP.panels.renderAbouts();
      sync.applying = previouslyApplying;
      if (announce && !previouslyApplying) await KMP_APP.sync.viewOpen();
    } catch (error) {
      showError(error.message);
    } finally {
      sync.applying = previouslyApplying;
    }
  }

  return {
    lanesFromProjection,
    applyProjection,
    loadProjection,
    scheduleProjection,
    cancelScheduledProjection,
    loadObservability,
    scheduleObservability,
    loadAbout,
  };
})();
