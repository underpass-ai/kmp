/* Application use cases for explicitly selected additional abouts.
   Reads use the same bounded projection port and clock/window as the primary. */
"use strict";
KMP_APP.layers = (() => {
  const { model, view } = KMP_APP.state;
  let generation = 0;
  let extentGeneration = 0;
  async function extent(about, clock, selectors, primaryProbe) {
    const names = [...new Set([about, ...view.layerAbouts])].slice(0, 6);
    const probes = await Promise.all(
      names.map((name) =>
        name === about && primaryProbe
          ? primaryProbe
          : KMP_APP.api.fetchProjection(
              name,
              clock,
              KMP_APP.api.EXTENT_FROM,
              KMP_APP.api.EXTENT_TO,
              "episode",
              128,
              KMP_LOOM.labelQuery(selectors),
            ),
      ),
    );
    const ranges = probes.map(KMP_LOOM.projectionExtent).filter(Boolean);
    if (!ranges.length) return { full: null, bins: [] };
    const t0 = Math.min(...ranges.map((range) => range.t0));
    const t1 = Math.max(...ranges.map((range) => range.t1));
    const pad = Math.max(1, (t1 - t0) * 0.02);
    const full = { t0: t0 - pad, t1: t1 + pad };
    // An extent probe spans the whole clock; its bins are too wide for a
    // useful brush. Request bounded atlas bins over the actual union.
    const overviews = await Promise.all(
      names.map((name) =>
        KMP_APP.api.fetchProjection(
          name,
          clock,
          new Date(Math.round(full.t0)).toISOString(),
          new Date(Math.round(full.t1)).toISOString(),
          "atlas",
          128,
          KMP_LOOM.labelQuery(selectors),
        ),
      ),
    );
    return {
      full,
      bins: overviews.flatMap((probe) => KMP_LOOM.foldAggregates(probe.bins)),
    };
  }
  async function load() {
    if (!view.full) return;
    const request = ++generation;
    const primary = model.about,
      clock = view.clock,
      from = view.t0,
      to = view.t1;
    const abouts = [...new Set(view.layerAbouts)]
      .filter((about) => about !== primary)
      .slice(0, 5);
    const snapshots = await Promise.all(
      abouts.map(async (about) => {
        try {
          const projection = await KMP_APP.api.fetchProjection(
            about,
            clock,
            new Date(Math.round(from)).toISOString(),
            new Date(Math.round(to)).toISOString(),
            model.currentLod,
            128,
            KMP_LOOM.labelQuery(view.selectors),
          );
          return { about, projection, lod: model.currentLod };
        } catch (error) {
          return {
            about,
            projection: {},
            lod: model.currentLod,
            error: error.message,
          };
        }
      }),
    );
    if (
      request !== generation ||
      primary !== model.about ||
      clock !== view.clock ||
      from !== view.t0 ||
      to !== view.t1
    )
      return;
    model.layerProjections = snapshots;
    KMP_APP.scene.requestDraw();
  }
  function invalidate() {
    generation++;
    model.layerProjections = [];
  }
  async function set(abouts) {
    if (abouts.length > 5) {
      KMP_APP.dom.showError("Choose at most five additional abouts.");
      return;
    }
    view.layerAbouts = [...new Set(abouts)].filter(
      (about) => about !== model.about,
    );
    invalidate();
    KMP_APP.panels.renderAbouts();
    const request = ++extentGeneration;
    const primary = model.about,
      clock = view.clock;
    const from = view.t0,
      to = view.t1;
    const wasFull =
      !view.full || (from === view.full.t0 && to === view.full.t1);
    try {
      const result = await extent(primary, clock, view.selectors);
      if (
        request !== extentGeneration ||
        primary !== model.about ||
        clock !== view.clock
      )
        return;
      model.overviewBins = result.bins;
      if (!result.full) {
        await load();
        KMP_APP.sync.reportView();
        return;
      }
      const expand = wasFull && from === view.t0 && to === view.t1;
      view.full = expand
        ? result.full
        : KMP_LOOM.extentIncluding(result.full, view.t0, view.t1);
      KMP_APP.viewport.setWindow(
        expand ? view.full.t0 : view.t0,
        expand ? view.full.t1 : view.t1,
      );
    } catch (error) {
      KMP_APP.dom.showError(error.message);
    }
  }
  async function activate(about) {
    const previous = model.about,
      from = view.t0,
      to = view.t1;
    view.layerAbouts = [...new Set([previous, ...view.layerAbouts])]
      .filter((name) => name && name !== about)
      .slice(0, 5);
    await KMP_APP.data.loadAbout(about, false);
    view.full = KMP_LOOM.extentIncluding(view.full, from, to);
    KMP_APP.viewport.setWindow(from, to);
    KMP_APP.data.cancelScheduledProjection();
    await KMP_APP.data.loadProjection();
    KMP_APP.sync.reportView();
  }
  return { load, set, activate, invalidate, extent };
})();
