/* Transient read activity, independent of DOM, transport and shared view state.
   A newer read of the same resource supersedes its old completion token. */
"use strict";
globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.loading = (() => {
  function createTracker(publish) {
    const pending = new Map();
    function notify() {
      const state = {};
      for (const region of ["scene", "detail"]) {
        const tasks = [...pending.values()].filter(
          (task) => task.region === region,
        );
        const current = tasks.sort((a, b) => a.priority - b.priority).at(-1);
        state[region] = { busy: Boolean(current), label: current?.label || "" };
      }
      publish(state);
    }
    function begin(key, region, label, priority = 0) {
      const token = { region, label, priority };
      pending.delete(key);
      pending.set(key, token);
      notify();
      return () => {
        if (pending.get(key) !== token) return;
        pending.delete(key);
        notify();
      };
    }
    return { begin };
  }

  const tracker = createTracker((state) => KMP_APP.loadingView?.render(state));
  const clocks = {
    observed: "Observed",
    occurred: "Occurred",
    ingested: "Ingested",
    validity: "Validity",
  };
  function beginRequest(path, params = {}, method = "GET") {
    if (method !== "GET") return () => {};
    let region = "scene",
      label;
    switch (path) {
      case "/api/projection":
        label = clocks[params.axis]
          ? `Loading ${clocks[params.axis]} time…`
          : "Loading memory…";
        break;
      case "/api/info":
      case "/api/abouts":
        label = "Loading memory…";
        break;
      case "/api/observability":
        label = "Loading activity…";
        break;
      case "/api/node":
        region = "detail";
        label = "Loading evidence…";
        break;
      case "/api/trace":
        region = "detail";
        label = "Following the proof…";
        break;
      default:
        return () => {}; // Shared-view polling and control are not content reads.
    }
    return tracker.begin(`${path}:${params.about || ""}`, region, label);
  }
  return { createTracker, begin: tracker.begin, beginRequest };
})();
