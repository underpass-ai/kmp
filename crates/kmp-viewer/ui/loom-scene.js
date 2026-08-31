/* KMP ChronoLoom — the scene.
   The rendering adapter: the vendored Pixi stage with its lanes, marks,
   braids, arcs and labels, the 2D-canvas navigator, the tooltip, the theme
   and its palette. Everything visual and nothing semantic — the state says
   what to draw, the viewport says where, this file only draws it.
   Exposes KMP_APP.scene. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.scene = (() => {
  const { model, view, entryAlpha, visibleLanes } = KMP_APP.state;
  const { $, el, fmtMs, fmtMsFull } = KMP_APP.dom;
  const xOf = (t) => KMP_APP.viewport.xOf(t);

  /* ---------------- theme & palette ---------------- */

  const THEMES = ["auto", "light", "dark"];
  let themeIndex = 0;
  let paletteCache = null;

  function applyTheme() {
    const choice = THEMES[themeIndex];
    const dark =
      choice === "dark" || (choice === "auto" && matchMedia("(prefers-color-scheme: dark)").matches);
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    $("btn-theme").textContent = choice[0].toUpperCase() + choice.slice(1);
    paletteCache = null;
    resetTextPools();
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

  /* ---------------- renderer ---------------- */

  const loomCanvas = $("loom-canvas");
  let app = null;
  let laneGfx = null;
  let validityGfx = null;
  let arcsGfx = null;
  let braidGfx = null;
  let marksGfx = null;
  let selectGfx = null;
  let overlayGfx = null;
  let textLayer = null;
  const textPools = { lane: new Map(), axis: [], mark: new Map(), bubble: new Map() };
  let dirty = true;
  let hitList = []; // {x, y, r, kind: "entry"|"cluster"|"exemplar", ...}

  const canvas = () => loomCanvas;

  function requestDraw() {
    dirty = true;
  }

  function resetTextPools() {
    if (!textLayer) return;
    for (const pool of [textPools.lane, textPools.mark, textPools.bubble]) {
      for (const label of pool.values()) label.destroy();
      pool.clear();
    }
    for (const label of textPools.axis) label.destroy();
    textPools.axis = [];
    textLayer.removeChildren();
    dirty = true;
  }

  async function setup() {
    if (typeof PIXI === "undefined") throw new Error("the vendored pixi.js bundle did not load");
    app = new PIXI.Application();
    await app.init({
      canvas: loomCanvas,
      resizeTo: $("stage"),
      backgroundAlpha: 0,
      antialias: true,
      autoDensity: true,
      resolution: devicePixelRatio || 1,
    });
    laneGfx = new PIXI.Graphics();
    validityGfx = new PIXI.Graphics();
    arcsGfx = new PIXI.Graphics();
    braidGfx = new PIXI.Graphics();
    marksGfx = new PIXI.Graphics();
    selectGfx = new PIXI.Graphics();
    overlayGfx = new PIXI.Graphics();
    textLayer = new PIXI.Container();
    app.stage.addChild(laneGfx, overlayGfx, validityGfx, arcsGfx, braidGfx, marksGfx, selectGfx, textLayer);
    app.renderer.on("resize", () => {
      dirty = true;
      drawNavigator();
      KMP_APP.data.scheduleProjection();
    });
    app.ticker.add(() => {
      if (dirty) {
        dirty = false;
        renderLoom();
      }
    });
  }

  /* Layout: an axis strip on top, lanes below it, the navigator floating at
     the bottom of the stage (its height is reserved). */
  const AXIS_H = 30;
  const PULSE_H = 52;
  const NAV_RESERVED = 96;
  /* How many entry labels the scene keeps baked. Text is the most expensive
     thing on the stage, so the pool is bounded — and evicts rather than
     simply refusing to grow. */
  const LABEL_POOL_MAX = 400;

  function laneGeometry() {
    const lanes = visibleLanes();
    const pulse = view.overlays.length ? PULSE_H : 0;
    const height = loomCanvas.clientHeight - AXIS_H - pulse - NAV_RESERVED;
    const laneH = lanes.length ? Math.max(44, Math.min(150, height / lanes.length)) : height;
    const tops = new Map();
    lanes.forEach((lane, i) => tops.set(lane.name, AXIS_H + pulse + i * laneH));
    return { lanes, laneH, tops, pulse };
  }

  function laneText(key, text, size, color, alpha) {
    let label = textPools.lane.get(key);
    if (!label) {
      label = new PIXI.Text({
        text,
        style: {
          fontFamily: "ui-monospace, monospace",
          fontSize: size,
          fill: color,
          stroke: { color: palette().halo, width: 3, join: "round" },
        },
        resolution: (devicePixelRatio || 1) * 2,
      });
      textPools.lane.set(key, label);
      textLayer.addChild(label);
    } else if (label.text !== text) {
      label.text = text;
    }
    label.alpha = alpha;
    label.visible = true;
    return label;
  }

  /* Class-styled arc between two points: a quadratic bow, dashed by pattern.
     Direction is a small arrowhead at the target end. */
  function drawArc(gfx, x1, y1, x2, y2, color, style, alphaScale) {
    const mx = (x1 + x2) / 2;
    const my = (y1 + y2) / 2;
    const dx = x2 - x1;
    const dy = y2 - y1;
    const len = Math.max(1, Math.hypot(dx, dy));
    const lift = Math.min(60, len * 0.25);
    const cx = mx - (dy / len) * lift;
    const cy = my + (dx / len) * lift;
    const alpha = style.alpha * alphaScale;
    if (alpha <= 0.02) return;

    const steps = Math.max(8, Math.min(48, Math.floor(len / 12)));
    const points = [];
    for (let i = 0; i <= steps; i += 1) {
      const t = i / steps;
      const a = 1 - t;
      points.push([
        a * a * x1 + 2 * a * t * cx + t * t * x2,
        a * a * y1 + 2 * a * t * cy + t * t * y2,
      ]);
    }
    if (!style.dash) {
      gfx.moveTo(points[0][0], points[0][1]);
      for (let i = 1; i < points.length; i += 1) gfx.lineTo(points[i][0], points[i][1]);
      gfx.stroke({ width: style.width, color, alpha });
    } else {
      // Walk the sampled curve, alternating pen-down segments per the
      // pattern.
      let patternIndex = 0;
      let remaining = style.dash[0];
      let penDown = true;
      let prev = points[0];
      for (let i = 1; i < points.length; i += 1) {
        let current = prev;
        let target = points[i];
        let segLen = Math.hypot(target[0] - current[0], target[1] - current[1]);
        while (segLen > 0.01) {
          const take = Math.min(segLen, remaining);
          const ratio = take / segLen;
          const nx = current[0] + (target[0] - current[0]) * ratio;
          const ny = current[1] + (target[1] - current[1]) * ratio;
          if (penDown) {
            gfx.moveTo(current[0], current[1]).lineTo(nx, ny);
          }
          current = [nx, ny];
          segLen -= take;
          remaining -= take;
          if (remaining <= 0.01) {
            penDown = !penDown;
            patternIndex = (patternIndex + 1) % style.dash.length;
            remaining = style.dash[patternIndex];
          }
        }
        prev = target;
      }
      gfx.stroke({ width: style.width, color, alpha });
    }
    if (style.arrow) {
      const [px, py] = points[points.length - 2];
      const angle = Math.atan2(y2 - py, x2 - px);
      const size = 5 + style.width;
      gfx
        .poly([
          x2,
          y2,
          x2 - size * Math.cos(angle - 0.4),
          y2 - size * Math.sin(angle - 0.4),
          x2 - size * Math.cos(angle + 0.4),
          y2 - size * Math.sin(angle + 0.4),
        ])
        .fill({ color, alpha });
    }
  }

  function renderLoom() {
    if (!app) return;
    if (!view.full) {
      // An empty clock still owns the canvas: the previous about's lanes,
      // marks and labels must not stand in for the one the user selected
      // (#421). Clear everything and let the empty-clock message speak.
      for (const layer of [laneGfx, validityGfx, arcsGfx, braidGfx, marksGfx, selectGfx, overlayGfx]) {
        if (layer) layer.clear();
      }
      hitList = [];
      resetTextPools();
      return;
    }
    const p = palette();
    const width = loomCanvas.clientWidth;
    const geometry = laneGeometry();
    const msPerPx = (view.t1 - view.t0) / Math.max(1, width);
    const lod = model.currentLod;
    $("lod-chip").textContent =
      lod === "atlas" ? "Atlas · density" : lod === "episode" ? "Episode · braids" : "Moment · entries";

    laneGfx.clear();
    validityGfx.clear();
    arcsGfx.clear();
    braidGfx.clear();
    marksGfx.clear();
    selectGfx.clear();
    overlayGfx.clear();
    hitList = [];
    const wanted = new Set();

    renderObservability(p, width, geometry.pulse);

    // Lanes: alternating quiet bands + name.
    geometry.lanes.forEach((lane, i) => {
      const top = geometry.tops.get(lane.name);
      if (i % 2 === 1) {
        laneGfx.rect(0, top, width, geometry.laneH).fill({ color: p.surface2, alpha: 0.35 });
      }
      laneGfx
        .moveTo(0, top + geometry.laneH)
        .lineTo(width, top + geometry.laneH)
        .stroke({ width: 1, color: p.laneLine, alpha: 0.8 });
      const key = `lane:${lane.name}`;
      wanted.add(key);
      const label = laneText(key, lane.name, 11, p.textMuted, 0.9);
      label.position.set(8, top + 6);
    });

    // Axis: gridlines through the lanes, labels in the strip above.
    const lens = KMP_APP.viewport.lens();
    const axis = KMP_LOOM.screenAxisTicks(lens, width, 110);
    let axisIndex = 0;
    for (const tick of axis.ticks) {
      const x = tick.ratio * width;
      laneGfx
        .moveTo(x, AXIS_H)
        .lineTo(x, loomCanvas.clientHeight - NAV_RESERVED)
        .stroke({ width: 1, color: p.laneLine, alpha: 0.6 });
      let label = textPools.axis[axisIndex];
      if (!label) {
        label = new PIXI.Text({
          text: "",
          style: { fontFamily: "ui-monospace, monospace", fontSize: 10, fill: p.textMuted },
          resolution: (devicePixelRatio || 1) * 2,
        });
        textPools.axis.push(label);
        textLayer.addChild(label);
      }
      label.text = KMP_LOOM.tickLabel(tick.time, tick.step);
      label.style.fill = p.textMuted;
      label.visible = true;
      label.anchor.set(0.5, 0);
      label.position.set(x, 8);
      axisIndex += 1;
    }
    for (let i = axisIndex; i < textPools.axis.length; i += 1) textPools.axis[i].visible = false;

    // Event-density is a narrative lens, never elapsed time in disguise.
    // Every compressed silence gets a visible double-slash scale break.
    for (const time of lens.breaks) {
      const x = xOf(time);
      laneGfx
        .moveTo(x - 5, AXIS_H - 7)
        .lineTo(x - 1, AXIS_H + 1)
        .moveTo(x + 1, AXIS_H - 7)
        .lineTo(x + 5, AXIS_H + 1)
        .stroke({ width: 1.5, color: p.danger, alpha: 0.9 });
    }

    for (const [pin, color] of [[view.pinA, p.accent], [view.pinB, p.cls.evidential]]) {
      if (!pin || pin.instant < view.t0 || pin.instant > view.t1) continue;
      const x = xOf(pin.instant);
      selectGfx
        .moveTo(x, AXIS_H)
        .lineTo(x, loomCanvas.clientHeight - NAV_RESERVED)
        .stroke({ width: 2, color, alpha: 0.7 });
    }

    const laneMid = (name) => {
      const top = geometry.tops.get(name);
      return top === undefined ? null : top + geometry.laneH / 2;
    };

    if (lod === "atlas") {
      renderAtlas(p, geometry, width);
    } else {
      renderWeave(p, geometry, width, lod, msPerPx, laneMid);
    }

    const clocked = model.entries.filter((entry) => KMP_LOOM.strictMs(entry, view.clock) !== null).length;
    if (model.currentLod === "moment" && model.entries.length && clocked === 0) {
      const key = "status:no-selected-clock";
      wanted.add(key);
      const label = laneText(
        key,
        `No entries carry the ${view.clock} clock · hollow marks use fallback placement`,
        13,
        p.danger,
        1
      );
      label.anchor.set(0.5, 0.5);
      label.position.set(width / 2, Math.max(AXIS_H + 24, (loomCanvas.clientHeight - NAV_RESERVED) / 2));
    }

    // Hide labels not used this frame.
    for (const [key, label] of textPools.lane) {
      if (!wanted.has(key)) label.visible = false;
    }
  }

  function renderObservability(p, width, pulseHeight) {
    if (!pulseHeight) return;
    const aligned = KMP_LOOM.alignObservabilitySeries(
      model.observability.series,
      view.t0,
      view.t1
    );
    const exemplars = new Map(
      (model.observability.exemplars || []).map((exemplar) => [exemplar.id, exemplar])
    );
    const colors = pulseColors(p);
    const top = AXIS_H + 5;
    const height = pulseHeight - 12;
    aligned.forEach((series, index) => {
      const color = colors[index % colors.length];
      const points = series.points.map((point) => [
        point.xRatio * width,
        top + height - point.yRatio * height,
        point,
      ]);
      if (points.length > 1) {
        overlayGfx.moveTo(points[0][0], points[0][1]);
        for (let i = 1; i < points.length; i += 1) overlayGfx.lineTo(points[i][0], points[i][1]);
        overlayGfx.stroke({ width: 1.5, color, alpha: 0.75 });
      }
      for (const [x, y, point] of points) {
        overlayGfx.circle(x, y, 3).fill({ color, alpha: 0.9 });
        hitList.push({
          x,
          y,
          r: 6,
          kind: "exemplar",
          series: series.name,
          unit: series.unit,
          scope: series.scope,
          value: point.value,
          at: point.at_millis,
          exemplar: exemplars.get(point.exemplar_id),
        });
      }
    });
    overlayGfx
      .moveTo(0, AXIS_H + pulseHeight)
      .lineTo(width, AXIS_H + pulseHeight)
      .stroke({ width: 1, color: p.laneLine, alpha: 0.8 });
  }

  /* Atlas: density ribbons per lane — the shape of the memory, no nodes. */
  function renderAtlas(p, geometry, width) {
    const max = Math.max(1, ...model.bins.map((bin) => Number(bin.total || 0)));
    for (const lane of geometry.lanes) {
      const base = geometry.tops.get(lane.name) + geometry.laneH - 6;
      const maxH = geometry.laneH - 16;
      for (const bin of model.bins.filter((item) => item.dimension === lane.name)) {
        const from = Date.parse(bin.from);
        const to = Date.parse(bin.to);
        if (!Number.isFinite(from) || !Number.isFinite(to) || !bin.total) continue;
        const x0 = xOf(from);
        const x1 = xOf(to);
        const barW = Math.max(1, x1 - x0);
        const barH = Math.max(2, (bin.total / max) * maxH);
        let y = base;
        const slices = Object.entries(bin.by_kind || {}).sort((a, c) => c[1] - a[1]);
        for (const [kind, count] of slices) {
          const sliceH = (count / bin.total) * barH;
          marksGfx
            .rect(x0 + 0.5, y - sliceH, Math.max(1, barW - 1), sliceH)
            .fill({ color: kindColor(kind), alpha: 0.8 });
          y -= sliceH;
        }
        hitList.push({
          x: x0 + barW / 2,
          y: base - barH / 2,
          r: Math.max(barW, 10),
          kind: "cluster",
          refs: null,
          count: bin.total,
          t0: from,
          t1: to,
        });
      }
    }
  }

  /* Episode & moment: entries (or tight clusters) on their lanes, braided. */
  function renderWeave(p, geometry, width, lod, msPerPx, laneMid) {
    const minGap = lod === "episode" ? 18 * msPerPx : 0;
    const clusters = new Map();
    if (lod === "episode") {
      for (const cluster of model.clusters) {
        const from = Date.parse(cluster.from);
        const to = Date.parse(cluster.to);
        if (!Number.isFinite(from) || !Number.isFinite(to)) continue;
        if (!clusters.has(cluster.dimension)) clusters.set(cluster.dimension, []);
        clusters.get(cluster.dimension).push({
          refs: cluster.refs || [],
          total: Number(cluster.total || 0),
          t: (from + to) / 2,
          from,
          to,
          strictCount: Number(cluster.total || 0),
          byKind: new Map(Object.entries(cluster.by_kind || {})),
        });
      }
    } else {
      for (const entry of model.entries) {
        const t = KMP_LOOM.placedMs(entry, view.clock);
        if (t === null || t < view.t0 || t > view.t1) continue;
        for (const dimension of new Set(entry.coords.map((coordinate) => coordinate.dimension))) {
          if (!clusters.has(dimension)) clusters.set(dimension, []);
          clusters.get(dimension).push({
            refs: [entry.ref],
            total: 1,
            t,
            from: t,
            to: t,
            strictCount: KMP_LOOM.strictMs(entry, view.clock) === null ? 0 : 1,
            byKind: new Map([[entry.kind, 1]]),
          });
        }
      }
    }
    const drawnAt = new Map(); // ref -> [{x, y}]
    const wantedBubbles = new Set();

    for (const lane of geometry.lanes) {
      const laneClusters = clusters.get(lane.name) || [];
      const y = laneMid(lane.name);
      if (y === null) continue;
      for (const cluster of laneClusters) {
        const x = xOf(cluster.t);
        if (x < -40 || x > width + 40) continue;
        if (lod === "episode" || cluster.total > 1) {
          // A bundle of memory too tight to split at this zoom.
          const r = Math.min(18, 7 + 2.5 * Math.log2(cluster.refs.length + 1));
          const slices = [...cluster.byKind.entries()].sort((a, c) => c[1] - a[1]);
          const allFallback = cluster.strictCount === 0;
          if (allFallback) {
            marksGfx.circle(x, y, r).stroke({ width: 2, color: p.textMuted, alpha: 0.9 });
          } else {
            marksGfx.circle(x, y, r).fill({ color: p.surface, alpha: 0.9 });
          }
          let angle = -Math.PI / 2;
          const total = cluster.total;
          for (const [kind, count] of slices) {
            const sweep = (count / total) * Math.PI * 2;
            // moveTo first: a bare arc() drags a line in from the pen's last
            // resting place, which after a stroke is the origin.
            marksGfx
              .moveTo(x + (r - 1.5) * Math.cos(angle), y + (r - 1.5) * Math.sin(angle))
              .arc(x, y, r - 1.5, angle, angle + sweep)
              .stroke({ width: 3, color: kindColor(kind), alpha: 0.95 });
            angle += sweep;
          }
          const key = `bubble:${lane.name}:${Math.round(x)}`;
          wantedBubbles.add(key);
          let label = textPools.bubble.get(key);
          if (!label) {
            label = new PIXI.Text({
              text: "",
              style: { fontFamily: "ui-monospace, monospace", fontSize: 10, fill: p.text },
              resolution: (devicePixelRatio || 1) * 2,
            });
            textPools.bubble.set(key, label);
            textLayer.addChild(label);
          }
          label.text = String(total);
          label.style.fill = p.text;
          label.anchor.set(0.5, 0.5);
          label.position.set(x, y);
          label.visible = true;
          hitList.push({
            x,
            y,
            r: r + 4,
            kind: "cluster",
            refs: cluster.refs,
            count: total,
            t0: cluster.from - Math.max(minGap, 1),
            t1: cluster.to + Math.max(minGap, 1),
          });
        } else {
          const ref = cluster.refs[0];
          const m = model.byRef.get(ref);
          const a = entryAlpha(m);
          if (a <= 0.02) continue;
          const strict = KMP_LOOM.strictMs(m, view.clock) !== null;
          const r = view.selectedRef === ref ? 7 : 5.5;
          if (strict) {
            marksGfx.circle(x, y, r).fill({ color: kindColor(m.kind), alpha: a });
          } else {
            // Hollow: sitting at its precedence fallback, not its own clock.
            marksGfx.circle(x, y, r).stroke({ width: 2, color: kindColor(m.kind), alpha: a });
          }
          if (model.contradictedRefs.has(m.ref)) {
            marksGfx
              .poly([x - 4, y - r - 3, x + 4, y - r - 3, x, y - r - 9])
              .fill({ color: p.danger, alpha: a });
          }
          let bucket = drawnAt.get(ref);
          if (!bucket) drawnAt.set(ref, (bucket = []));
          bucket.push({ x, y });
          hitList.push({ x, y, r: r + 4, kind: "entry", ref });
        }
      }
    }

    // Braids: one entry woven through several lanes — a vertical thread.
    for (const [ref, spots] of drawnAt) {
      if (spots.length < 2) continue;
      const a = entryAlpha(model.byRef.get(ref)) * 0.7;
      const sorted = [...spots].sort((s, t) => s.y - t.y);
      for (let i = 1; i < sorted.length; i += 1) {
        braidGfx
          .moveTo(sorted[i - 1].x, sorted[i - 1].y)
          .lineTo(sorted[i].x, sorted[i].y)
          .stroke({ width: 1.2, color: p.accent, alpha: a * 0.5 });
      }
    }

    // Validity: on the validity clock, facts stretch for as long as they
    // held.
    if (view.clock === "validity") {
      for (const m of model.entries) {
        const from = m.clocks.validFrom;
        if (from === null) continue;
        const until = m.clocks.validUntil ?? view.t1;
        const seen = new Set();
        for (const coord of m.coords) {
          if (seen.has(coord.dimension)) continue;
          seen.add(coord.dimension);
          const y = laneMid(coord.dimension);
          if (y === null) continue;
          validityGfx
            .rect(xOf(from), y - 2, Math.max(2, xOf(until) - xOf(from)), 4)
            .fill({ color: kindColor(m.kind), alpha: 0.25 * entryAlpha(m) });
        }
      }
    }

    // Arcs: explanatory relations between drawn entries, styled by class.
    if (lod === "moment" || view.trace) {
      for (const edge of model.edges) {
        const from = drawnAt.get(edge.source);
        const to = drawnAt.get(edge.target);
        if (!from || !to) continue;
        const inTrace = view.trace && view.trace.edgeKeys.has(`${edge.source} ${edge.rel} ${edge.target}`);
        if (lod !== "moment" && !inTrace) continue;
        const style = KMP_LOOM.arcStyle(edge.class);
        const aScale = view.trace ? (inTrace ? 1.4 : 0.15) : 1;
        drawArc(
          arcsGfx,
          from[0].x,
          from[0].y,
          to[0].x,
          to[0].y,
          inTrace ? p.accent : classColor(edge.class),
          style,
          Math.min(entryAlpha(model.byRef.get(edge.source)), entryAlpha(model.byRef.get(edge.target))) * aScale
        );
      }
      // Supersession: a quiet grey tie from the replacement to its history.
      for (const edge of model.supersessions) {
        const from = drawnAt.get(edge.source);
        const to = drawnAt.get(edge.target);
        if (!from || !to) continue;
        drawArc(arcsGfx, from[0].x, from[0].y, to[0].x, to[0].y, p.textMuted, { dash: [2, 4], width: 1, alpha: 0.4, arrow: false }, 1);
      }
      // Contradiction: both claims alive — a danger-colored zigzag between
      // them.
      for (const edge of model.contradictions) {
        const from = drawnAt.get(edge.source);
        const to = drawnAt.get(edge.target);
        if (!from || !to) continue;
        drawArc(arcsGfx, from[0].x, from[0].y, to[0].x, to[0].y, p.danger, { dash: [4, 3], width: 1.8, alpha: 0.9, arrow: false }, 1);
      }
    }

    // Selection ring + glow.
    if (view.selectedRef) {
      const spots = drawnAt.get(view.selectedRef);
      if (spots) {
        for (const spot of spots) {
          selectGfx.circle(spot.x, spot.y, 10).stroke({ width: 2.5, color: p.accent, alpha: 1 });
          selectGfx.circle(spot.x, spot.y, 14).stroke({ width: 6, color: p.accent, alpha: 0.18 });
        }
      }
    }

    // Entry labels, only when there is room to own the words.
    const showLabels = msPerPx < 4000;
    for (const [key, label] of textPools.mark) label.visible = false;
    if (showLabels) {
      for (const item of hitList) {
        if (item.kind !== "entry") continue;
        const m = model.byRef.get(item.ref);
        if (entryAlpha(m) < 0.2) continue;
        const key = `mark:${item.ref}`;
        let label = textPools.mark.get(key);
        const text = m.text.length > 34 ? m.text.slice(0, 33) + "…" : m.text;
        if (!label && textPools.mark.size >= LABEL_POOL_MAX) {
          // Evict, do not stop labelling: a bare cap meant that once the
          // pool filled, entries stopped getting names for the rest of the
          // session.
          for (const [oldKey, oldLabel] of textPools.mark) {
            if (oldLabel.visible) continue;
            oldLabel.destroy();
            textPools.mark.delete(oldKey);
            break;
          }
        }
        if (!label) {
          label = new PIXI.Text({
            text,
            style: {
              fontFamily: "system-ui, sans-serif",
              fontSize: 11,
              fill: palette().text,
              stroke: { color: palette().halo, width: 3, join: "round" },
            },
            resolution: (devicePixelRatio || 1) * 2,
          });
          textPools.mark.set(key, label);
          textLayer.addChild(label);
        }
        label.alpha = entryAlpha(m);
        label.visible = true;
        label.anchor.set(0, 0.5);
        label.position.set(item.x + 10, item.y - 12);
      }
    }

    for (const [key, label] of textPools.bubble) {
      if (!wantedBubbles.has(key)) label.visible = false;
    }
  }

  /* ---------------- the navigator ---------------- */

  function drawNavigator() {
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
    pen.fillStyle = "rgba(72, 120, 224, 0.16)";
    pen.fillRect(a, 0, Math.max(2, b - a), height);
    pen.fillStyle = p.accent;
    pen.fillRect(a - 1, 0, 2, height);
    pen.fillRect(b - 1, 0, 2, height);
    pen.fillRect(a - 1, height / 2 - 5, 3, 10);
    pen.fillRect(b - 2, height / 2 - 5, 3, 10);

    $("nav-range").textContent = `${fmtMs(view.t0)} → ${fmtMs(view.t1)}`;
    $("nav-note").textContent = view.clock;
  }

  /* ---------------- hits & tooltip ---------------- */

  function hitAt(x, y) {
    let best = null;
    let bestDist = Infinity;
    for (const item of hitList) {
      const dist = Math.hypot(item.x - x, item.y - y);
      if (dist <= item.r + 3 && dist < bestDist) {
        best = item;
        bestDist = dist;
      }
    }
    return best;
  }

  function updateTooltip(hit, sx, sy) {
    const tooltip = $("tooltip");
    tooltip.textContent = "";
    if (!hit) {
      tooltip.hidden = true;
      return;
    }
    if (hit.kind === "cluster") {
      tooltip.append(el("div", "tt-title", `${hit.count} entries`));
      tooltip.append(el("div", "tt-sub", "click to open this stretch of the weave"));
    } else if (hit.kind === "exemplar") {
      tooltip.append(
        el(
          "div",
          "tt-title",
          `${hit.series}: ${KMP_LOOM.formatMetricValue(hit.value, hit.unit)} ${hit.unit}`
        )
      );
      tooltip.append(el("div", "tt-sub", `${hit.scope} · ${fmtMsFull(hit.at)}`));
      if (hit.exemplar) {
        const revision = hit.exemplar.revision == null ? "revision unavailable" : `revision ${hit.exemplar.revision}`;
        tooltip.append(
          el("div", "tt-quote", `${hit.exemplar.operation} · ${hit.exemplar.about || "unknown bundle"} · ${revision}`)
        );
      }
    } else {
      const m = model.byRef.get(hit.ref);
      tooltip.append(el("div", "tt-title", m.text.length > 90 ? m.text.slice(0, 89) + "…" : m.text));
      const strict = KMP_LOOM.strictMs(m, view.clock) !== null;
      tooltip.append(
        el("div", "tt-sub", `${m.kind}${strict ? "" : " · placed by fallback — no " + view.clock + " clock"}`)
      );
      const t = KMP_LOOM.placedMs(m, view.clock);
      if (t !== null) tooltip.append(el("div", "tt-sub", fmtMsFull(t)));
      if (model.supersededRefs.has(m.ref)) tooltip.append(el("div", "tt-quote", "superseded — history, still true then"));
      if (model.contradictedRefs.has(m.ref)) tooltip.append(el("div", "tt-quote", "contradicted — both claims still alive"));
    }
    tooltip.hidden = false;
    const stage = $("stage").getBoundingClientRect();
    tooltip.style.left = `${Math.max(4, Math.min(sx + 14, stage.width - tooltip.offsetWidth - 8))}px`;
    tooltip.style.top = `${Math.max(4, Math.min(sy + 14, stage.height - tooltip.offsetHeight - 8))}px`;
  }

  /* ---------------- theme wiring ---------------- */

  function wire() {
    $("btn-theme").addEventListener("click", () => {
      themeIndex = (themeIndex + 1) % THEMES.length;
      applyTheme();
    });
    matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyTheme);
  }

  return {
    canvas,
    setup,
    wire,
    applyTheme,
    palette,
    kindColor,
    classColor,
    pulseColors,
    requestDraw,
    resetTextPools,
    drawNavigator,
    hitAt,
    updateTooltip,
  };
})();
