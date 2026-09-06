/* DOM adapter for read activity. Settle after the next scene paint so chained
   reads share one transition, without a timer or a minimum loading duration. */
"use strict";
KMP_APP.loadingView = (() => {
  const regions = { scene: "stage", detail: "detail" };
  const content = {
    scene: ["loom-canvas", "scene-labels"],
    detail: ["detail-body", "detail-empty"],
  };
  const frames = new Map();
  function apply(region, busy, label) {
    const root = document.getElementById(regions[region]);
    const status = document.getElementById(`${region}-loading`);
    const text = document.getElementById(`${region}-loading-label`);
    if (!root || !status || !text) return;
    root.dataset.loading = String(busy);
    // Keep the live status outside the busy subtree so assistive technology
    // can announce it while the content is still loading.
    for (const id of content[region])
      document.getElementById(id)?.setAttribute("aria-busy", String(busy));
    status.setAttribute("aria-hidden", String(!busy));
    if (busy) text.textContent = label;
  }
  function render(state) {
    for (const region of Object.keys(regions)) {
      cancelAnimationFrame(frames.get(region));
      frames.delete(region);
      const { busy, label } = state[region];
      if (busy) apply(region, true, label);
      else
        frames.set(
          region,
          requestAnimationFrame(() => {
            frames.set(
              region,
              requestAnimationFrame(() => {
                frames.delete(region);
                apply(region, false, "");
              }),
            );
          }),
        );
    }
  }
  return { render };
})();
