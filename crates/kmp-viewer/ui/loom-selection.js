/* KMP ChronoLoom — selection and trace use cases.
   Selecting an entry is evidence work: the entry is inspected through the
   same read the agent uses, revealed at Moment when the current rung has no
   body for it, and its audit path is fetched rather than guessed.
   Exposes KMP_APP.selection. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.selection = (() => {
  const { model, view, tracePick } = KMP_APP.state;
  const api = (...args) => KMP_APP.api.call(...args);
  const showError = (message) => KMP_APP.dom.showError(message);

  /* A coarse projection has no entry bodies, so a ref an agent names cannot
     be resolved from model.byRef yet. Inspecting it gives us its real
     clocks; move to a Moment-sized window around that clock, then ask the
     projection for the entry rather than pretending the selection was
     honoured. */
  async function revealEntryAtMoment(ref) {
    const inspect = await api("/api/node", { about: model.about, id: ref, raw: "1" });
    const entry = KMP_LOOM.entryModel({
      ref_id: inspect.node.id,
      kind: inspect.node.kind,
      text: inspect.node.summary || inspect.node.title || "",
      coordinates: inspect.raw_coordinates || [],
    });
    const instant = KMP_LOOM.placedMs(entry, view.clock);
    if (instant === null) {
      throw new Error(`cannot reveal ${ref}: it carries no temporal coordinate`);
    }
    const momentSpan = Math.max(1000, 8000 * Math.max(1, KMP_APP.scene.canvas().clientWidth));
    KMP_APP.viewport.setWindow(instant - momentSpan / 2, instant + momentSpan / 2);
    KMP_APP.data.cancelScheduledProjection();
    await KMP_APP.data.loadProjection();
    if (!model.byRef.has(ref)) {
      throw new Error(`cannot reveal ${ref}: it is not present in the Moment projection`);
    }
    return inspect;
  }

  async function selectEntry(ref) {
    try {
      const revealed = model.byRef.has(ref) ? null : await revealEntryAtMoment(ref);
      const m = model.byRef.get(ref);
      if (!m) throw new Error(`cannot select ${ref}: the entry is not in this projection`);
      const inspect = revealed || (await api("/api/node", { about: model.about, id: ref, raw: "1" }));
      view.selectedRef = ref;
      KMP_APP.scene.requestDraw();
      KMP_APP.sync.reportView();
      KMP_APP.panels.renderPrism(m);
      KMP_APP.panels.renderDetail(inspect, m);
      return true;
    } catch (error) {
      showError(error.message);
      return false;
    }
  }

  async function runTrace({ framePath = false, preserveWindow = false } = {}) {
    try {
      const trace = await api("/api/trace", { about: model.about, from: tracePick.from, to: tracePick.to });
      if (framePath) {
        await KMP_APP.sync.frameRefs(trace.nodes.map((node) => node.id));
      } else if (!preserveWindow) {
        const missingEndpoint = [tracePick.from, tracePick.to].find(
          (ref) => ref && !model.byRef.has(ref)
        );
        if (missingEndpoint) await revealEntryAtMoment(missingEndpoint);
      }
      view.trace = {
        refs: new Set(trace.nodes.map((n) => n.id)),
        edgeKeys: new Set(trace.edges.map((e) => `${e.source} ${e.rel} ${e.target}`)),
      };
      KMP_APP.panels.renderTrace(trace);
      KMP_APP.scene.requestDraw();
      KMP_APP.sync.reportView();
      showError("");
    } catch (error) {
      showError(error.message);
    }
  }

  return { revealEntryAtMoment, selectEntry, runTrace };
})();
