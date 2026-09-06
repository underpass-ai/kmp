/* DOM adapter for exact UTC dates, presets and selection focus. */
"use strict";
KMP_APP.timeControls = (() => {
  const { model, view } = KMP_APP.state;
  const { $ } = KMP_APP.dom;
  const move = (from, to) => KMP_APP.viewport.setWindow(from, to);
  function refresh() {
    if (!view.full) return;
    if (document.activeElement !== $("time-from"))
      $("time-from").value = new Date(Math.round(view.t0))
        .toISOString()
        .slice(0, -1);
    if (document.activeElement !== $("time-to"))
      $("time-to").value = new Date(Math.round(view.t1))
        .toISOString()
        .slice(0, -1);
    $("focus-selection").disabled = !view.selectedRef;
    const span = Math.round(view.t1 - view.t0);
    $("time-span").value =
      view.t0 === view.full.t0 && view.t1 === view.full.t1
        ? "all"
        : [3600000, 21600000, 86400000, 259200000].includes(span)
          ? String(span)
          : "custom";
  }
  function wire() {
    $("time-apply").addEventListener("click", () => {
      const from = Date.parse($("time-from").value + "Z"),
        to = Date.parse($("time-to").value + "Z");
      if (!Number.isFinite(from) || !Number.isFinite(to) || from >= to) {
        KMP_APP.dom.showError("Choose an end after the start (UTC).");
        return;
      }
      if (view.full) move(from, to);
    });
    for (const [id, direction] of [
      ["time-previous", -1],
      ["time-next", 1],
    ])
      $(id).addEventListener("click", () => {
        if (!view.full) return;
        const delta = (view.t1 - view.t0) * direction * 0.5;
        move(view.t0 + delta, view.t1 + delta);
      });
    $("time-span").addEventListener("change", (event) => {
      if (!view.full) return;
      if (event.target.value === "all") {
        move(view.full.t0, view.full.t1);
        return;
      }
      const span = Number(event.target.value);
      if (!Number.isFinite(span)) return;
      const selected = model.byRef.get(view.selectedRef),
        center = selected
          ? KMP_LOOM.strictMs(selected, view.clock)
          : (view.t0 + view.t1) / 2;
      if (center !== null) move(center - span / 2, center + span / 2);
    });
    $("focus-selection").addEventListener("click", () => {
      const selected = model.byRef.get(view.selectedRef);
      if (!selected) return;
      const time = KMP_LOOM.strictMs(selected, view.clock);
      if (time === null) return;
      const span = Math.max(60000, (view.t1 - view.t0) * 0.2);
      move(time - span / 2, time + span / 2);
    });
  }
  return { refresh, wire };
})();
