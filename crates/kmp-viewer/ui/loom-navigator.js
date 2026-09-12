/* Canvas adapter for the shared whole-extent temporal brush. */
"use strict";
KMP_APP.navigator = (() => {
  const {model, view} = KMP_APP.state;
  const { $, fmtMs } = KMP_APP.dom;
  const palette = () => KMP_APP.theme.palette();
  const kindColor = kind => KMP_APP.theme.kindColor(kind);
  function drawNavigator() {
    KMP_APP.timeControls?.refresh();
    const strip = $("nav-canvas");
    if (!view.full) {
      const pen = strip.getContext("2d");
      if (pen) pen.clearRect(0, 0, strip.width, strip.height);
      return;
    }
    const width = strip.clientWidth || 1;
    const height = 42;
    const dpr = devicePixelRatio || 1;
    if (strip.width !== Math.round(width * dpr)) strip.width = Math.round(width * dpr);
    if (strip.height !== Math.round(height * dpr)) strip.height = Math.round(height * dpr);
    const pen = strip.getContext("2d");
    pen.setTransform(dpr, 0, 0, dpr, 0, 0);
    pen.clearRect(0, 0, width, height);
    const p = palette();
    const span = view.full.t1 - view.full.t0;
    const xAt = (t) => ((t - view.full.t0) / span) * width;

    // The navigator consumes the server's whole-extent atlas; it never bins
    // a hidden whole-about entry list in the browser.
    const cells = new Map();
    for (const bin of model.overviewBins) {
      const key = `${bin.from}\u0000${bin.to}`;
      if (!cells.has(key)) cells.set(key, { from: bin.from, to: bin.to, byKind: new Map() });
      const cell = cells.get(key);
      for (const [kind, count] of Object.entries(bin.by_kind || {})) {
        cell.byKind.set(kind, (cell.byKind.get(kind) || 0) + Number(count));
      }
    }
    const totals = [...cells.values()].map((cell) =>
      [...cell.byKind.values()].reduce((sum, count) => sum + count, 0)
    );
    const max = Math.max(1, ...totals);
    for (const cell of cells.values()) {
      const from = Date.parse(cell.from);
      const to = Date.parse(cell.to);
      if (!Number.isFinite(from) || !Number.isFinite(to)) continue;
      const x0 = xAt(from);
      const x1 = xAt(to);
      const barW = Math.max(1, x1 - x0);
      const total = [...cell.byKind.values()].reduce((sum, count) => sum + count, 0);
      if (!total) continue;
      const barH = Math.max(2, (total / max) * (height - 8));
      let y = height - 3;
      for (const [kind, count] of [...cell.byKind.entries()].sort((a, c) => c[1] - a[1])) {
        const sliceH = (count / total) * barH;
        pen.fillStyle = kindColor(kind);
        pen.globalAlpha = 0.85;
        pen.fillRect(x0 + 0.5, y - sliceH, Math.max(1, barW - 1), sliceH);
        y -= sliceH;
      }
    }
    pen.globalAlpha = 1;

    // The windowpane.
    const a = xAt(view.t0);
    const b = xAt(view.t1);
    // The pane follows the theme accent; it used to be a fixed blue that the
    // green-cast shell never wore in either theme.
    pen.fillStyle = p.pane || "rgba(72, 120, 224, 0.16)";
    pen.fillRect(a, 0, Math.max(2, b - a), height);
    pen.fillStyle = p.accent;
    pen.fillRect(a - 1, 0, 2, height);
    pen.fillRect(b - 1, 0, 2, height);
    pen.fillRect(a - 1, height / 2 - 5, 3, 10);
    pen.fillRect(b - 2, height / 2 - 5, 3, 10);

    $("nav-range").textContent = `${fmtMs(view.t0)} → ${fmtMs(view.t1)}`;
    $("nav-note").textContent = view.clock;
  }

  return { draw: drawNavigator };
})();
