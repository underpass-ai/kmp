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

test("folding a lane sums its labels' bins and merges its clusters' refs", () => {
  const bins = loom.foldAggregates([
    { dimension: "task", scope_id: "about:a:dimension:t-1", from: "A", to: "B", total: 2, by_kind: { decision: 2 } },
    { dimension: "task", scope_id: "about:a:dimension:t-2", from: "A", to: "B", total: 1, by_kind: { evidence: 1 } },
    { dimension: "task", scope_id: "about:a:dimension:t-1", from: "B", to: "C", total: 1, by_kind: { decision: 1 } },
    { dimension: "agentic_process", scope_id: "about:a:dimension:p", from: "A", to: "B", total: 3, by_kind: { decision: 3 } },
  ]);
  assert.equal(bins.length, 3);
  // The loom's objects come from another realm; compare by content.
  assert.equal(
    JSON.stringify(bins[0]),
    JSON.stringify({ dimension: "task", from: "A", to: "B", total: 3, by_kind: { decision: 2, evidence: 1 } })
  );
  assert.equal(bins[2].dimension, "agentic_process");
  const clusters = loom.foldAggregates([
    { dimension: "task", scope_id: "s1", from: "A", to: "B", total: 2, refs: ["x", "y"], by_kind: { decision: 2 } },
    { dimension: "task", scope_id: "s2", from: "A", to: "B", total: 1, refs: ["y"], by_kind: { decision: 1 } },
  ]);
  assert.equal(clusters.length, 1);
  assert.equal(JSON.stringify(clusters[0].refs), JSON.stringify(["x", "y"]), "a ref standing in two labels is one ref");
  assert.equal(clusters[0].total, 3, "memberships still count as memberships");
  assert.equal(loom.foldAggregates(undefined).length, 0);
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

/* ---------------- the aggregate snapshot (#463) ---------------- */

test("a full snapshot's facets normalize cleared filters unambiguously", () => {
  const cleared = loom.agentStateFacets({
    view_revision: 42,
    focus: { time_range: null, refs: ["decision:new", "success:old"] },
    search: null,
  });
  assert.equal(cleared.search, "", "a null search reads as empty, not as leave-alone");
  assert.equal(cleared.range, null);
  assert.equal(cleared.explicitRange, false);
  assert.equal(JSON.stringify(cleared.refs), JSON.stringify(["decision:new", "success:old"]));

  const framed = loom.agentStateFacets({
    focus: { time_range: { from: "2026-08-31T16:49:00Z", to: "2026-08-31T17:39:00Z" } },
    search: "attempt-000005",
  });
  assert.equal(framed.search, "attempt-000005");
  assert.equal(framed.explicitRange, true);
  assert.equal(JSON.stringify(framed.refs), JSON.stringify([]));

  const open = loom.agentStateFacets({ focus: { time_range: { from: "2026-08-31T16:49:00Z" } } });
  assert.equal(open.explicitRange, false, "an open-ended window is not an explicit frame");
  assert.equal(loom.agentStateFacets({}).range, null, "a bare snapshot still normalizes");
});

test("the known extent grows to include refs newer than the cached probe", () => {
  const current = { t0: 1000, t1: 2000 };
  const grown = loom.extentIncluding(current, 500, 3000);
  assert.equal(grown.t0, 500);
  assert.equal(grown.t1, 3000);
  const inside = loom.extentIncluding(current, 1200, 1800);
  assert.equal(inside.t0, 1000);
  assert.equal(inside.t1, 2000);
  const fresh = loom.extentIncluding(null, 4000, 4000);
  assert.equal(fresh.t1, fresh.t0 + 1, "a single instant never collapses the extent");
  assert.equal(loom.extentIncluding(current, NaN, 3000), current, "an unreadable stamp changes nothing");
});

/* ---------------- label selectors ---------------- */

test("the selector grammar round-trips and refuses what means nothing", () => {
  const selectors = loom.parseLabelQuery("task in launch|other; incident notexists ;env notin prod;bad like x;task in");
  assert.equal(
    JSON.stringify(selectors),
    JSON.stringify([
      { key: "env", op: "notin", values: ["prod"] },
      { key: "incident", op: "notexists", values: [] },
      { key: "task", op: "in", values: ["launch", "other"] },
    ])
  );
  assert.equal(loom.labelQuery(selectors), "env notin prod;incident notexists;task in launch|other", "sorted: order carries no meaning");
  assert.equal(loom.labelQuery(loom.parseLabelQuery(loom.labelQuery(selectors))), loom.labelQuery(selectors));
  assert.equal(loom.labelQuery([]), "");
  assert.equal(loom.labelQuery([{ key: "task", op: "exists", values: ["stray"] }]), "task exists", "exists carries no values");
});

test("adding a selector merges values on the same key and operator", () => {
  let selectors = loom.withSelector([], { key: "task", op: "in", values: ["a"] });
  selectors = loom.withSelector(selectors, { key: "task", op: "in", values: ["b"] });
  assert.equal(loom.labelQuery(selectors), "task in a|b");
  selectors = loom.withSelector(selectors, { key: "task", op: "notin", values: ["c"] });
  assert.equal(loom.labelQuery(selectors), "task in a|b;task notin c");
  selectors = loom.withSelector(selectors, { key: "task", op: "exists", values: [] });
  selectors = loom.withSelector(selectors, { key: "task", op: "exists", values: [] });
  assert.equal(loom.labelQuery(selectors), "task in a|b;task notin c;task exists");
  assert.equal(loom.labelQuery(loom.withSelector(selectors, { key: "", op: "in", values: ["x"] })), loom.labelQuery(selectors));
});

/* ---------------- lanes, fibres, rows ---------------- */

const catalogue = [
  { dimension: "task", scope_id: "about:a:dimension:t-1", value: "t-1", in_range: 5, entries: 11, last_observed_at: "2026-09-05T06:00:00Z" },
  { dimension: "task", scope_id: "about:a:dimension:t-2", value: "t-2", in_range: 0, entries: 9 },
  { dimension: "task", scope_id: "about:a:dimension:t-3", value: "t-3", in_range: 2, entries: 2 },
  { dimension: "agentic_process", scope_id: "about:a:dimension:p-1", value: "p-1", in_range: 7, entries: 40 },
  { dimension: "agentic_episode", scope_id: "about:a:dimension:e-1", value: "e-1", in_range: 1, entries: 1 },
  { dimension: "agentic_episode", scope_id: "about:a:dimension:e-2", value: "e-2", in_range: 0, entries: 3 },
];

test("lanes come from the catalogue with a fibre per value, empty ones included", () => {
  const lanes = loom.lanesFromLabels(catalogue);
  assert.equal(JSON.stringify(lanes.map((lane) => lane.name)), JSON.stringify(["task", "agentic_process", "agentic_episode"]));
  const task = lanes[0];
  assert.equal(task.count, 7, "a lane's use here is the sum of its fibres'");
  assert.equal(task.total, 22);
  assert.equal(JSON.stringify(task.fibres.map((f) => f.value)), JSON.stringify(["t-1", "t-3", "t-2"]), "by use here, then ever");
  assert.equal(task.fibres[2].count, 0, "an empty fibre is a fibre");
  assert.equal(task.fibres[0].lastObserved, Date.parse("2026-09-05T06:00:00Z"));
  assert.equal(loom.bareValue("about:a:dimension:conversation:rachel"), "conversation:rachel");
  assert.equal(loom.bareValue("plain"), "plain");
});

test("rows are a fibre each when they fit, and fold to a budget without hiding", () => {
  const lanes = loom.lanesFromLabels(catalogue);
  const roomy = loom.laneRows(lanes, { budget: 100 });
  assert.equal(JSON.stringify(roomy.map((l) => l.rows.map((r) => r.kind))), JSON.stringify([["fibre", "fibre", "fibre"], ["fibre"], ["fibre", "fibre"]]));
  assert.equal(roomy[1].rows[0].label, "p-1", "a lane with one value shows the value");

  const folded = loom.laneRows(lanes, { budget: 100, folded: new Set(["task"]) });
  assert.equal(folded[0].rows.length, 1);
  assert.equal(folded[0].rows[0].kind, "folded");
  assert.equal(folded[0].rows[0].fibres.length, 3, "a folded row still knows its fibres");

  const tight = loom.laneRows(lanes, { budget: 4 });
  // task needs 3, episode 2, process 1 = 6 > 4: the biggest folds, the rest fit.
  assert.equal(tight[0].folded, true);
  assert.equal(tight[0].auto, true, "folded to fit, and says so");
  assert.equal(tight[2].rows.length, 2);

  const squeezed = loom.laneRows(lanes, { budget: 5 });
  // 5 - 1 fixed = 4 spare for task (3) and episode (2): two rows each.
  const task = squeezed[0];
  assert.equal(task.folded, false);
  assert.equal(task.rows.length, 2);
  assert.equal(task.rows[0].kind, "fibre");
  assert.equal(task.rows[1].kind, "more");
  assert.equal(task.rows[1].label, "+2 more");
  assert.equal(task.rows[1].count, 2, "the more row counts what it folds");

  const pinned = loom.laneRows(lanes, { budget: 5, pinned: new Set([loom.fibreId("task", "about:a:dimension:t-2")]) });
  assert.equal(pinned[0].rows[0].label, "t-2", "a pinned fibre keeps a row of its own");
  assert.equal(pinned[0].rows[0].pinned, true);

  const index = loom.rowIndex(squeezed);
  const at = (lane, scope) => index.get(loom.fibreId(lane, scope));
  assert.equal(at("task", "about:a:dimension:t-3").kind, "more");
  assert.equal(at("task", "about:a:dimension:t-1").kind, "fibre");
  assert.equal(at("agentic_process", "about:a:dimension:p-1").kind, "fibre");
});

test("fibre overlap counts shared refs at Moment and at Episode, nothing at Atlas", () => {
  const e = (ref, scopes) => entry(scopes.map((s) => ({ dimension: "d", scope_id: s })), { ref });
  const id = (scope) => loom.fibreId("d", scope);
  const atMoment = loom.fibreOverlap(id("t-1"), { entries: [e("a", ["t-1", "p-1"]), e("b", ["t-1"]), e("c", ["p-1", "e-1"])] });
  assert.equal(atMoment.size, 2);
  assert.equal(atMoment.overlap.get(id("p-1")), 1);
  assert.equal(atMoment.overlap.get(id("e-1")), 0);
  const atEpisode = loom.fibreOverlap(id("t-1"), {
    clusters: [
      { dimension: "d", scope_id: "t-1", refs: ["a", "b"] },
      { dimension: "d", scope_id: "p-1", refs: ["a", "c"] },
    ],
  });
  assert.equal(atEpisode.size, 2);
  assert.equal(atEpisode.overlap.get(id("p-1")), 1);
  const atAtlas = loom.fibreOverlap(id("t-1"), {});
  assert.equal(atAtlas.size, 0);
  // The same value under two keys — an about written before v0.12.0 fixed
  // one key per value — is two fibres, never one.
  const twoKeys = loom.lanesFromLabels([
    { dimension: "task", scope_id: "s", value: "s", in_range: 2, entries: 2 },
    { dimension: "agentic_process", scope_id: "s", value: "s", in_range: 1, entries: 1 },
  ]);
  assert.notEqual(twoKeys[0].fibres[0].id, twoKeys[1].fibres[0].id);
  const both = entry([{ dimension: "task", scope_id: "s" }, { dimension: "agentic_process", scope_id: "s" }], { ref: "x" });
  const only = entry([{ dimension: "task", scope_id: "s" }], { ref: "y" });
  const split = loom.fibreOverlap(loom.fibreId("task", "s"), { entries: [both, only] });
  assert.equal(split.size, 2);
  assert.equal(split.overlap.get(loom.fibreId("agentic_process", "s")), 1);
});

test("a coordinate stitched on by kmp_relabel says so, and carries its why", () => {
  const m = entry([
    { dimension: "task", scope_id: "s1" },
    { dimension: "task", scope_id: "s2", method: "kmp_relabel", why: "belongs here", motivation: "Relabelled by agent at T." },
  ]);
  assert.equal(loom.stitched(m.coords[0]), false);
  assert.equal(loom.stitched(m.coords[1]), true);
  assert.equal(m.coords[1].why, "belongs here");
  assert.equal(m.coords[0].why, null);
});
