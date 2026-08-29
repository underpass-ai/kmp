#!/usr/bin/env node
"use strict";

import fs from "node:fs";

const [file, extraJson = "{}"] = process.argv.slice(2);
if (!file) throw new Error("usage: signal.mjs FILE [JSON]");
const extra = JSON.parse(extraJson);
fs.writeFileSync(file, `${JSON.stringify({
  wall_time: new Date().toISOString(),
  monotonic_ns: process.hrtime.bigint().toString(),
  ...extra,
})}\n`, { mode: 0o600 });
