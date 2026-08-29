#!/usr/bin/env node
"use strict";

import crypto from "node:crypto";

export function obsWebSocketAuthentication(credential, salt, challenge) {
  if (![credential, salt, challenge].every((value) => typeof value === "string" && value)) {
    throw new TypeError("OBS WebSocket authentication requires credential, salt and challenge");
  }
  // OBS WebSocket 5.x mandates this exact two-stage SHA-256 challenge response.
  // The random run-scoped credential is never stored and this is not a KDF.
  const secret = crypto.createHash("sha256").update(credential + salt).digest("base64");
  // This second SHA-256 is likewise part of the protocol wire response, not storage.
  return crypto.createHash("sha256").update(secret + challenge).digest("base64");
}
