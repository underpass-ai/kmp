#!/usr/bin/env node
"use strict";

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { scheduledStopPlan } from "./obs-schedule-contract.mjs";

const plan30 = scheduledStopPlan(
  { picture_contract: { canvas: { fps: 30 } } },
  54000,
);
assert.equal(plan30.pictureContractFps, 30);
assert.equal(plan30.scheduledStopAdvanceFrames, 1);
assert.equal(plan30.scheduledStopAdvanceNs, 33333333n);
assert.equal(plan30.scheduledStopAdvanceMs, 33.333333);
assert.equal(plan30.nominalDurationNs, 54000000000n);
assert.equal(plan30.stopAfterStartNs, 53966666667n);

const plan25 = scheduledStopPlan(
  { picture_contract: { canvas: { fps: 25 } } },
  10000,
);
assert.equal(plan25.scheduledStopAdvanceNs, 40000000n);
assert.equal(plan25.stopAfterStartNs, 9960000000n);
assert.throws(() => scheduledStopPlan({}, 10000), /canvas\.fps/);
assert.throws(
  () => scheduledStopPlan({ picture_contract: { canvas: { fps: 30 } } }, 0),
  /positive integer/,
);

const directory = path.dirname(fileURLToPath(import.meta.url));
const scheduler = fs.readFileSync(path.join(directory, "obs-schedule.mjs"), "utf8");
const verifier = fs.readFileSync(path.join(directory, "verify-run.py"), "utf8");
const schedulerClauses = [
  "const stopPlan = scheduledStopPlan(edl, durationMs);",
  "const stopTargetNs = recordStartNs + stopPlan.stopAfterStartNs;",
  "scheduled_stop_advance_frames: stopPlan.scheduledStopAdvanceFrames",
  "scheduled_stop_advance_ms: stopPlan.scheduledStopAdvanceMs",
  "scheduled_stop_advance_ns: stopPlan.scheduledStopAdvanceNs.toString()",
];
for (const clause of schedulerClauses) assert.ok(scheduler.includes(clause), `scheduler lost: ${clause}`);
assert.ok(verifier.includes('"obs_stop_one_frame_advance"'), "verifier lost stop-advance gate");

const omitted = scheduler.replace(
  "const stopPlan = scheduledStopPlan(edl, durationMs);",
  "const stopPlan = { stopAfterStartNs: BigInt(durationMs) * 1000000n };",
);
assert.notEqual(omitted, scheduler);
assert.ok(
  schedulerClauses.some((clause) => !omitted.includes(clause)),
  "test failed to detect omitted one-frame plan",
);

const missingEvidence = scheduler.replace(
  "scheduled_stop_advance_frames: stopPlan.scheduledStopAdvanceFrames,",
  "",
);
assert.notEqual(missingEvidence, scheduler);
assert.ok(
  schedulerClauses.some((clause) => !missingEvidence.includes(clause)),
  "test failed to detect omitted StopRecord evidence",
);

console.log("OBS schedule: EDL-derived one-frame StopRecord advance and omission guards PASS");
