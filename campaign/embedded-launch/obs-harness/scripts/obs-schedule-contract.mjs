#!/usr/bin/env node
"use strict";

/** Derive the one-frame StopRecord advance from the checked-in picture contract. */
export function scheduledStopPlan(edl, durationMs) {
  const fps = Number(edl?.picture_contract?.canvas?.fps);
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error("EDL picture_contract.canvas.fps must be positive");
  }
  if (!Number.isInteger(durationMs) || durationMs <= 0) {
    throw new Error("scheduled duration must be a positive integer millisecond count");
  }
  const nominalDurationNs = BigInt(durationMs) * 1000000n;
  const scheduledStopAdvanceNs = BigInt(Math.round(1000000000 / fps));
  if (scheduledStopAdvanceNs <= 0n || scheduledStopAdvanceNs >= nominalDurationNs) {
    throw new Error("one-frame StopRecord advance is outside the recording duration");
  }
  return {
    pictureContractFps: fps,
    scheduledStopAdvanceFrames: 1,
    scheduledStopAdvanceNs,
    scheduledStopAdvanceMs: Number(scheduledStopAdvanceNs) / 1000000,
    nominalDurationNs,
    stopAfterStartNs: nominalDurationNs - scheduledStopAdvanceNs,
  };
}
