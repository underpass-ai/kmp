# kmp-viewer

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate is **ChronoLoom**, its local,
read-only visualizer: the same graph-temporal memory an agent reads, woven for
a human. Same facade, same semantics, no parallel read model.

## Memory you can see

ChronoLoom places memory on stable dimension lanes and offers three semantic
zoom levels: Atlas bins, Episode clusters and Moment entries. The
application owns this bounded, paginated projection; the browser never walks a
whole about or a depth-four graph to manufacture it.

Ask Codex or Claude to show the memory behind a decision: the agent can focus
the relevant evidence and light up its proof path in ChronoLoom. The view stays
shared — you can click, filter, pan, undo or take control yourself at any time.

Four reading moves sit on top of the loom:

- **Travel** — the whole timeline on a density strip; scrub it and the graph
  shows the memory as of that instant, the future simply not there yet.
- **Audit** — two clicks pick a claim and where it should lead; every hop of
  the kernel's trace renders with its why, evidence and confidence, the path
  glowing gradient ink from violet to green.
- **Search** — plain words or `kind:` / `dim:` / `id:`; Enter frames the
  hits and steps everything else back.
- **Lens and compare** — elapsed time remains proportional; event-density
  compresses silence with explicit scale breaks. Two pinned instants remain on
  screen with entry, relation, validity and evidence diffs.

Optional observability series share the same range, cursor and zoom. Every
series retains its exact unit and scope, and exemplars resolve to the operation
that emitted them; the viewer does not infer health or causality from temporal
alignment.

Edges wear their semantic class — causal blue, evidential green, temporal
violet, structural quiet — and node kinds wear the wire's own names in both
themes.

## Why in-process

Mounted inside the agent session, the viewer sees every write the moment it
projects and reads through the exact same kernel facade. There is no second
database connection, daemon, sync protocol or parallel interpretation of
memory.

`MemoryViewerServer` wraps one memory facade and serves it on a listener you
bind — generic over the same stores as the application service, so the
embedded edition mounts it over its selected local engine and another edition
can mount it over its own composition unchanged.

The usual way in is [`kmp-mcp`](https://crates.io/crates/kmp-mcp): every
embedded MCP session brings the viewer up over its own kernel at
`127.0.0.1:7317`, unasked. The printed link carries a random capability for
that process only; the first request exchanges it for an HttpOnly, SameSite
cookie and redirects to the clean URL. A local process with only the port gets
`401`. `KMP_VIEWER_ADDR` moves it; `off` declines it.
An MCP host that negotiates the Apps extension can instead open the identical
self-contained renderer from `ui://kmp/chronoloom.html`; no localhost browser
is required, and the bulk visual chunks stay in app structured content.

## A deliberately small surface

A hand-rolled HTTP/1.1 GET server bound to loopback, a UI compiled into the
binary with `include_str!`, and no dependency the embedded edition does not
already carry: no HTTP framework, no bundler, no CDN, nothing fetched at
runtime. The render engine is vendored and hash-verified in
`ui/vendor/VENDOR.md`.

Read-only means read-only: the viewer serves GETs and cannot write memory.

## License

Apache-2.0.
