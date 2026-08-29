#!/usr/bin/env node
"use strict";

import crypto from "node:crypto";

export const TERMINAL_VIEWPORT_CONTRACT = "kmp.terminal-semantic-viewport.v1";
export const TERMINAL_VIEWPORT_RESET = "\x1b[2J\x1b[H";
export const TERMINAL_VIEWPORT_RESET_SHA256 = crypto
  .createHash("sha256")
  .update(TERMINAL_VIEWPORT_RESET)
  .digest("hex");
export const TERMINAL_COLUMNS = 32;
export const TERMINAL_ROW_BUDGET = 24;

// GNOME Terminal's title chrome ends above y=40 in the isolated X11 screen.
// Semantic beats are reset to row 1, so focus scenes must follow that origin
// instead of magnifying a fixed band from the old scrollback viewport.
export const TERMINAL_SEMANTIC_CROP = Object.freeze([0, 40, 672, 378]);

export function captureSceneDefinitions() {
  return [
    {
      name: "KMP/Wide",
      sources: [{ role: "wide", crop: [0, 0, 1920, 1080], target: [0, 0, 1920, 1080] }],
    },
    {
      name: "KMP/TerminalFocus",
      sources: [
        { role: "primary-terminal", crop: [...TERMINAL_SEMANTIC_CROP], target: [0, 0, 1920, 1080] },
        { role: "secondary-chronoloom", crop: [672, 189, 1248, 702], target: [1390, 748, 500, 281] },
      ],
    },
    {
      name: "KMP/ChronoFocus",
      sources: [
        { role: "primary-chronoloom", crop: [1350, 160, 570, 321], target: [0, 0, 1920, 1080] },
        { role: "secondary-terminal", crop: [...TERMINAL_SEMANTIC_CROP], target: [40, 748, 520, 292] },
      ],
    },
    {
      name: "KMP/ProofFocus",
      sources: [
        { role: "primary-proof", crop: [1350, 430, 570, 321], target: [0, 0, 1920, 1080] },
        { role: "secondary-terminal", crop: [...TERMINAL_SEMANTIC_CROP], target: [40, 748, 520, 292] },
      ],
    },
    {
      name: "KMP/CTAFocus",
      sources: [
        { role: "primary-cta", crop: [...TERMINAL_SEMANTIC_CROP], target: [0, 0, 1920, 1080] },
        { role: "secondary-chronoloom", crop: [672, 189, 1248, 702], target: [1390, 748, 500, 281] },
      ],
    },
  ];
}

export function validateTerminalSceneContract(scenes) {
  const terminalSources = scenes
    .flatMap((scene) => scene.sources.map((source) => ({ scene: scene.name, ...source })))
    .filter((source) => ["primary-terminal", "secondary-terminal", "primary-cta"].includes(source.role));
  const errors = [];
  if (terminalSources.length !== 4) errors.push(`terminal focus source count ${terminalSources.length} != 4`);
  for (const source of terminalSources) {
    if (JSON.stringify(source.crop) !== JSON.stringify(TERMINAL_SEMANTIC_CROP)) {
      errors.push(`${source.scene}/${source.role} does not crop from the semantic origin`);
    }
  }
  return errors;
}

export function validateSemanticSceneAlignment(steps, schedule) {
  const errors = [];
  for (const event of schedule.filter((item) => ["KMP/TerminalFocus", "KMP/CTAFocus"].includes(item.scene))) {
    if (!steps.some((step) => opensSemanticViewport(step) && step.at_ms === event.at_ms)) {
      errors.push(`${event.scene} at ${event.at_ms}ms has no composition-opening terminal beat`);
    }
  }
  return errors;
}

function sha256(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

export function semanticStepIdentity(step) {
  if (!step || !Number.isInteger(step.at_ms)) throw new Error("semantic terminal step needs integer at_ms");
  const identity = { at_ms: step.at_ms, type: step.type };
  if (step.type === "say") {
    if (!step.speaker || typeof step.text !== "string") throw new Error("say step needs speaker and text");
    return { ...identity, speaker: step.speaker, text_sha256: sha256(step.text) };
  }
  if (step.type === "process") {
    if (!step.action || !step.process_id) throw new Error("process step needs action and process_id");
    return { ...identity, action: step.action, process_id: step.process_id };
  }
  throw new Error(`step type ${step.type} is not a semantic terminal beat`);
}

export function opensSemanticViewport(step) {
  return step?.type === "say" || step?.type === "process";
}

export function semanticViewportAudit(step) {
  return {
    event_type: "viewport_reset",
    contract: TERMINAL_VIEWPORT_CONTRACT,
    mechanism: "ansi_clear_display_2_home",
    reset_sequence_sha256: TERMINAL_VIEWPORT_RESET_SHA256,
    terminal_columns: TERMINAL_COLUMNS,
    row_budget: TERMINAL_ROW_BUDGET,
    step: semanticStepIdentity(step),
  };
}

export function renderedTerminalRows(speaker, text, columns = TERMINAL_COLUMNS) {
  if (!Number.isInteger(columns) || columns <= 0) throw new Error("terminal columns must be positive");
  const prefix = speaker ? `${String(speaker).toUpperCase()}  ` : "";
  const physicalLines = String(text).split("\n");
  const wrapped = physicalLines.reduce((total, line, index) => {
    const cells = Array.from(`${index === 0 ? prefix : ""}${line}`).length;
    return total + Math.max(1, Math.ceil(cells / columns));
  }, 0);
  return wrapped + 1;
}

export function consumeTerminalRows(used, speaker, text, budget = TERMINAL_ROW_BUDGET) {
  if (!Number.isInteger(used) || used < 0) throw new Error("used terminal rows must be non-negative");
  const rows = renderedTerminalRows(speaker, text);
  const next = used + rows;
  if (next > budget) {
    throw new Error(`semantic terminal viewport row budget exceeded: ${next} > ${budget}`);
  }
  return { rows, used: next, budget };
}

export function resetSemanticViewport(step, { write, audit }) {
  if (typeof write !== "function" || typeof audit !== "function") {
    throw new Error("resetSemanticViewport needs write and audit callbacks");
  }
  const evidence = semanticViewportAudit(step);
  write(TERMINAL_VIEWPORT_RESET);
  audit(evidence);
  return evidence;
}

export function validateSemanticViewportAudit(steps, rows) {
  const expected = steps
    .filter(opensSemanticViewport)
    .map((step) => semanticViewportAudit(step));
  const actual = rows.filter((row) => row.event_type === "viewport_reset");
  const errors = [];
  if (actual.length !== expected.length) {
    errors.push(`viewport reset count ${actual.length} != ${expected.length}`);
  }
  for (let index = 0; index < Math.max(actual.length, expected.length); index += 1) {
    const observed = actual[index];
    const wanted = expected[index];
    if (!observed || !wanted) continue;
    for (const key of [
      "event_type", "contract", "mechanism", "reset_sequence_sha256",
      "terminal_columns", "row_budget",
    ]) {
      if (observed[key] !== wanted[key]) errors.push(`viewport reset ${index} has wrong ${key}`);
    }
    if (JSON.stringify(observed.step) !== JSON.stringify(wanted.step)) {
      errors.push(`viewport reset ${index} is bound to the wrong semantic step`);
    }
  }
  return errors;
}
