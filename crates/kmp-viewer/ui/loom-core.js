/* KMP ChronoLoom — loom core.
   Pure logic only: no DOM, no PIXI, no fetch. Time is the primary geometry;
   everything here turns entries, coordinates and relations into positions,
   lanes, bins, clusters, prisms and axes — plain data in, plain data out.
   Loaded before loom.js; exposes a single global, KMP_LOOM. */
"use strict";

const KMP_LOOM = (() => {
  /* ---------------- clocks ----------------
     KMP has no single clock. Every coordinate may carry occurred, observed,
     ingested and a validity interval; the loom lets the reader choose which
     one is the x-axis, and never invents a time an entry does not carry. */

  const CLOCKS = ["occurred", "observed", "ingested", "validity"];

  const parseMs = (value) => (value ? Date.parse(value) : null);

  function entryModel(entry) {
    const coords = (entry.coordinates || []).map((c) => ({
      dimension: c.dimension,
      scope: c.scope_id,
      sequence: c.sequence === undefined ? null : c.sequence,
      rank: c.rank === undefined ? null : c.rank,
      occurred: parseMs(c.occurred_at),
      observed: parseMs(c.observed_at),
      ingested: parseMs(c.ingested_at),
      validFrom: parseMs(c.valid_from),
      validUntil: parseMs(c.valid_until),
    }));
    const earliest = (field) =>
      coords.reduce((best, c) => (c[field] !== null && (best === null || c[field] < best) ? c[field] : best), null);
    const latest = (field) =>
      coords.reduce((best, c) => (c[field] !== null && (best === null || c[field] > best) ? c[field] : best), null);
    return {
      ref: entry.ref_id,
      kind: entry.kind,
      text: entry.text || "",
      coords,
      clocks: {
        occurred: earliest("occurred"),
        observed: earliest("observed"),
        ingested: earliest("ingested"),
        validFrom: earliest("validFrom"),
        validUntil: latest("validUntil"),
      },
    };
  }

  /* The domain's own precedence, used for ordering and for the honest
     fallback position of an entry that lacks the selected clock. */
  function fallbackMs(model) {
    const c = model.clocks;
    const t = c.occurred ?? c.validFrom ?? c.observed ?? c.ingested;
    return t === undefined ? null : t;
  }

  /* Strict reading of the selected clock — null when the entry does not
     carry it. Validity reads valid_from. */
  function strictMs(model, clock) {
    if (clock === "validity") return model.clocks.validFrom;
    return model.clocks[clock];
  }

  /* Where the entry sits on the axis: its own clock, or — hollow-marked by
     the renderer — the precedence fallback. */
  function placedMs(model, clock) {
    const t = strictMs(model, clock);
    return t !== null ? t : fallbackMs(model);
  }

  function minSequence(model) {
    return model.coords.reduce(
      (best, c) => (c.sequence !== null && c.sequence < best ? c.sequence : best),
      Number.MAX_SAFE_INTEGER
    );
  }

  function compareModels(a, b) {
    const ta = fallbackMs(a);
    const tb = fallbackMs(b);
    if (ta !== tb) return (ta ?? Infinity) - (tb ?? Infinity);
    const sa = minSequence(a);
    const sb = minSequence(b);
    if (sa !== sb) return sa - sb;
    return a.ref < b.ref ? -1 : a.ref > b.ref ? 1 : 0;
  }

  /* ---------------- lanes ----------------
     Dimensions are stable lanes; scopes group inside their dimension. Order
     is first-appearance so the map does not reshuffle underfoot. */

  function buildLanes(models) {
    const lanes = new Map(); // dimension -> {name, index, count, scopes: Map(scope -> count)}
    for (const model of models) {
      for (const coord of model.coords) {
        let lane = lanes.get(coord.dimension);
        if (!lane) {
          lanes.set(coord.dimension, (lane = { name: coord.dimension, index: lanes.size, count: 0, scopes: new Map() }));
        }
        lane.count += 1;
        lane.scopes.set(coord.scope, (lane.scopes.get(coord.scope) || 0) + 1);
      }
    }
    return [...lanes.values()];
  }

  /* ---------------- extent, bins, clusters ---------------- */

  function extent(models, clock) {
    let t0 = null;
    let t1 = null;
    for (const model of models) {
      const t = placedMs(model, clock);
      if (t === null) continue;
      if (t0 === null || t < t0) t0 = t;
      if (t1 === null || t > t1) t1 = t;
      if (clock === "validity" && model.clocks.validUntil !== null && model.clocks.validUntil > t1) {
        t1 = model.clocks.validUntil;
      }
    }
    if (t0 === null) return null;
    if (t0 === t1) t1 = t0 + 1;
    return { t0, t1 };
  }

  /* Per-lane density over a window: bins of {total, byKind}. Atlas fabric. */
  function laneBins(models, clock, laneIndexOf, laneCount, t0, t1, bucketCount) {
    const grid = Array.from({ length: laneCount }, () =>
      Array.from({ length: bucketCount }, () => ({ total: 0, byKind: new Map() }))
    );
    const span = Math.max(1, t1 - t0);
    for (const model of models) {
      const t = placedMs(model, clock);
      if (t === null || t < t0 || t > t1) continue;
      const b = Math.min(bucketCount - 1, Math.floor(((t - t0) / span) * bucketCount));
      for (const coord of model.coords) {
        const lane = laneIndexOf(coord.dimension);
        if (lane === undefined) continue;
        const cell = grid[lane][b];
        cell.total += 1;
        cell.byKind.set(model.kind, (cell.byKind.get(model.kind) || 0) + 1);
      }
    }
    let max = 1;
    for (const row of grid) for (const cell of row) if (cell.total > max) max = cell.total;
    return { grid, max };
  }

  /* Episode fabric: entries within the window grouped per lane into clusters
     no tighter than `minGapMs`. A cluster of one is just the entry. */
  function laneClusters(models, clock, t0, t1, minGapMs) {
    const perLane = new Map(); // dimension -> [{t, refs, byKind}]
    const windowed = models
      .map((model) => ({ model, t: placedMs(model, clock) }))
      .filter((x) => x.t !== null && x.t >= t0 && x.t <= t1)
      .sort((a, b) => a.t - b.t);
    for (const { model, t } of windowed) {
      const seen = new Set();
      for (const coord of model.coords) {
        if (seen.has(coord.dimension)) continue;
        seen.add(coord.dimension);
        let clusters = perLane.get(coord.dimension);
        if (!clusters) perLane.set(coord.dimension, (clusters = []));
        const last = clusters[clusters.length - 1];
        if (last && t - last.tLast <= minGapMs) {
          last.refs.push(model.ref);
          last.tLast = t;
          last.tSum += t;
          last.byKind.set(model.kind, (last.byKind.get(model.kind) || 0) + 1);
        } else {
          clusters.push({ refs: [model.ref], tLast: t, tSum: t, byKind: new Map([[model.kind, 1]]) });
        }
      }
    }
    for (const clusters of perLane.values()) {
      for (const cluster of clusters) cluster.t = cluster.tSum / cluster.refs.length;
    }
    return perLane;
  }

  /* ---------------- semantic zoom ladder ----------------
     The zoom changes representation, not just size. Thresholds are explicit
     milliseconds-per-pixel; evidence is a selection state, not a zoom. */

  function lodFor(msPerPx) {
    if (msPerPx > 600e3) return "atlas"; // > 10 min per pixel: weeks on screen
    if (msPerPx > 20e3) return "episode"; // > 20 s per pixel: hours on screen
    return "moment";
  }

  /* ---------------- axis ---------------- */

  const AXIS_STEPS = [
    1e3, 5e3, 15e3, 30e3,
    60e3, 5 * 60e3, 15 * 60e3, 30 * 60e3,
    3600e3, 3 * 3600e3, 6 * 3600e3, 12 * 3600e3,
    86400e3, 2 * 86400e3, 7 * 86400e3, 14 * 86400e3, 30 * 86400e3, 90 * 86400e3, 365 * 86400e3,
  ];

  function axisTicks(t0, t1, maxTicks) {
    const span = Math.max(1, t1 - t0);
    let step = AXIS_STEPS[AXIS_STEPS.length - 1];
    for (const candidate of AXIS_STEPS) {
      if (span / candidate <= maxTicks) {
        step = candidate;
        break;
      }
    }
    const ticks = [];
    for (let t = Math.ceil(t0 / step) * step; t <= t1; t += step) ticks.push(t);
    return { step, ticks };
  }

  function tickLabel(ms, step) {
    const iso = new Date(ms).toISOString();
    if (step >= 86400e3) return iso.slice(5, 10);
    if (step >= 60e3) return iso.slice(11, 16);
    return iso.slice(11, 19);
  }

  /* ---------------- relations ----------------
     Classes carry meaning; color is never the only channel — each class has
     its own dash pattern and weight, and structural threads fade first. */

  const CLASS_STYLE = {
    causal: { dash: null, width: 2, alpha: 0.9, arrow: true },
    evidential: { dash: [5, 4], width: 1.5, alpha: 0.8, arrow: true },
    motivational: { dash: [1, 3], width: 1.5, alpha: 0.8, arrow: true },
    procedural: { dash: [7, 2, 2, 2], width: 1.3, alpha: 0.7, arrow: true },
    constraint: { dash: [2, 2], width: 1.8, alpha: 0.7, arrow: false },
    structural: { dash: null, width: 0.7, alpha: 0.3, arrow: false },
  };

  function arcStyle(cls) {
    return CLASS_STYLE[cls] || CLASS_STYLE.structural;
  }

  /* Edges whose ends are both drawn entries; supersession and contradiction
     keep their distinct nature instead of melting into one problem color. */
  function classifyEdges(edges, hasRef) {
    const arcs = [];
    const supersessions = [];
    const contradictions = [];
    for (const edge of edges) {
      if (!hasRef(edge.source) || !hasRef(edge.target)) continue;
      if (edge.rel === "supersedes") supersessions.push(edge);
      else if (edge.rel === "contradicts") contradictions.push(edge);
      else arcs.push(edge);
    }
    return { arcs, supersessions, contradictions };
  }

  /* ---------------- the polytemporal prism ----------------
     One entry, four rails: reality (occurred + validity), perception
     (observed), persistence (ingested), order (sequence/rank per scope).
     Absent clocks stay absent — an incomplete rail is information. */

  function prism(model) {
    const c = model.clocks;
    const stamps = [c.occurred, c.observed, c.ingested, c.validFrom, c.validUntil].filter(
      (t) => t !== null
    );
    const span = stamps.length
      ? { t0: Math.min(...stamps), t1: Math.max(...stamps) }
      : null;
    if (span && span.t0 === span.t1) span.t1 = span.t0 + 1;
    return {
      span,
      rails: {
        occurred: c.occurred,
        observed: c.observed,
        ingested: c.ingested,
        validity: c.validFrom !== null || c.validUntil !== null
          ? { from: c.validFrom, until: c.validUntil }
          : null,
      },
      order: model.coords.map((coord) => ({
        dimension: coord.dimension,
        scope: coord.scope,
        sequence: coord.sequence,
        rank: coord.rank,
      })),
    };
  }

  /* ---------------- search ---------------- */

  function parseQuery(raw) {
    const query = { text: [], kind: null, dim: null, id: null };
    for (const token of raw.trim().toLowerCase().split(/\s+/).filter(Boolean)) {
      if (token.startsWith("kind:")) query.kind = token.slice(5);
      else if (token.startsWith("dim:")) query.dim = token.slice(4);
      else if (token.startsWith("id:")) query.id = token.slice(3);
      else query.text.push(token);
    }
    query.empty = !query.text.length && !query.kind && !query.dim && !query.id;
    return query;
  }

  function matchesQuery(query, fields) {
    if (query.empty) return false;
    if (query.kind && !fields.kind.includes(query.kind)) return false;
    if (query.id && !fields.id.includes(query.id)) return false;
    if (query.dim && !(fields.dim || "").includes(query.dim)) return false;
    for (const term of query.text) {
      if (!fields.text.includes(term) && !fields.id.includes(term)) return false;
    }
    return true;
  }

  return {
    CLOCKS,
    entryModel,
    fallbackMs,
    strictMs,
    placedMs,
    compareModels,
    buildLanes,
    extent,
    laneBins,
    laneClusters,
    lodFor,
    axisTicks,
    tickLabel,
    arcStyle,
    classifyEdges,
    prism,
    parseQuery,
    matchesQuery,
  };
})();
