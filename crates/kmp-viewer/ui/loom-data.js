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
      counts.set(
        item.dimension,
        (counts.get(item.dimension) || 0) + Number(item.total || 0),
      );
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

  // Index every currently projected owner for selection and evidence panels.
  // The individual projections still define the separate scene planes.
  function refreshEntries() {
    const projections = [{about: model.about, projection: model.projection || {}},
      ...model.layerProjections];
    const entries = projections.flatMap(({about, projection}) =>
      (projection.entries || []).map(KMP_LOOM.entryModel).map((entry) => ({...entry, about})));
    model.byRef = new Map(entries.map((entry) => [entry.ref, entry]));
    model.entries = [...model.byRef.values()].sort(KMP_LOOM.compareModels);
    model.total = projections.reduce((count, {projection}) => count + Number(projection.page?.total || 0), 0);
    model.lanes = lanesFromProjection(model.projection, model.entries);
    model.laneIndex = new Map(
      model.lanes.map((lane) => [lane.name, lane.index]),
    );
    model.proofEdges = projections.flatMap(({ projection }) => projection.relations || []).map((edge) => ({
      ...edge,
      source: edge.from,
      target: edge.to,
    }));
    const classified = KMP_LOOM.classifyEdges(model.proofEdges, (ref) =>
      model.byRef.has(ref),
    );
    model.edges = classified.arcs;
    model.supersessions = classified.supersessions;
    model.contradictions = classified.contradictions;
    model.supersededRefs = new Set(
      classified.supersessions.map((edge) => edge.target),
    );
    model.contradictedRefs = new Set(
      classified.contradictions.flatMap((edge) => [edge.source, edge.target]),
    );
  }

  function applyProjection(projection, lod) {
    model.projection = projection;
    model.currentLod = lod;
    model.maxMarksPerLane = KMP_LOOM.maxMarksPerLane(projection);
    // Label aggregates stay distinct from individual memories.
    model.bins = KMP_LOOM.foldAggregates(projection.bins);
    model.clusters = KMP_LOOM.foldAggregates(projection.clusters);
    refreshEntries();
    const fullSpan = view.full && view.full.t1 - view.full.t0;
    if (
      !view.layerAbouts.length &&
      fullSpan &&
      view.t1 - view.t0 >= fullSpan * 0.999
    ) {
      model.overviewBins = model.bins;
    }
    KMP_APP.viewport.updateAxisLens();
    KMP_APP.panels.renderRail();
    KMP_APP.panels.renderStats();
    KMP_APP.scene.requestDraw();
    KMP_APP.scene.drawNavigator();
    return KMP_APP.layers?.load();
  }

  async function loadProjection() {
    if (!model.about || !view.full) return;
    KMP_APP.layers?.invalidate();
    const generation = ++model.loadGeneration;
    const width = Math.max(1, KMP_APP.scene.canvas().clientWidth || 1);
    const msPerPx = (view.t1 - view.t0) / width;
    let lod =
      view.requestedLod ||
      KMP_LOOM.lodFor(msPerPx, width, model.maxMarksPerLane);
    const bins = Math.max(24, Math.min(512, Math.floor(width / 7)));
    const fetchAt = (level) =>
      fetchProjection(
        model.about,
        view.clock,
        new Date(Math.round(view.t0)).toISOString(),
        new Date(Math.round(view.t1)).toISOString(),
        level,
        bins,
        KMP_LOOM.labelQuery(view.selectors),
      );
    try {
      let projection = await fetchAt(lod);
      if (generation !== model.loadGeneration) return;
      const resolvedLod = KMP_LOOM.lodFor(
        msPerPx,
        width,
        KMP_LOOM.maxMarksPerLane(projection),
      );
      if (!view.requestedLod && resolvedLod !== lod) {
        lod = resolvedLod;
        projection = await fetchAt(lod);
        if (generation !== model.loadGeneration) return;
      }
      await applyProjection(projection, lod);
      if (generation !== model.loadGeneration) return;
      if (projection.truncated) {
        showError(
          `projection is partial (${projection.page.returned}/${projection.page.total}); zoom into a smaller range for detail`,
        );
      } else {
        showError("");
      }
    } catch (error) {
      if (generation === model.loadGeneration) showError(error.message);
    }
  }

  /* A chip changed: the kernel filters, so the projection is asked again,
     and both faces hear about it. */
  async function setSelectors(selectors) {
    view.selectors = KMP_LOOM.normalizeSelectors(selectors);
    KMP_APP.panels.renderChips();
    if (!model.about) return;
    // The extent may move under a filter; re-probe like a fresh about.
    await KMP_APP.data.loadAbout(model.about, false);
    KMP_APP.sync.reportView();
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
        showError(
          `telemetry series unavailable: ${model.observability.missing.join(", ")}`,
        );
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
      // A chip belongs to the about whose catalogue named it; a fresh
      // about starts without any, unless the caller is applying a snapshot
      // that carries them.
      if (about !== model.about && !previouslyApplying) view.selectors = [];
      const probe = await fetchProjection(
        about,
        view.clock,
        KMP_APP.api.EXTENT_FROM,
        KMP_APP.api.EXTENT_TO,
        "episode",
        128,
        KMP_LOOM.labelQuery(view.selectors),
      );
      if (generation !== model.loadGeneration) return;
      const combined = KMP_APP.layers
        ? await KMP_APP.layers.extent(about, view.clock, view.selectors, probe)
        : null;
      if (generation !== model.loadGeneration) return;
      const extent = KMP_LOOM.projectionExtent(probe);
      // The clicked about becomes the current about even when this clock is
      // empty: keeping the previous about's picture on screen presented
      // stale data as if it belonged to the one the user selected (#421).
      model.about = about;
      model.maxMarksPerLane = KMP_LOOM.maxMarksPerLane(probe);
      model.overviewBins = combined
        ? combined.bins
        : KMP_LOOM.foldAggregates(probe.bins);
      const hasExtent = combined ? Boolean(combined.full) : Boolean(extent);
      if (hasExtent) {
        if (combined) view.full = combined.full;
        else {
          const pad = Math.max(1, (extent.t1 - extent.t0) * 0.02);
          view.full = { t0: extent.t0 - pad, t1: extent.t1 + pad };
        }
        view.t0 = view.full.t0;
        view.t1 = view.full.t1;
        view.windowStack = [];
      }
      model.observability = { series: [], exemplars: [] };
      view.selectedRef = null;
      view.trace = null;
      view.searchHits = new Set();
      view.hiddenLanes = new Set();
      KMP_APP.panels.renderChips();
      view.pinA = null;
      view.pinB = null;
      view.diff = null;
      view.focusRange = null;
      KMP_APP.panels.syncFocusButton();
      KMP_APP.panels.renderDiffPanel();
      KMP_APP.panels.hideTraceBox();
      KMP_APP.panels.renderDetailEmpty();
      if (!hasExtent) {
        // An explicit empty state for the active clock: zero entries, lanes
        // and relations, a cleared canvas and navigator, and the
        // empty-clock explanation — nothing announced, because there is no
        // range to share.
        view.full = null;
        view.windowStack = [];
        applyProjection(
          {
            entries: [],
            page: { total: 0 },
            bins: [],
            clusters: [],
            relations: [],
          },
          "atlas",
        );
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
    refreshEntries,
    setSelectors,
    loadProjection,
    scheduleProjection,
    cancelScheduledProjection,
    loadObservability,
    scheduleObservability,
    loadAbout,
  };
})();
