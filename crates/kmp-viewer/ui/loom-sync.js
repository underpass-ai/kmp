/* KMP ChronoLoom — the agent's hand on the loom.
   The view is a shared aggregate with its own revision. An agent moves it by
   declaring intent through kmp_view_apply_intent; the browser follows by
   long poll, and reports back where the human is looking so the agent can
   see it and rebase rather than yanking the loom away mid-gesture. Every
   agent move arrives named, explained and undoable.
   Exposes KMP_APP.sync — the browser half of the aggregate contract. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.sync = (() => {
  const { model, view, sync, tracePick } = KMP_APP.state;
  const api = (...args) => KMP_APP.api.call(...args);

  const VIEW_ID = "default";

  async function viewOpen() {
    try {
      const state = await api(
        "/api/view/open",
        {
          id: VIEW_ID,
          about: model.about || "",
          expected_revision: sync.revision,
        },
        "POST"
      );
      sync.revision = state.view_revision || 0;
      KMP_APP.panels.renderProvenance(state);
      startViewPolling();
    } catch (error) {
      // A viewer that cannot reach its own view still draws the memory.
      KMP_APP.dom.showError(`view sync unavailable: ${error.message}`);
    }
  }

  function startViewPolling() {
    if (sync.polling) return;
    sync.polling = true;
    pollView();
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
          KMP_APP.panels.renderProvenance(state);
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
        await KMP_APP.data.loadAbout(state.about);
      }
      if (state.clock && state.clock !== view.clock) {
        KMP_APP.viewport.setClock(state.clock, false);
      }

      const projection = state.projection || {};
      if (projection.overlays) await KMP_APP.data.loadObservability(projection.overlays);
      if (projection.dimensions) {
        const keep = new Set(projection.dimensions);
        view.hiddenLanes = new Set(
          model.lanes.map((lane) => lane.name).filter((name) => !keep.has(name))
        );
        KMP_APP.panels.renderRail();
      }

      const facets = KMP_LOOM.agentStateFacets(state);
      const { range, refs, explicitRange } = facets;
      let framed = false;
      if (explicitRange) {
        const from = Date.parse(range.from);
        const to = Date.parse(range.to);
        if (Number.isFinite(from) && Number.isFinite(to)) {
          view.focusRange = { from, to };
          KMP_APP.panels.syncFocusButton();
          KMP_APP.viewport.setWindow(view.full.t0, view.full.t1);
          framed = true;
        }
      } else {
        // Ref-only (or empty) focus replaces an earlier explicit range in
        // the complete aggregate snapshot; it is not a patch that
        // preserves it.
        view.focusRange = null;
        KMP_APP.panels.syncFocusButton();
        if (refs.length) framed = await frameRefs(refs);
      }
      // A rung is a density to fall back on, not an override: an intent that
      // named its own window asked for that window.
      if (projection.semantic_zoom && !framed) {
        KMP_APP.viewport.applyZoomRung(projection.semantic_zoom);
      }

      KMP_APP.panels.setSearch(facets.search);
      if (state.trace) {
        tracePick.from = state.trace.from;
        tracePick.to = state.trace.to;
        await KMP_APP.selection.runTrace({ framePath: !explicitRange, preserveWindow: explicitRange });
      }
      if (state.selection) {
        if (await KMP_APP.selection.selectEntry(state.selection)) {
          KMP_APP.viewport.centerOn(state.selection);
        }
      }
    } finally {
      sync.applying = false;
    }
  }

  /* "Frame these refs" — the canonical intent. The window becomes the span
     they occupy on the current clock, with room to breathe. */
  async function frameRefs(refs) {
    const stamps = [];
    for (const ref of refs) {
      let entry = model.byRef.get(ref);
      if (!entry) {
        const inspect = await api("/api/node", { about: model.about, id: ref, raw: "1" });
        entry = KMP_LOOM.entryModel({
          ref_id: inspect.node.id,
          kind: inspect.node.kind,
          text: inspect.node.summary || inspect.node.title || "",
          coordinates: inspect.raw_coordinates || [],
        });
      }
      const stamp = KMP_LOOM.placedMs(entry, view.clock);
      if (stamp !== null) stamps.push(stamp);
    }
    if (!stamps.length) return false;
    const lo = Math.min(...stamps);
    const hi = Math.max(...stamps);
    const pad = Math.max(60000, (hi - lo) * 0.4);
    // The full extent is a cached probe. Include refs ingested after that
    // probe before setWindow clamps the requested range to the stale
    // boundary.
    view.full = KMP_LOOM.extentIncluding(view.full, lo - pad, hi + pad);
    KMP_APP.viewport.setWindow(lo - pad, hi + pad);
    // setWindow normally reloads after the gesture settles. An agent intent
    // is atomic: load the framed projection before applying its trace or
    // selection, so refs outside the previous Moment are present when those
    // moves run.
    KMP_APP.data.cancelScheduledProjection();
    await KMP_APP.data.loadProjection();
    return true;
  }

  /* Where the human is looking, reported so the agent's read of the view is
     the truth rather than whatever it last asked for. Debounced, and silent
     while the loom is busy obeying an intent — otherwise the two would
     echo. */
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
      const search = KMP_APP.panels.searchText();
      if (search) params.set("search", search);
      if (tracePick.from && tracePick.to) {
        params.set("trace_from", tracePick.from);
        params.set("trace_to", tracePick.to);
      }
      const signature = params.toString();
      if (signature === sync.lastReport) return;
      sync.lastReport = signature;
      try {
        const state = await api("/api/view/report", Object.fromEntries(params), "POST");
        if (state.view_revision) sync.revision = state.view_revision;
        KMP_APP.panels.renderProvenance(state);
      } catch (error) {
        // The loom keeps working even when nobody is listening to it.
      }
    }, 400);
  }

  async function undoAgentMove() {
    try {
      const state = await api("/api/view/undo", { id: VIEW_ID }, "POST");
      sync.revision = state.view_revision;
      await applyAgentState(state);
      KMP_APP.panels.renderProvenance(state);
    } catch (error) {
      KMP_APP.dom.showError(error.message);
    }
  }

  return {
    VIEW_ID,
    viewOpen,
    startViewPolling,
    applyAgentState,
    frameRefs,
    reportView,
    undoAgentMove,
  };
})();
