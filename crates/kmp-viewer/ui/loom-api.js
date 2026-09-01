/* KMP ChronoLoom — the backend port.
   The one door to the kernel: every read the loom makes goes through
   KMP_APP.api.call, which speaks HTTP to the sibling /api/* routes — or
   defers to the MCP Apps bridge when a host mounted one (KMP_APP_API).
   Nothing here invents data the kernel did not return. */
"use strict";

globalThis.KMP_APP = globalThis.KMP_APP || {};

KMP_APP.api = (() => {
  async function call(path, params, method = "GET") {
    if (globalThis.KMP_APP_API) return globalThis.KMP_APP_API(path, params || {}, method);
    const query = params
      ? "?" +
        Object.entries(params)
          .filter(([, v]) => v !== undefined && v !== null && v !== "")
          .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
          .join("&")
      : "";
    const response = await fetch(path + query, { method });
    const body = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(body.error || `${path} failed with ${response.status}`);
    return body;
  }

  /* The whole span the kernel could ever hold; the extent probe's frame. */
  const EXTENT_FROM = "1900-01-01T00:00:00Z";
  const EXTENT_TO = "2100-01-01T00:00:00Z";

  async function fetchProjection(about, axis, from, to, lod = "atlas", bins = 128) {
    const params = {
      about,
      from: from || EXTENT_FROM,
      to: to || EXTENT_TO,
      lod,
      bins,
      limit: 2048,
    };
    if (axis) params.axis = axis;
    return call("/api/projection", params);
  }

  return { call, fetchProjection, EXTENT_FROM, EXTENT_TO };
})();
