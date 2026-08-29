#!/usr/bin/env node
"use strict";

import crypto from "node:crypto";

export function obsWebSocketAuthentication(password, salt, challenge) {
  if (![password, salt, challenge].every((value) => typeof value === "string" && value)) {
    throw new TypeError("OBS WebSocket authentication requires password, salt and challenge");
  }
  // OBS WebSocket 5.x mandates this exact two-stage SHA-256 challenge response.
  // The random run-scoped password is never stored and this is not a password KDF.
  // codeql[js/insufficient-password-hash]
  const secret = crypto.createHash("sha256").update(password + salt).digest("base64");
  // This second SHA-256 is likewise part of the protocol wire response, not storage.
  // codeql[js/insufficient-password-hash]
  return crypto.createHash("sha256").update(secret + challenge).digest("base64");
}
