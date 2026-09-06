/* Shared-control use case and DOM adapter. Provenance comes from the server;
   an agent attribution is not evidence of a live connection or an exclusive lease. */
"use strict";
KMP_APP.control = (() => {
  const { sync } = KMP_APP.state;
  let snapshot = null,
    taking = false;
  function render(state) {
    snapshot = state;
    KMP_APP.provenance.render(state);
  }
  function unavailable() {
    KMP_APP.provenance.unavailable();
  }
  async function takeControl() {
    if (taking || !snapshot || snapshot.last_change?.actor === "human") return;
    taking = true;
    KMP_APP.provenance.busy(true);
    try {
      const state = await KMP_APP.api.call(
        "/api/view/take-control",
        { id: KMP_APP.sync.VIEW_ID, expected_revision: sync.revision },
        "POST",
      );
      sync.revision = state.view_revision;
      render(state);
    } catch (error) {
      KMP_APP.dom.showError(error.message);
      // A stale handoff must show the actual frame before it can be retried.
      try {
        const state = await KMP_APP.api.call("/api/view", {
          id: KMP_APP.sync.VIEW_ID,
        });
        sync.revision = state.view_revision;
        await KMP_APP.sync.applyAgentState(state);
        render(state);
      } catch (_) {
        unavailable();
      }
    } finally {
      taking = false;
      KMP_APP.provenance.busy(false);
    }
  }
  function humanGesture() {
    if (
      !sync.applying &&
      snapshot?.last_change &&
      snapshot.last_change.actor !== "human"
    )
      takeControl();
  }
  return { render, humanGesture, takeControl, unavailable };
})();
