"use strict";

/* Whole paths under node: what a kmp_curate path answer becomes on the loom,
   and the apply call a person's declaration composes. The loom never makes
   that call; these tests pin that it only ever composes it. */

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

function model() {
  const context = vm.createContext({ console });
  for (const name of ["loom-path-model.js"])
    vm.runInContext(fs.readFileSync(path.join(__dirname, name), "utf8"), context, { filename: name });
  return vm.runInContext("KMP_APP.pathModel", context);
}
const plain = (value) => JSON.parse(JSON.stringify(value));
const end = (ref, about = "service:alpha") => ({ ref, about, excerpt: ref });
const found = {
  from: "a:1",
  to: "b:3",
  review_token: "f".repeat(64),
  summary: "2 paths",
  warnings: [],
  paths: [
    {
      proposed: 1,
      confidence: 0.61,
      hops: [
        { from: end("a:1"), to: end("a:2"), rel: "causes", declared: true, reversed: true, confidence: 1 },
        {
          from: end("a:2"),
          to: end("b:3", "service:beta"),
          rel: "same_event_as",
          declared: false,
          reversed: false,
          confidence: 0.6149,
          item_id: "m1",
        },
      ],
    },
    {
      proposed: 0,
      confidence: 1,
      hops: [{ from: end("a:1"), to: end("a:2"), rel: "causes", declared: true, reversed: true, confidence: 1 }],
    },
  ],
  avoided: [{ from: end("a:1"), to: end("a:4"), rel: "supports", support: 0.12 }],
};

test("a path answer draws each hop once, declared in its stored direction", () => {
  const drawn = model().overlay(found);
  assert.equal(drawn.chains.length, 2);
  assert.equal(drawn.hops.length, 3, "the shared declared hop is drawn once");
  const [declared, proposed, avoided] = drawn.hops;
  assert.deepEqual(plain([declared.kind, declared.source, declared.target]), ["declared", "a:2", "a:1"]);
  assert.equal(declared.item_id, null);
  assert.deepEqual(plain([proposed.kind, proposed.source, proposed.target]), ["proposed", "a:2", "b:3"]);
  assert.equal(proposed.item_id, "m1");
  assert.equal(avoided.kind, "avoided");
  assert.equal(avoided.confidence, 0.12);
  assert.deepEqual(plain([...drawn.refs].sort()), ["a:1", "a:2", "a:4", "b:3"]);
  assert.equal(model().overlay(null), null);
  assert.equal(model().overlay({ from: "" }), null);
});

test("the scene draws only hops with both ends in the frame; labels carry rel and confidence", () => {
  const paths = model();
  const drawn = paths.overlay(found);
  const placed = new Set(["a:1", "a:2"]);
  assert.deepEqual(plain(paths.sceneHops(drawn, placed).map((hop) => hop.kind)), ["declared"]);
  assert.equal(paths.hopLabel(drawn.hops[0]), "causes");
  assert.equal(paths.hopLabel(drawn.hops[1]), "same_event_as · 0.61");
  assert.deepEqual(plain(paths.framingRefs(drawn, "service:beta")), ["b:3"]);
});

test("declaring a proposed hop composes the exact kmp_curate apply call and nothing else", () => {
  const paths = model();
  const drawn = paths.overlay(found);
  const proposed = drawn.hops[1];
  const { call } = paths.declareCall(drawn, proposed, {
    why: "  Both reports name the same outage window. ",
    evidence: "INC-17 in both texts.",
    confidence: "medium",
    actor: "human",
  });
  assert.deepEqual(plain(call), {
    tool: "kmp_curate",
    arguments: {
      mode: "apply",
      about: "service:alpha",
      review_token: "f".repeat(64),
      accepted: [
        {
          item_id: "m1",
          why: "Both reports name the same outage window.",
          evidence: "INC-17 in both texts.",
          confidence: "medium",
        },
      ],
      actor: "human",
    },
  });
});

test("a declaration without the person's why or evidence, or of a declared hop, is refused", () => {
  const paths = model();
  const drawn = paths.overlay(found);
  assert.match(paths.declareCall(drawn, drawn.hops[1], { evidence: "e" }).error, /why/);
  assert.match(paths.declareCall(drawn, drawn.hops[1], { why: "w" }).error, /sources/);
  assert.match(paths.declareCall(drawn, drawn.hops[0], { why: "w", evidence: "e" }).error, /proposed/);
  const frozenless = paths.overlay({ ...found, review_token: null });
  assert.match(paths.declareCall(frozenless, frozenless.hops[1], { why: "w", evidence: "e" }).error, /review/);
  const { call } = paths.declareCall(drawn, drawn.hops[1], { why: "w", evidence: "e", confidence: "sure" });
  assert.equal(call.arguments.accepted[0].confidence, undefined, "outside the vocabulary is left out");
  assert.equal(call.arguments.actor, undefined, "the agent names itself when none is given");
});
