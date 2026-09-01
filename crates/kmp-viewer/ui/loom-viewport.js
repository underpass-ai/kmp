/* KMP ChronoLoom — the viewport.
   The time-window use cases: which clock the axis reads, which window is
   open on it, and the temporal lens that turns instants into screen ratios.
   Everything that moves the window funnels through here, so a move always
   re-renders, reloads and reports the same way. Exposes KMP_APP.viewport. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.viewport = (() => {
  const { model, view, clampWindow } = KMP_APP.state;
  const dom = () => KMP_APP.dom;
  const scene = () => KMP_APP.scene;
  const data = () => KMP_APP.data;
  const sync = () => KMP_APP.sync;

  let axisLens = KMP_LOOM.temporalLens({ mode: "elapsed", t0: 0, t1: 1 });

  function updateAxisLens() {
    const entryEvents = model.entries
      .map((entry) => KMP_LOOM.placedMs(entry, view.clock))
      .filter((time) => time !== null);
    const clusterEvents = model.clusters
      .map((cluster) => {
        const from = Date.parse(cluster.from);
        const to = Date.parse(cluster.to);
        return Number.isFinite(from) && Number.isFinite(to) ? (from + to) / 2 : null;
      })
      .filter((time) => time !== null);
    axisLens = KMP_LOOM.temporalLens({
      mode: view.lensMode,
      t0: view.t0,
      t1: view.t1,
      events: entryEvents.length ? entryEvents : clusterEvents,
      focus: view.focusRange,
    });
  }

  const lens = () => axisLens;
  const xOf = (t) => axisLens.toRatio(t) * scene().canvas().clientWidth;
  const tOf = (x) => axisLens.fromRatio(x / Math.max(1, scene().canvas().clientWidth));

  function setClock(clock, reset, refresh = true) {
    const changed = view.clock !== clock;
    view.clock = clock;
    KMP_APP.panels.syncClockChips(clock);
    if (!view.full) {
      if (changed && model.about) {
        // The empty state must not be a trap: an about with nothing on this
        // clock may hold entries on the one just selected, so a clock change
        // re-probes the about instead of repeating the message (#421).
        data().loadAbout(model.about, false);
        return;
      }
      dom().showError("no entry carries any clock — the loom has no axis to weave on");
      return;
    }
    if (reset || view.t0 >= view.t1) {
      view.t0 = view.full.t0;
      view.t1 = view.full.t1;
      view.windowStack = [];
    } else {
      view.t0 = Math.max(view.t0, view.full.t0);
      view.t1 = Math.min(view.t1, view.full.t1);
    }
    updateAxisLens();
    KMP_APP.panels.renderStats();
    scene().requestDraw();
    scene().drawNavigator();
    sync().reportView();
    if (refresh) data().scheduleProjection();
  }

  function setWindow(t0, t1, remember = true) {
    const clamped = clampWindow(view.full, t0, t1);
    if (remember) view.windowStack.push([view.t0, view.t1]);
    view.t0 = clamped.t0;
    view.t1 = clamped.t1;
    updateAxisLens();
    KMP_APP.panels.renderStats();
    scene().requestDraw();
    scene().drawNavigator();
    sync().reportView();
    data().scheduleProjection();
    data().scheduleObservability();
  }

  /* A rung of the ladder is a density of time per pixel; asking for
     "moment" asks for a window fine enough to show entries. */
  function applyZoomRung(rung) {
    const width = Math.max(1, scene().canvas().clientWidth);
    const target = { atlas: 1.2e6, episode: 120e3, moment: 8e3 }[rung];
    if (!target) return;
    const span = Math.max(1000, target * width);
    const centre = (view.t0 + view.t1) / 2;
    setWindow(centre - span / 2, centre + span / 2);
  }

  function centerOn(ref) {
    const m = model.byRef.get(ref);
    if (!m) return;
    const t = KMP_LOOM.placedMs(m, view.clock);
    if (t === null) return;
    const span = view.t1 - view.t0;
    if (t < view.t0 || t > view.t1) setWindow(t - span / 2, t + span / 2);
  }

  return { updateAxisLens, lens, xOf, tOf, setClock, setWindow, applyZoomRung, centerOn };
})();
