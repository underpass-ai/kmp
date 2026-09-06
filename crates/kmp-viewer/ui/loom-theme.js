/* DOM theme adapter. Shared palette for evidence, navigator and WebGL. */
"use strict";
KMP_APP.theme = (() => {
  const { $ } = KMP_APP.dom;
  const requestDraw = () => KMP_APP.scene.requestDraw();
  const THEMES = ["auto", "light", "dark"];
  let themeIndex = 2;
  let paletteCache = null;

  function applyTheme() {
    const choice = THEMES[themeIndex];
    const dark =
      choice === "dark" || (choice === "auto" && matchMedia("(prefers-color-scheme: dark)").matches);
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    $("btn-theme").textContent = choice[0].toUpperCase() + choice.slice(1);
    paletteCache = null;

    KMP_APP.panels.renderPulseLegend();
    requestDraw();
  }

  function palette() {
    if (!paletteCache) {
      const style = getComputedStyle(document.documentElement);
      const read = (name) => style.getPropertyValue(name).trim();
      paletteCache = {
        surface: read("--surface-1"),
        surface2: read("--surface-2"),
        text: read("--text-primary"),
        textMuted: read("--text-muted"),
        accent: read("--accent"),
        laneLine: read("--lane-line"),
        halo: read("--halo"),
        danger: read("--danger"),
        overflow: read("--kind-overflow"),
        kind: {
          memory_anchor: read("--kind-anchor"),
          about: read("--kind-anchor"),
          decision: read("--kind-decision"),
          memory_evidence: read("--kind-evidence"),
          evidence: read("--kind-evidence"),
          success_path: read("--kind-success"),
          error_path: read("--kind-error"),
          constraint: read("--kind-constraint"),
          observation: read("--kind-observation"),
          semantic_delta: read("--kind-delta"),
          preference: read("--kind-preference"),
          feedback: read("--kind-feedback"),
          memory_dimension: read("--kind-dimension"),
        },
        cls: {
          causal: read("--class-causal"),
          evidential: read("--class-evidential"),
          motivational: read("--class-motivational"),
          procedural: read("--class-procedural"),
          constraint: read("--class-constraint"),
          structural: read("--class-structural"),
        },
      };
    }
    return paletteCache;
  }

  const kindColor = (kind) => palette().kind[kind] || palette().overflow;
  const classColor = (cls) => palette().cls[cls] || palette().cls.structural;
  const pulseColors = (p = palette()) => [
    p.accent,
    p.cls.causal,
    p.cls.evidential,
    p.cls.constraint,
    p.danger,
  ];

  function wire() {
    $("btn-theme").addEventListener("click", () => {
      themeIndex = (themeIndex + 1) % THEMES.length;
      applyTheme();
      KMP_APP.scene.drawNavigator();
    });
    matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyTheme);
  }
  return {applyTheme, palette, kindColor, classColor, pulseColors, wire};
})();
