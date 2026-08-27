/* MCP Apps transport adapter for the otherwise host-independent ChronoLoom. */
"use strict";

(() => {
  let nextId = 1;
  let currentAbout = null;
  let currentViewId = "default";
  let reportSequence = 0;
  const pending = new Map();
  const aboutWaiters = [];

  function request(method, params) {
    const id = nextId++;
    const promise = new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    window.parent.postMessage({ jsonrpc: "2.0", id, method, params }, "*");
    return promise;
  }

  function notify(method, params = {}) {
    window.parent.postMessage({ jsonrpc: "2.0", method, params }, "*");
  }

  window.addEventListener("message", (event) => {
    const message = event.data;
    if (!message || message.jsonrpc !== "2.0") return;
    if (message.id !== undefined && pending.has(message.id)) {
      const waiter = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) waiter.reject(new Error(message.error.message || JSON.stringify(message.error)));
      else waiter.resolve(message.result);
      return;
    }
    if (message.method === "ui/notifications/tool-input") {
      currentAbout = message.params && message.params.arguments && message.params.arguments.about;
      currentViewId =
        (message.params && message.params.arguments && message.params.arguments.view_id) || "default";
      while (aboutWaiters.length) aboutWaiters.shift()(currentAbout);
    }
    if (message.method === "ui/notifications/host-context-changed") {
      const theme = message.params && message.params.theme;
      if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;
    }
    if (message.method === "ui/resource-teardown" && message.id !== undefined) {
      window.parent.postMessage({ jsonrpc: "2.0", id: message.id, result: {} }, "*");
    }
  });

  const ready = request("ui/initialize", {
    protocolVersion: "2026-01-26",
    appCapabilities: { availableDisplayModes: ["inline", "fullscreen"] },
    clientInfo: { name: "kmp-chronoloom", version: "1" },
  }).then((result) => {
    const theme = result && result.hostContext && result.hostContext.theme;
    if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;
    notify("ui/notifications/initialized");
    return result;
  });

  async function about() {
    await ready;
    if (currentAbout) return currentAbout;
    try {
      const state = await callTool("kmp_view_get_state", { view_id: currentViewId });
      currentAbout = state.state && state.state.about;
    } catch (_) {
      // Tool input normally follows ui/notifications/initialized.
    }
    if (currentAbout) return currentAbout;
    return new Promise((resolve) => {
      aboutWaiters.push(resolve);
      setTimeout(() => resolve(currentAbout), 1500);
    });
  }

  async function callTool(name, args) {
    await ready;
    const result = await request("tools/call", { name, arguments: args });
    if (result && result.isError) {
      const text = result.content && result.content[0] && result.content[0].text;
      throw new Error(text || `${name} failed`);
    }
    return (result && result.structuredContent) || {};
  }

  const projectionArgs = (params) => ({
    about: params.about || currentAbout,
    from: params.from || "1900-01-01T00:00:00Z",
    to: params.to || "2100-01-01T00:00:00Z",
    lod: params.lod || "moment",
    bins: Number(params.bins || 128),
    limit: Number(params.limit || 2048),
    ...(params.axis ? { axis: params.axis } : {}),
    ...(params.cursor ? { cursor: params.cursor } : {}),
  });

  const nodeFromEntry = (entry) => ({
    id: entry.ref_id,
    kind: entry.kind,
    title: entry.text,
    summary: entry.text,
    status: "",
    labels: [],
    properties: {},
  });
  const edgeFromRelation = (edge) => ({ ...edge, source: edge.from, target: edge.to });

  async function appApi(path, params, method) {
    await ready;
    if (path === "/api/info") return { kernel_version: "mcp-app" };
    if (path === "/api/abouts") {
      const active = await about();
      return { abouts: active ? [active] : [] };
    }
    if (path === "/api/projection") {
      return callTool("kmp_view_read_projection", projectionArgs(params));
    }
    if (path === "/api/graph") {
      const projection = await callTool("kmp_view_read_projection", projectionArgs({ ...params, lod: "moment" }));
      return {
        about: projection.about,
        root_id: projection.about,
        revision: projection.revision,
        content_hash: projection.content_hash,
        nodes: (projection.entries || []).map(nodeFromEntry),
        edges: (projection.relations || []).map(edgeFromRelation),
        details: [],
        rendered: { content: "Visual projection", token_count: 0 },
        quality: {},
      };
    }
    if (path === "/api/node") {
      const inspect = await callTool("kmp_inspect", {
        ref: params.id,
        include: { incoming: true, outgoing: true, details: true, raw: true },
        budget: { max_bytes: 65536 },
      });
      const object = inspect.object || {};
      const raw = (inspect.raw || [])[0] || {};
      return {
        node: {
          id: object.ref,
          kind: object.kind,
          title: object.text,
          summary: object.text,
          status: "",
          labels: [],
          properties: object.metadata || {},
        },
        detail: { id: object.ref, detail: raw.detail || object.text || "", revision: raw.revision || 0 },
        incoming: ((inspect.links && inspect.links.incoming) || []).map(edgeFromRelation),
        outgoing: ((inspect.links && inspect.links.outgoing) || []).map(edgeFromRelation),
        evidence: inspect.evidence || [],
        raw: inspect.raw || [],
      };
    }
    if (path === "/api/nodes") {
      const ids = String(params.ids || "").split(",").filter(Boolean);
      const nodes = [];
      const missing = [];
      for (const id of ids) {
        try { nodes.push((await appApi("/api/node", { id })).node); }
        catch (_) { missing.push(id); }
      }
      return { nodes, missing };
    }
    if (path === "/api/trace") {
      const trace = await callTool("kmp_trace", { from: params.from, to: params.to });
      return {
        from: params.from,
        to: params.to,
        nodes: [params.from, params.to].map((id) => ({ id, title: id, kind: "memory" })),
        edges: (trace.trace || []).map(edgeFromRelation),
        rendered: { content: trace.summary || "", token_count: 0 },
        quality: trace.quality || {},
      };
    }
    if (path === "/api/observability") {
      const names = String(params.series || "").split(",").filter(Boolean);
      return { contract: "kmp.observability.projection.v1", series: [], exemplars: [], missing: names, truncated: false };
    }
    if (path === "/api/view") {
      const args = { view_id: params.id || currentViewId };
      let result = await callTool("kmp_view_get_state", args);
      let state = result.state || result;
      if (params.since && Number(params.since) === Number(state.view_revision)) {
        await new Promise((resolve) => setTimeout(resolve, 750));
        result = await callTool("kmp_view_get_state", args);
        state = result.state || result;
      }
      return state;
    }
    if (path === "/api/view/open") {
      const result = await callTool("kmp_view_open", {
        about: params.about,
        view_id: params.id || currentViewId,
        expected_revision: Number(params.expected_revision) || undefined,
      });
      return result.state || result;
    }
    if (path === "/api/view/report") {
      const result = await callTool("kmp_view_apply_intent", {
        view_id: params.id || currentViewId,
        idempotency_key: `chronoloom-human-${Date.now()}-${reportSequence++}`,
        actor: "human",
        target: { about: params.about || currentAbout },
        focus: { time_range: { axis: params.clock, from: params.from, to: params.to } },
        selection: params.selection || null,
        trace: params.trace_from && params.trace_to ? { from: params.trace_from, to: params.trace_to } : null,
        search: params.search || null,
      });
      return result.state || result;
    }
    if (path === "/api/view/undo") {
      const result = await callTool("kmp_view_undo", { view_id: params.id || currentViewId });
      return result.state || result;
    }
    throw new Error(`MCP App route ${path} is not implemented`);
  }

  globalThis.KMP_APP_API = appApi;
})();
