/* ChronoLoom composition root. The semantic view and bounded projections
   are shared between human and agent. About planes, the WebGL camera and
   DOM panels are adapters; this file only wires them and joins the view. */
"use strict";

(() => {
  const { model, sync } = KMP_APP.state;
  const { showError, setNotice } = KMP_APP.dom;
  const api = (...args) => KMP_APP.api.call(...args);

  async function init() {
    KMP_APP.scene.wire();
    KMP_APP.panels.wire();
    KMP_APP.catalogue.wire();
    KMP_APP.evidence.wire();
    KMP_APP.timeControls.wire();
    KMP_APP.provenance.wire();
    KMP_APP.gestures.wire();
    KMP_APP.paths.wire();
    KMP_APP.scene.applyTheme();
    try {
      await KMP_APP.scene.setup();
    } catch (error) {
      // Without WebGL only the 3D scene is lost. Layers, labels, search,
      // the time window and memory detail read the kernel, not the GPU,
      // so the page goes on loading and says, for as long as it is true,
      // what is missing.
      KMP_APP.scene.unavailable(error.message);
      setNotice("renderer", "3D scene unavailable: this browser cannot start WebGL");
    }
    try {
      // These describe independent authorities: server identity, the memory
      // catalogue and the already shared view. Start them together so a
      // browser join does not manufacture a serial startup waterfall.
      const infoRead = api("/api/info");
      const aboutsRead = api("/api/abouts");
      const viewRead = api("/api/view", { id: KMP_APP.sync.VIEW_ID }).catch(
        () => null,
      );
      const [info, aboutsView, existing] = await Promise.all([
        infoRead,
        aboutsRead,
        viewRead,
      ]);
      showStore(info);
      model.abouts = aboutsView.abouts;
      KMP_APP.panels.renderAbouts();
      if (!model.abouts.length) {
        setNotice("store", "This store holds no memory yet; ingest some and reload");
        KMP_APP.provenance.idle("Nothing to show yet", "No memory in this store");
        return;
      }

      // A browser joining an already prepared loom is a reader first.
      // Opening the first about before this GET used to erase an agent's
      // focus, overlays and revision merely because the page was cold.
      if (existing && existing.about) {
        sync.revision = existing.view_revision || 0;
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

  /* Which store is open: several sessions each serve a viewer, and the
     header is where a person tells them apart. */
  function showStore(info) {
    const slot = KMP_APP.dom.$("store-path");
    if (!slot || !info?.data_dir) return;
    slot.textContent = info.data_dir;
    slot.title = `Store: ${info.data_dir}`;
    slot.hidden = false;
  }

  init();
})();
