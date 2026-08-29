#!/usr/bin/env node
"use strict";

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  TERMINAL_SEMANTIC_CROP,
  TERMINAL_COLUMNS,
  TERMINAL_ROW_BUDGET,
  TERMINAL_VIEWPORT_CONTRACT,
  TERMINAL_VIEWPORT_RESET,
  TERMINAL_VIEWPORT_RESET_SHA256,
  captureSceneDefinitions,
  consumeTerminalRows,
  opensSemanticViewport,
  resetSemanticViewport,
  semanticViewportAudit,
  validateSemanticSceneAlignment,
  validateTerminalSceneContract,
  validateSemanticViewportAudit,
} from "./terminal-presentation-contract.mjs";

const steps = [
  { at_ms: 0, type: "say", speaker: "process", text: "The semantic hook." },
  { at_ms: 1000, type: "tool", process_id: "process-a", name: "kmp_inspect" },
  { at_ms: 2000, type: "process", process_id: "process-b", action: "start" },
  { at_ms: 3000, type: "hold" },
];

const writes = [];
const audits = [];
for (const step of steps.filter(opensSemanticViewport)) {
  resetSemanticViewport(step, {
    write: (value) => writes.push(value),
    audit: (value) => audits.push(value),
  });
}

assert.deepEqual(TERMINAL_SEMANTIC_CROP, [0, 40, 672, 378]);
assert.equal(TERMINAL_COLUMNS, 32);
assert.equal(TERMINAL_ROW_BUDGET, 24);
const terminalSources = captureSceneDefinitions()
  .flatMap((scene) => scene.sources.map((source) => ({ scene: scene.name, ...source })))
  .filter((source) => ["primary-terminal", "secondary-terminal", "primary-cta"].includes(source.role));
assert.equal(terminalSources.length, 4);
assert.ok(terminalSources.every((source) => JSON.stringify(source.crop) === JSON.stringify(TERMINAL_SEMANTIC_CROP)));
assert.deepEqual(validateTerminalSceneContract(captureSceneDefinitions()), []);
assert.equal(writes.join(""), TERMINAL_VIEWPORT_RESET.repeat(2));
assert.match(TERMINAL_VIEWPORT_RESET_SHA256, /^[0-9a-f]{64}$/);
assert.equal(audits[0].contract, TERMINAL_VIEWPORT_CONTRACT);
assert.deepEqual(validateSemanticViewportAudit(steps, audits), []);

let rowUse = consumeTerminalRows(0, "user", "Why does this decision still hold?");
rowUse = consumeTerminalRows(rowUse.used, "kmp", "decision: The evidence is still current.");
assert.ok(rowUse.used <= TERMINAL_ROW_BUDGET);
assert.throws(
  () => consumeTerminalRows(rowUse.used, "kmp", "x".repeat(TERMINAL_COLUMNS * TERMINAL_ROW_BUDGET)),
  /row budget exceeded/,
);

const omitted = audits.slice(1);
assert.ok(validateSemanticViewportAudit(steps, omitted).length > 0, "an omitted reset must fail");

const wrongHash = structuredClone(audits);
wrongHash[1].reset_sequence_sha256 = "0".repeat(64);
assert.ok(validateSemanticViewportAudit(steps, wrongHash).length > 0, "a mutated reset must fail");

const staleScrollbackCrop = captureSceneDefinitions();
staleScrollbackCrop[1].sources[0].crop = [0, 160, 672, 378];
assert.ok(
  validateTerminalSceneContract(staleScrollbackCrop).length > 0,
  "the old scrollback crop must fail the semantic-origin contract",
);

const wrongBinding = structuredClone(audits);
wrongBinding[0].step.text_sha256 = "f".repeat(64);
assert.ok(validateSemanticViewportAudit(steps, wrongBinding).length > 0, "a reset bound to other copy must fail");

const extra = [...audits, semanticViewportAudit(steps[0])];
assert.ok(validateSemanticViewportAudit(steps, extra).length > 0, "an extra reset must fail");

assert.throws(
  () => resetSemanticViewport(steps[3], { write: () => {}, audit: () => {} }),
  /not a semantic terminal beat/,
);
assert.throws(
  () => resetSemanticViewport(steps[1], { write: () => {}, audit: () => {} }),
  /not a semantic terminal beat/,
);

const harness = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const edl = JSON.parse(fs.readFileSync(path.join(harness, "..", "edl.json"), "utf8"));
for (const name of [
  "technical-smoke",
  "fresh-process-same-why",
  "two-processes-one-memory",
  "keep-the-wrong-turn",
]) {
  const scenario = JSON.parse(fs.readFileSync(path.join(harness, "scenarios", `${name}.json`), "utf8"));
  const rows = scenario.steps
    .filter(opensSemanticViewport)
    .map((step) => semanticViewportAudit(step));
  assert.deepEqual(
    validateSemanticViewportAudit(scenario.steps, rows),
    [],
    `${name} must have one auditable reset per semantic beat`,
  );
  const master = edl.masters.find((item) => item.id === name);
  if (master) {
    assert.deepEqual(validateSemanticSceneAlignment(scenario.steps, master.obs_schedule), []);
    const shifted = structuredClone(master.obs_schedule);
    const terminalFocus = shifted.find((item) => ["KMP/TerminalFocus", "KMP/CTAFocus"].includes(item.scene));
    terminalFocus.at_ms += 1;
    assert.ok(
      validateSemanticSceneAlignment(scenario.steps, shifted).length > 0,
      `${name} must reject a focus scene detached from its terminal beat`,
    );
  }
}

const twoProcessScenario = JSON.parse(
  fs.readFileSync(path.join(harness, "scenarios", "two-processes-one-memory.json"), "utf8"),
);
const writtenMemory = twoProcessScenario.steps.find((step) => step.name === "kmp_write_memory").arguments.current;
const askStep = twoProcessScenario.steps.find((step) => step.name === "kmp_ask");
const inspectStep = twoProcessScenario.steps.find((step) => step.name === "kmp_inspect");
assert.deepEqual(askStep.display_paths, [
  { label: "decision", path: "structuredContent.proof.evidence.0.text" },
  { label: "evidence", path: "structuredContent.proof.evidence.1.text" },
]);
assert.deepEqual(inspectStep.display_paths, [
  { label: "ref", path: "structuredContent.object.ref" },
]);
let twoProcessRows = consumeTerminalRows(0, "user", "Why is max_connections back at 200?");
for (const [speaker, text] of [
  ["kmp", "process-b · kmp_ask"],
  ["kmp", `decision: ${writtenMemory.summary}`],
  ["kmp", `evidence: ${writtenMemory.evidence}`],
  ["kmp", "process-b · kmp_inspect"],
  ["kmp", `ref: ${writtenMemory.ref}`],
]) {
  twoProcessRows = consumeTerminalRows(twoProcessRows.used, speaker, text);
}
assert.ok(
  twoProcessRows.used <= TERMINAL_ROW_BUDGET,
  "the complete Process B question/answer composition must remain above the scroll boundary",
);

console.log("terminal presentation contract: ok");
