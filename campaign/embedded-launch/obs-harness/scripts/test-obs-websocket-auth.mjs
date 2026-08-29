#!/usr/bin/env node
"use strict";

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { obsWebSocketAuthentication } from "./obs-websocket-auth.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDir, "../../../..");
const scratchParent = path.join(root, "tmp", "evidence-pack", "capture", "runs");
fs.mkdirSync(scratchParent, { recursive: true });

assert.equal(
  obsWebSocketAuthentication(
    "correct horse battery staple",
    "obs-salt-vector",
    "obs-challenge-vector",
  ),
  "gt5fnCs4M2w3P1fy6nhZu3u+Cz3hf8ri1YMUjt7VJcU=",
  "OBS 5.x protocol vector changed",
);
assert.throws(() => obsWebSocketAuthentication("", "salt", "challenge"), TypeError);

const runDir = fs.mkdtempSync(path.join(scratchParent, "auth-test-"));
try {
  const prepared = spawnSync(
    process.execPath,
    [path.join(scriptDir, "prepare-run.mjs"), runDir, "45120"],
    { encoding: "utf8" },
  );
  assert.equal(prepared.status, 0, prepared.stderr);
  const passwordPath = path.join(runDir, "control", "obs-password.private");
  const password = fs.readFileSync(passwordPath, "utf8").trim();
  const auth = JSON.parse(fs.readFileSync(path.join(runDir, "obs-auth.json"), "utf8"));
  assert.deepEqual(Object.keys(auth).sort(), ["auth_required", "cleartext_retained", "port"]);
  assert.equal(auth.auth_required, true);
  assert.equal(auth.cleartext_retained, false);

  const sanitized = spawnSync(
    process.execPath,
    [path.join(scriptDir, "sanitize-run.mjs"), runDir],
    { encoding: "utf8" },
  );
  assert.equal(sanitized.status, 0, sanitized.stderr);
  assert.equal(fs.existsSync(passwordPath), false);
  const retainedText = [...walk(runDir)]
    .filter((file) => [".ini", ".json", ".jsonl", ".log", ".txt"].includes(path.extname(file)))
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n");
  assert.equal(retainedText.includes(password), false);
  assert.match(retainedText, /ServerPassword=<ephemeral-redacted>/);
} finally {
  fs.rmSync(runDir, { recursive: true, force: true });
}

console.log("OBS WebSocket auth: protocol vector and non-persistence PASS");

function* walk(directory) {
  for (const item of fs.readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, item.name);
    if (item.isDirectory()) yield* walk(target);
    else if (item.isFile()) yield target;
  }
}
