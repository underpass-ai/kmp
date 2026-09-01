/* KMP ChronoLoom — the loom's state.
   One place for everything the application knows: the memory model the
   kernel returned, the view the person is holding, and the sync ledger the
   shared aggregate advances. The objects are stable references — modules
   destructure them once and mutate properties, never reassign — so every
   module reads the same truth.
   Pure besides the data: no DOM, no PIXI, no fetch. Exposes KMP_APP.state. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.state = (() => {
  /* What the kernel said: the current about, its projection, entries,
     lanes, relations and telemetry. */
  const model = {
    about: null,
    abouts: [],
    projection: null,
    currentLod: "atlas",
    maxMarksPerLane: 0,
    total: 0,
    bins: [],
    clusters: [],
    overviewBins: [],
    loadGeneration: 0,
    entries: [], // entryModels, ordered
    byRef: new Map(),
    lanes: [], // [{name, index, count, scopes}]
    laneIndex: new Map(),
    edges: [], // explanatory arcs (classified)
    supersessions: [],
    contradictions: [],
    proofEdges: [],
    supersededRefs: new Set(),
    contradictedRefs: new Set(),
    observability: { series: [], exemplars: [] },
  };

  /* What the person is holding: clock, window, filters, selection, trace. */
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
    trace: null, // {refs:Set, edgeKeys:Set}
    overlays: [],
    lensMode: "elapsed",
    focusRange: null,
    pinA: null,
    pinB: null,
    diff: null,
  };

  /* The shared-aggregate ledger: revision, echo suppression, report debounce. */
  const sync = {
    revision: 0,
    applying: false,
    reportTimer: null,
    lastReport: "",
    polling: false,
  };

  /* The two ends a trace is being picked from. Mutated in place. */
  const tracePick = { from: null, to: null };

  /* How faded an entry draws under the active filters. The domain of "what
     is emphasized right now", independent of any renderer. */
  function entryAlpha(m) {
    let a = 1;
    if (model.supersededRefs.has(m.ref)) a = 0.35; // history, not garbage
    if (view.dimmedKinds.has(m.kind)) a = Math.min(a, 0.15);
    if (view.searchHits.size) a = view.searchHits.has(m.ref) ? 1 : Math.min(a, 0.15);
    if (view.trace) a = view.trace.refs.has(m.ref) ? 1 : Math.min(a, 0.12);
    return a;
  }

  function visibleLanes() {
    return model.lanes.filter((lane) => !view.hiddenLanes.has(lane.name));
  }

  /* Clamps a requested window into the known extent with a one-second
     floor — the arithmetic of setWindow, kept pure and testable. */
  function clampWindow(full, t0, t1, minSpan = 1000) {
    const clamped0 = Math.max(full.t0, Math.min(t0, full.t1 - minSpan));
    const clamped1 = Math.min(full.t1, Math.max(t1, clamped0 + minSpan));
    return { t0: clamped0, t1: clamped1 };
  }

  return { model, view, sync, tracePick, entryAlpha, visibleLanes, clampWindow };
})();
