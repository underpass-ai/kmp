"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const file = path.join(__dirname, "loom-core.js");
const context = vm.createContext({});
vm.runInContext(fs.readFileSync(file, "utf8"), context, { filename: file });
vm.runInContext("globalThis.__KMP_LOOM__ = KMP_LOOM;", context);
const loom = context.__KMP_LOOM__;

test("semantic zoom separates dense and sparse windows at the same time scale", () => {
  const msPerPx = 129_203;
  const width = 2_784;

  assert.equal(loom.lodFor(msPerPx, width, 400), "atlas");
  assert.equal(loom.lodFor(msPerPx, width, 4), "episode");
  assert.notEqual(
    loom.lodFor(msPerPx, width, 400),
    loom.lodFor(msPerPx, width, 4)
  );
});

test("a nearly empty long window does not use Atlas", () => {
  assert.equal(loom.lodFor(2_000_000, 2_784, 3), "episode");
});

test("projection density uses the busiest lane", () => {
  const projection = {
    clusters: [
      { dimension: "agentic_episode", total: 190 },
      { dimension: "agentic_episode", total: 210 },
      { dimension: "agentic_process", total: 399 },
      { dimension: "task", total: 400 },
    ],
    included_dimensions: ["agentic_episode", "agentic_process", "task"],
    page: { total: 1_200 },
  };

  assert.equal(loom.maxMarksPerLane(projection), 400);
});

test("page totals provide a safe density estimate for a body-only page", () => {
  const projection = {
    entries: [],
    included_dimensions: ["agentic_episode", "agentic_process", "task"],
    page: { total: 1_200 },
  };

  assert.equal(loom.maxMarksPerLane(projection), 400);
});

test("a whole-second cluster endpoint still covers the entry inside that second", () => {
  // #454: one entry at 03:12:33.731471Z. The episode projection reported it,
  // the endpoint was serialized as the second that contains it, and the
  // moment window derived from that endpoint ended at .002Z — before the
  // entry — so a populated about rendered 0/0.
  const extent = loom.projectionExtent({
    clusters: [
      { dimension: "agentic_episode", from: "2026-08-31T03:12:33Z", to: "2026-08-31T03:12:33Z", total: 1 },
    ],
    page: { total: 1 },
  });

  const entry = Date.parse("2026-08-31T03:12:33.731Z");
  assert.ok(extent.t0 <= entry, `window starts at ${extent.t0}, entry at ${entry}`);
  assert.ok(extent.t1 >= entry, `window ends at ${extent.t1}, entry at ${entry}`);
});

test("a fractional cluster endpoint covers what Date.parse truncates below the millisecond", () => {
  const extent = loom.projectionExtent({
    clusters: [
      {
        dimension: "agentic_episode",
        from: "2026-08-31T03:12:33.731471Z",
        to: "2026-08-31T03:12:33.731471Z",
        total: 1,
      },
    ],
  });

  assert.ok(extent.t0 <= Date.parse("2026-08-31T03:12:33.731Z"));
  assert.ok(extent.t1 >= Date.parse("2026-08-31T03:12:33.732Z"));
});

test("bins are the coarse fallback and clusters win when both are present", () => {
  const projection = {
    bins: [{ from: "2020-01-01T00:00:00Z", to: "2030-01-01T00:00:00Z" }],
    clusters: [{ from: "2026-08-31T03:12:33Z", to: "2026-08-31T03:12:34Z" }],
  };
  const withClusters = loom.projectionExtent(projection);
  assert.equal(withClusters.t0, Date.parse("2026-08-31T03:12:33Z"));

  const binsOnly = loom.projectionExtent({ bins: projection.bins });
  assert.equal(binsOnly.t0, Date.parse("2020-01-01T00:00:00Z"));
  assert.ok(binsOnly.t1 >= Date.parse("2030-01-01T00:00:00Z"));
});

test("a projection with no aggregates has no extent", () => {
  assert.equal(loom.projectionExtent({}), null);
  assert.equal(loom.projectionExtent({ clusters: [], bins: [] }), null);
});

/* ---------------- clocks & placement ---------------- */

const entry = (coordinates, extra = {}) =>
  loom.entryModel({ ref_id: extra.ref || "r", kind: extra.kind || "decision", text: extra.text || "", coordinates });

test("an entry reads each clock from its earliest coordinate and never invents one", () => {
  const m = entry([
    { dimension: "d", scope_id: "s", occurred_at: "2026-08-31T10:00:00Z", observed_at: "2026-08-31T11:00:00Z" },
    { dimension: "d", scope_id: "s", occurred_at: "2026-08-31T09:00:00Z", valid_from: "2026-08-31T08:00:00Z", valid_until: "2026-08-31T12:00:00Z" },
  ]);
  assert.equal(m.clocks.occurred, Date.parse("2026-08-31T09:00:00Z"));
  assert.equal(m.clocks.observed, Date.parse("2026-08-31T11:00:00Z"));
  assert.equal(m.clocks.ingested, null, "an absent clock stays absent");
  assert.equal(loom.strictMs(m, "ingested"), null);
  assert.equal(loom.strictMs(m, "validity"), Date.parse("2026-08-31T08:00:00Z"));
  assert.equal(loom.placedMs(m, "ingested"), m.clocks.occurred, "placement falls back by precedence");
});

test("entries order by fallback time, then sequence, then ref", () => {
  const early = entry([{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T09:00:00Z" }], { ref: "b" });
  const late = entry([{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T10:00:00Z" }], { ref: "a" });
  assert.ok(loom.compareModels(early, late) < 0);
  const seqOne = entry([{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T09:00:00Z", sequence: 1 }], { ref: "z" });
  const seqTwo = entry([{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T09:00:00Z", sequence: 2 }], { ref: "a" });
  assert.ok(loom.compareModels(seqOne, seqTwo) < 0, "sequence outranks ref");
  const refA = entry([{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T09:00:00Z" }], { ref: "a" });
  const refB = entry([{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T09:00:00Z" }], { ref: "b" });
  assert.ok(loom.compareModels(refA, refB) < 0);
});

test("lanes keep first-appearance order and count scope members", () => {
  const lanes = loom.buildLanes([
    entry([{ dimension: "alpha", scope_id: "s1" }]),
    entry([{ dimension: "beta", scope_id: "s1" }, { dimension: "alpha", scope_id: "s2" }]),
  ]);
  assert.equal(JSON.stringify(lanes.map((lane) => lane.name)), JSON.stringify(["alpha", "beta"]));
  assert.equal(lanes[0].count, 2);
  assert.equal(lanes[0].scopes.get("s2"), 1);
});

test("the extent stretches to open validity and never collapses to a point", () => {
  const models = [
    entry([{ dimension: "d", scope_id: "s", valid_from: "2026-08-31T08:00:00Z", valid_until: "2026-08-31T12:00:00Z" }]),
  ];
  const extent = loom.extent(models, "validity");
  assert.equal(extent.t1, Date.parse("2026-08-31T12:00:00Z"));
  const instant = [entry([{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T08:00:00Z" }])];
  const point = loom.extent(instant, "occurred");
  assert.equal(point.t1, point.t0 + 1);
  assert.equal(loom.extent([], "occurred"), null);
});

/* ---------------- observability & units ---------------- */

test("series align on the shared axis but normalize only inside their own strip", () => {
  const [series] = loom.alignObservabilitySeries(
    [
      {
        name: "noise_ratio",
        unit: "ratio",
        scope: "store",
        points: [
          { at_millis: 0, value: 10 },
          { at_millis: 50, value: 30 },
          { at_millis: 100, value: 20 },
          { at_millis: 999, value: 99 },
        ],
      },
    ],
    0,
    100
  );
  assert.equal(series.points.length, 3, "points outside the window are not drawn");
  assert.equal(series.points[1].yRatio, 1, "the strip's own maximum reaches the top");
  assert.equal(series.points[0].xRatio, 0);
});

test("units decide honest display precision", () => {
  assert.equal(loom.formatMetricValue(1234.6, "tokens"), "1,235");
  assert.equal(loom.formatMetricValue(0.123456, "ratio"), "0.12");
  assert.equal(loom.formatMetricValue(12.34, "%"), "12.3");
  assert.equal(loom.formatMetricValue(123.456, "ms"), "123");
  assert.equal(loom.formatMetricValue(12.34, "ms"), "12.3");
  assert.equal(loom.formatMetricValue(1.234, "ms"), "1.23");
  assert.equal(loom.formatMetricValue(Infinity, "ms"), "—");
  assert.equal(loom.formatMetricValue(1234, ""), "1,234");
  assert.equal(loom.formatMetricValue(1.23456789, ""), "1.235");
});

/* ---------------- axis ---------------- */

test("axis ticks pick a calendar step that respects the tick budget", () => {
  const { step, ticks } = loom.axisTicks(0, 10 * 60e3, 12);
  assert.equal(step, 60e3, "ten minutes at twelve ticks reads in minutes");
  assert.equal(ticks[0], 0);
  assert.ok(ticks.length <= 12);
  assert.equal(loom.tickLabel(Date.parse("2026-08-31T10:05:00Z"), 60e3), "10:05");
  assert.equal(loom.tickLabel(Date.parse("2026-08-31T10:05:07Z"), 1e3), "10:05:07");
  assert.equal(loom.tickLabel(Date.parse("2026-08-31T10:05:00Z"), 86400e3), "08-31");
});

test("screen ticks honour their placement promise through a nonlinear lens", () => {
  const lens = loom.temporalLens({
    mode: "event_density",
    t0: 0,
    t1: 100 * 60e3,
    events: [0, 1000, 2000, 3000, 99 * 60e3 + 58000, 100 * 60e3],
  });
  const { ticks } = loom.screenAxisTicks(lens, 1000, 110);
  assert.ok(ticks.length >= 2);
  for (let i = 1; i < ticks.length; i += 1) {
    assert.ok(ticks[i].ratio >= ticks[i - 1].ratio, "ticks never cross on screen");
  }
  assert.equal(ticks[0].ratio, 0);
  assert.equal(ticks[ticks.length - 1].ratio, 1);
});

/* ---------------- relations ---------------- */

test("every relation class has a distinct dash-and-weight voice", () => {
  assert.equal(loom.arcStyle("causal").dash, null);
  assert.ok(loom.arcStyle("evidential").dash);
  assert.equal(loom.arcStyle("unheard-of").width, loom.arcStyle("structural").width, "unknown classes fade like structure");
});

test("supersession and contradiction keep their nature instead of melting into arcs", () => {
  const has = new Set(["a", "b", "c"]);
  const { arcs, supersessions, contradictions } = loom.classifyEdges(
    [
      { source: "a", target: "b", rel: "led_to" },
      { source: "b", target: "a", rel: "supersedes" },
      { source: "a", target: "c", rel: "contradicts" },
      { source: "a", target: "ghost", rel: "led_to" },
    ],
    (ref) => has.has(ref)
  );
  assert.equal(arcs.length, 1);
  assert.equal(supersessions.length, 1);
  assert.equal(contradictions.length, 1);
});

/* ---------------- the prism ---------------- */

test("the prism keeps absent rails absent and spans only real stamps", () => {
  const m = entry([
    { dimension: "d", scope_id: "s", occurred_at: "2026-08-31T08:00:00Z", ingested_at: "2026-08-31T09:00:00Z", sequence: 3 },
  ]);
  const prism = loom.prism(m);
  assert.equal(prism.rails.observed, null);
  assert.equal(prism.rails.validity, null);
  assert.equal(prism.span.t0, Date.parse("2026-08-31T08:00:00Z"));
  assert.equal(prism.order[0].sequence, 3);
  const bare = loom.prism(entry([{ dimension: "d", scope_id: "s" }]));
  assert.equal(bare.span, null, "no stamps, no span");
});

/* ---------------- search ---------------- */

test("the query grammar separates kind, dim and id tokens from text", () => {
  const query = loom.parseQuery("kind:decision dim:process id:kmp truncation cliff");
  assert.equal(query.kind, "decision");
  assert.equal(query.dim, "process");
  assert.equal(query.id, "kmp");
  assert.equal(JSON.stringify(query.text), JSON.stringify(["truncation", "cliff"]));
  assert.equal(loom.parseQuery("   ").empty, true);
});

test("matching requires every facet the query names", () => {
  const fields = {
    text: "the truncation cliff root cause",
    id: "project:kmp:entry:decision:one",
    kind: "decision",
    dim: "agentic_process main",
  };
  assert.equal(loom.matchesQuery(loom.parseQuery("kind:decision truncation"), fields), true);
  assert.equal(loom.matchesQuery(loom.parseQuery("kind:evidence truncation"), fields), false);
  assert.equal(loom.matchesQuery(loom.parseQuery("dim:process cliff"), fields), true);
  assert.equal(loom.matchesQuery(loom.parseQuery("id:missing"), fields), false);
  assert.equal(loom.matchesQuery(loom.parseQuery(""), fields), false, "an empty query matches nothing");
});

/* ---------------- projections at an instant ---------------- */

test("a projection at an instant holds what was known and still valid", () => {
  const early = entry([{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T08:00:00Z" }], { ref: "early" });
  const expired = loom.entryModel({
    ref_id: "expired",
    kind: "constraint",
    text: "",
    coordinates: [{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T07:00:00Z", valid_until: "2026-08-31T09:00:00Z" }],
  });
  const late = entry([{ dimension: "d", scope_id: "s", occurred_at: "2026-08-31T12:00:00Z" }], { ref: "late" });
  const projection = loom.projectionAt(
    [early, expired, late],
    [
      { source: "early", target: "late", rel: "led_to" },
      { source: "ghost", target: "early", rel: "supports" },
    ],
    "occurred",
    Date.parse("2026-08-31T10:00:00Z")
  );
  assert.equal(JSON.stringify(projection.entries.map((e) => e.ref)), JSON.stringify(["early"]));
  assert.equal(projection.relations.length, 1, "supports reaches a drawn target even from off-screen");
});
