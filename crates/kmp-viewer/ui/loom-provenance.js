/* DOM adapter for attribution. It renders a snapshot; it cannot claim control. */
"use strict";
KMP_APP.provenance = (() => {
  const { $ } = KMP_APP.dom;
  function render(state) {
    const change = state.last_change;
    const human = !change || change.actor === "human";
    $("shared-control").dataset.driver = human ? "human" : "agent";
    $("control-owner").textContent = human
      ? "You’re in control"
      : "Agent guiding";
    $("control-reason").textContent =
      change?.explanation ||
      (human
        ? "Explore the memory. Your agent can follow the same frame."
        : "The agent moved this shared view.");
    $("control-detail").textContent = change
      ? `${change.actor} · ${change.at}`
      : "Every shared move has an author and a way back.";
    $("connection-status").textContent =
      `Shared view · revision ${state.view_revision}`;
    $("take-control").hidden = human;
    $("agent-undo").disabled = !state.can_undo;
  }
  function unavailable() {
    $("connection-status").textContent = "View sync unavailable · reconnecting";
  }
  function busy(value) {
    $("take-control").disabled = value;
  }
  function wire() {
    $("take-control").addEventListener("click", () =>
      KMP_APP.control.takeControl(),
    );
  }
  return { render, unavailable, busy, wire };
})();
