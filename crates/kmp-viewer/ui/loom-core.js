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

  /* ---------------- semantic zoom ladder ----------------
     The zoom changes representation, not just size. Thresholds are explicit
     milliseconds-per-pixel; evidence is a selection state, not a zoom. */

  function lodFor(msPerPx) {
    if (msPerPx > 600e3) return "atlas"; // > 10 min per pixel: weeks on screen
    if (msPerPx > 20e3) return "episode"; // > 20 s per pixel: hours on screen
    return "moment";
  }

  /* Observability shares the temporal axis but not a value axis. Each series
     is normalized only for drawing inside its own labelled strip; the exact
     value, unit and scope remain attached for selection and inspection. */
  function alignObservabilitySeries(series, t0, t1) {
    return (series || []).map((item) => {
      const points = (item.points || [])
        .filter((point) => point.at_millis >= t0 && point.at_millis <= t1)
        .sort((a, b) => a.at_millis - b.at_millis);
      const values = points.map((point) => point.value);
      const min = values.length ? Math.min(...values) : 0;
      const max = values.length ? Math.max(...values) : 0;
      const span = Math.max(Number.EPSILON, max - min);
      return {
        name: item.name,
        unit: item.unit,
        scope: item.scope,
        min,
        max,
        points: points.map((point) => ({
          ...point,
          xRatio: (point.at_millis - t0) / Math.max(1, t1 - t0),
          yRatio: max === min ? 0.5 : (point.value - min) / span,
        })),
      };
    });
  }

  /* Units determine honest display precision. A ratio is not made more
     certain by a binary float's long tail, while an exact token count should
     never grow decimals. */
  function formatMetricValue(value, unit) {
    const number = Number(value);
    if (!Number.isFinite(number)) return "—";
    const normalized = String(unit || "").trim().toLowerCase();
    if (["token", "tokens", "entry", "entries", "event", "events", "count"].includes(normalized)) {
      return Math.round(number).toLocaleString("en-US");
    }
    if (normalized === "ratio") return number.toFixed(2);
    if (normalized === "%" || normalized === "percent" || normalized === "percentage") {
      return number.toFixed(1);
    }
    if (["ms", "millisecond", "milliseconds"].includes(normalized)) {
      if (Math.abs(number) >= 100) return number.toFixed(0);
      if (Math.abs(number) >= 10) return number.toFixed(1);
      return number.toFixed(2);
    }
    if (Number.isInteger(number)) return number.toLocaleString("en-US");
    return Number(number.toPrecision(4)).toString();
  }

  /* A monotone, invertible time transform. Elapsed time is proportional.
     Event density caps long silent gaps and reports every compressed segment
     as a break so a renderer can never pass narrative spacing off as elapsed
     duration. A focus interval may reserve 70% of the axis while keeping both
     contexts visible. */
  function temporalLens({ mode = "elapsed", t0, t1, events = [], focus = null }) {
    const start = Math.min(t0, t1);
    const end = Math.max(t0 + 1, t1);
    const eventTimes = [...new Set(events.filter((t) => t >= start && t <= end))].sort((a, b) => a - b);
    let knots = [start, ...eventTimes, end].filter((t, index, all) => index === 0 || t > all[index - 1]);
    if (knots.length < 2) knots = [start, end];
    const gaps = knots.slice(1).map((t, index) => t - knots[index]);
    const positive = gaps.filter((gap) => gap > 0).sort((a, b) => a - b);
    const median = positive.length ? positive[Math.floor(positive.length / 2)] : end - start;
    const silenceCap = Math.max(1, median * 3);
    let compressedGaps = gaps.map((gap) => mode === "event_density" && gap > silenceCap);
    let weights = gaps.map((gap) => (mode === "event_density" ? Math.min(gap, silenceCap) : gap));

    const focusFrom = focus ? Math.max(start, Math.min(end, focus.from)) : null;
    const focusTo = focus ? Math.max(start, Math.min(end, focus.to)) : null;
    const hasFocus = focusFrom !== null && focusTo !== null && focusFrom < focusTo;
    if (hasFocus) {
      knots = [...new Set([...knots, focusFrom, focusTo])]
        .filter((t) => t >= start && t <= end)
        .sort((a, b) => a - b);
      const pieces = knots.slice(1).map((to, index) => {
        const from = knots[index];
        const mid = (from + to) / 2;
        const region = mid < focusFrom ? "left" : mid > focusTo ? "right" : "focus";
        const gap = to - from;
        return {
          gap,
          region,
          effective: mode === "event_density" ? Math.min(gap, silenceCap) : gap,
        };
      });
      const totals = { left: 0, focus: 0, right: 0 };
      for (const piece of pieces) totals[piece.region] += piece.effective;
      const shares = { left: 0.15, focus: 0.7, right: 0.15 };
      weights = pieces.map(
        (piece) => (piece.effective / Math.max(1, totals[piece.region])) * shares[piece.region]
      );
      compressedGaps = pieces.map(
        (piece) => mode === "event_density" && piece.gap > silenceCap
      );
    }

    const total = Math.max(Number.EPSILON, weights.reduce((sum, weight) => sum + weight, 0));
    let cursor = 0;
    const segments = weights.map((weight, index) => {
      const u0 = cursor / total;
      cursor += weight;
      const gap = knots[index + 1] - knots[index];
      return {
        t0: knots[index],
        t1: knots[index + 1],
        u0,
        u1: cursor / total,
        compressed: compressedGaps[index],
      };
    });
    const locateTime = (time) =>
      segments.find((segment) => time <= segment.t1) || segments[segments.length - 1];
    const locateRatio = (ratio) =>
      segments.find((segment) => ratio <= segment.u1) || segments[segments.length - 1];
    return {
      mode,
      segments,
      breaks: [
        ...segments
          .filter((segment) => segment.compressed)
          .flatMap((segment) => [segment.t0, segment.t1]),
        ...(hasFocus ? [focusFrom, focusTo] : []),
      ]
        .filter((time, index, all) => time > start && time < end && all.indexOf(time) === index),
      toRatio(time) {
        const t = Math.max(start, Math.min(end, time));
        const segment = locateTime(t);
        return segment.u0 + ((t - segment.t0) / Math.max(1, segment.t1 - segment.t0)) * (segment.u1 - segment.u0);
      },
      fromRatio(ratio) {
        const u = Math.max(0, Math.min(1, ratio));
        const segment = locateRatio(u);
        return segment.t0 + ((u - segment.u0) / Math.max(Number.EPSILON, segment.u1 - segment.u0)) * (segment.t1 - segment.t0);
      },
    };
  }

  function projectionAt(models, edges, clock, instant) {
    const entries = models.filter((model) => {
      const t = placedMs(model, clock);
      if (t === null || t > instant) return false;
      return model.clocks.validUntil === null || model.clocks.validUntil > instant;
    });
    const refs = new Set(entries.map((entry) => entry.ref));
    return {
      instant,
      entries,
      relations: (edges || []).filter(
        (edge) =>
          (refs.has(edge.source) && refs.has(edge.target)) ||
          (edge.rel === "supports" && refs.has(edge.target))
      ),
    };
  }

  /* Stable set/content diff of two projections. Evidence and validity remain
     named dimensions of the result rather than being folded into a score. */
  function diffProjections(a, b) {
    const keyed = (items, key) => new Map((items || []).map((item) => [key(item), item]));
    const entryKey = (entry) => entry.ref;
    const relationKey = (edge) => `${edge.source}\u0000${edge.rel}\u0000${edge.target}\u0000${edge.class || ""}`;
    const diffSet = (left, right, key) => {
      const l = keyed(left, key);
      const r = keyed(right, key);
      const onlyA = [...l.keys()].filter((id) => !r.has(id));
      const onlyB = [...r.keys()].filter((id) => !l.has(id));
      const changed = [...l.keys()].filter(
        (id) => r.has(id) && JSON.stringify(l.get(id)) !== JSON.stringify(r.get(id))
      );
      return { onlyA, onlyB, changed };
    };
    const entries = diffSet(a.entries, b.entries, entryKey);
    const relations = diffSet(a.relations, b.relations, relationKey);
    const isEvidence = (entry) => (entry.kind || "").toLowerCase().includes("evidence");
    const evidenceItems = (projection) => [
      ...(projection.entries || []).filter(isEvidence),
      ...(projection.relations || [])
        .filter((edge) => edge.rel === "supports" || edge.evidence)
        .map((edge) => ({
          ref: `${edge.source}\u0000${edge.rel}\u0000${edge.target}`,
          text: edge.evidence || edge.why || "",
        })),
    ];
    const evidence = diffSet(evidenceItems(a), evidenceItems(b), entryKey);
    const validity = diffSet(
      (a.entries || []).map((entry) => ({ ref: entry.ref, from: entry.clocks.validFrom, until: entry.clocks.validUntil })),
      (b.entries || []).map((entry) => ({ ref: entry.ref, from: entry.clocks.validFrom, until: entry.clocks.validUntil })),
      entryKey
    );
    return { entries, relations, evidence, validity };
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

  /* Axis ticks are a screen-space promise. A temporal lens is deliberately
     non-linear, so choosing ticks in linear time and transforming them later
     can collapse a dozen labels into one compressed margin. Sample stable
     screen positions, invert through the lens, then snap each instant to the
     nearest useful calendar step for its local screen interval. */
  function screenAxisTicks(lens, width, spacing = 110) {
    const count = Math.max(1, Math.floor(Math.max(1, width) / Math.max(40, spacing)));
    const ratios = Array.from({ length: count + 1 }, (_, index) => index / count);
    const raw = ratios.map((ratio) => lens.fromRatio(ratio));
    const nearestStep = (span) => {
      const target = Math.max(1, span);
      return AXIS_STEPS.reduce((best, step) =>
        Math.abs(Math.log(step / target)) < Math.abs(Math.log(best / target)) ? step : best
      );
    };
    const ticks = raw.map((time, index) => {
      const before = raw[Math.max(0, index - 1)];
      const after = raw[Math.min(raw.length - 1, index + 1)];
      const step = nearestStep(Math.max(1, (after - before) / (index > 0 && index < raw.length - 1 ? 2 : 1)));
      if (index === 0 || index === raw.length - 1) {
        return { ratio: ratios[index], time, step };
      }
      let snapped = Math.round(time / step) * step;
      let ratio = lens.toRatio(snapped);
      // A calendar snap is useful only while it still honours the
      // screen-space placement promise. Otherwise the exact inverted instant
      // is more honest than moving a label into its neighbour.
      if (Math.abs(ratio - ratios[index]) > 0.08 / count) {
        snapped = time;
        ratio = ratios[index];
      }
      return {
        ratio,
        time: snapped,
        step,
      };
    });
    return { ticks };
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
    lodFor,
    alignObservabilitySeries,
    formatMetricValue,
    temporalLens,
    projectionAt,
    diffProjections,
    axisTicks,
    screenAxisTicks,
    tickLabel,
    arcStyle,
    classifyEdges,
    prism,
    parseQuery,
    matchesQuery,
  };
})();
