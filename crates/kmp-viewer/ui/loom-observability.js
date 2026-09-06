/* Optional telemetry adapter. Exact values stay attached to points;
   sharing time does not assert a causal relation to a memory. */
"use strict";
KMP_APP.observability = (() => {
  const { model, view } = KMP_APP.state;
  const { $, el } = KMP_APP.dom;
  function svgNode(name, attributes = {}) {
    const node = document.createElementNS("http://www.w3.org/2000/svg", name);
    for (const [key, value] of Object.entries(attributes))
      node.setAttribute(key, String(value));
    return node;
  }
  function render() {
    const panel = $("pulse-chart");
    panel.replaceChildren();
    const series = KMP_LOOM.alignObservabilitySeries(
      model.observability.series || [],
      view.t0,
      view.t1,
    );
    panel.hidden = !series.length;
    series.forEach((series, index) => {
      const row = el("div", "pulse-series");
      row.append(el("span", "", `${series.name} · ${series.unit || ""}`));
      const svg = svgNode("svg", {
        viewBox: "0 0 820 40",
        preserveAspectRatio: "none",
        role: "img",
        "aria-label": `${series.name} on the shared time window`,
      });
      const points = series.points || [];
      const values = points.map((point) => Number(point.value)),
        lo = Math.min(...values),
        hi = Math.max(...values);
      const x = (point) =>
        KMP_APP.viewport.lens().toRatio(Number(point.at_millis)) * 820;
      const y = (point) =>
        34 - ((Number(point.value) - lo) / Math.max(1e-12, hi - lo)) * 28;
      const color = KMP_APP.scene.pulseColors()[index % 5];
      svg.append(
        svgNode("polyline", {
          points: points.map((point) => `${x(point)},${y(point)}`).join(" "),
          fill: "none",
          stroke: color,
          "stroke-width": 1.5,
        }),
      );
      for (const point of points) {
        const dot = svgNode("circle", {
          cx: x(point),
          cy: y(point),
          r: 2,
          fill: color,
        });
        const title = svgNode("title");
        title.textContent = `${new Date(Number(point.at_millis)).toISOString()} · ${point.value} ${series.unit || ""}`;
        dot.append(title);
        svg.append(dot);
      }
      row.append(svg);
      panel.append(row);
    });
    const exemplars = el("div", "pulse-exemplars");
    for (const item of model.observability.exemplars || []) {
      const button = el(
        "button",
        "",
        `${item.operation} · ${item.about || "unknown about"}`,
      );
      button.addEventListener("click", () => {
        if (item.bundle_ref && model.byRef.has(item.bundle_ref))
          KMP_APP.selection.selectEntry(item.bundle_ref);
        else
          KMP_APP.dom.showError(
            `${item.operation} · ${item.about || "unknown about"} · revision ${item.revision ?? "unavailable"}`,
          );
      });
      exemplars.append(button);
    }
    panel.append(exemplars);
  }
  return { render };
})();
