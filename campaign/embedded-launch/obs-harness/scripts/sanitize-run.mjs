#!/usr/bin/env node
"use strict";

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const [runDir] = process.argv.slice(2);
if (!runDir || !runDir.includes("/evidence-pack/capture/runs/")) {
  throw new Error(`refusing to sanitize outside capture runs: ${runDir || "<missing>"}`);
}

function sha256(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

const textSuffixes = new Set([".ini", ".json", ".jsonl", ".log", ".txt", ".typescript", ".timing"]);
function visit(dir) {
  if (!fs.existsSync(dir)) return;
  for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
    const absolute = path.join(dir, item.name);
    if (item.isDirectory()) visit(absolute);
    else if (item.isFile() && textSuffixes.has(path.extname(item.name))) {
      let value = fs.readFileSync(absolute, "utf8");
      value = value.replace(/([?&]k=)([0-9a-f]{64})/gi, (_, prefix, capability) =>
        `${prefix}<redacted:sha256:${sha256(capability)}>`
      );
      value = value.replace(/^ServerPassword=.*$/m, "ServerPassword=<ephemeral-redacted>");
      fs.writeFileSync(absolute, value);
    }
  }
}
visit(runDir);

for (const target of [
  path.join(runDir, "control", "obs-password.private"),
  path.join(runDir, "control", "viewer-url.private"),
  path.join(runDir, "xauthority.private"),
  path.join(runDir, "browser-profile.private"),
  path.join(runDir, "obs-cache.private"),
]) {
  fs.rmSync(target, { recursive: true, force: true });
}
