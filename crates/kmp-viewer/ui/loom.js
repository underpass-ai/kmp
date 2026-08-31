/* KMP ChronoLoom — the composition root.
   Time is the primary geometry: a horizontal clock you choose (occurred,
   observed, ingested, validity), memory dimensions as stable lanes, entries
   as marks joining lanes with braids, explanatory relations as class-styled
   arcs, and a semantic-zoom ladder — atlas, episode, moment — that changes
   representation, not just size. Evidence is a selection, not a zoom.

   Vanilla JS + the vendored pixi bundle, in script-tag modules: pure logic
   in loom-core.js; state, the backend port, the use cases and the adapters
   each in their own loom-*.js file, all registered on KMP_APP. This file
   only wires them together and starts the loom. */
"use strict";

(() => {
  const { model, sync } = KMP_APP.state;
  const { showError } = KMP_APP.dom;
  const api = (...args) => KMP_APP.api.call(...args);

  async function init() {
    KMP_APP.scene.wire();
    KMP_APP.panels.wire();
    KMP_APP.gestures.wire();
    KMP_APP.scene.applyTheme();
    try {
      await KMP_APP.scene.setup();
    } catch (error) {
      showError(`renderer failed to start: ${error.message}`);
      return;
    }
    try {
      await api("/api/info");
      const aboutsView = await api("/api/abouts");
      model.abouts = aboutsView.abouts;
      KMP_APP.panels.renderAbouts();
      if (!model.abouts.length) {
        showError("the kernel holds no abouts yet — ingest some memory first");
        return;
      }

      // A browser joining an already prepared loom is a reader first.
      // Opening the first about before this GET used to erase an agent's
      // focus, overlays and revision merely because the page was cold.
      let existing = null;
      try {
        existing = await api("/api/view", { id: KMP_APP.sync.VIEW_ID });
      } catch (_) {
        // No aggregate exists yet; this browser will create the first one.
      }
      if (existing && existing.about) {
        sync.revision = existing.view_revision || 0;
        await KMP_APP.data.loadAbout(existing.about, false);
        await KMP_APP.sync.applyAgentState(existing);
        KMP_APP.panels.renderProvenance(existing);
        KMP_APP.sync.startViewPolling();
      } else {
        await KMP_APP.data.loadAbout(model.abouts[0]);
      }
    } catch (error) {
      showError(error.message);
    }
  }

  init();
})();
