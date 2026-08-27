# kmp-viewer

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate is its local, read-only web
viewer: the graph, its notes, the timeline and causal traces — rendered for a
human the way
`kmp_wake`, `kmp_inspect`, `kmp_near` and `kmp_trace` render them
for an agent. Same facade, same semantics, no parallel read model.

## What the viewer does

A memory of a thousand entries opens as a map, not a hairball: dimensions
fold into meta-marks sized by what they hold, and unfold in place on a
double-click. The layout is deterministic — the same store draws the same
picture — and stable at real scale: degree-normalized springs, Barnes-Hut
repulsion, a hard velocity cap, fit-to-view on settle and on `F`.

Three stories sit on top of the graph:

- **Travel** — the whole timeline on a density strip; scrub it and the graph
  shows the memory as of that instant, the future simply not there yet.
- **Audit** — two clicks pick a claim and where it should lead; every hop of
  the kernel's trace renders with its why, evidence and confidence, the path
  glowing gradient ink from violet to green.
- **Search** — plain words or `kind:` / `dim:` / `id:`; Enter frames the
  hits and steps everything else back.

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
`127.0.0.1:7317`, unasked. `KMP_VIEWER_ADDR` moves it; `off` declines it.

## A deliberately small surface

A hand-rolled HTTP/1.1 GET server bound to loopback, a UI compiled into the
binary with `include_str!`, and no dependency the embedded edition does not
already carry: no HTTP framework, no bundler, no CDN, nothing fetched at
runtime. The render engine is vendored and hash-verified in
`ui/vendor/VENDOR.md`.

Read-only means read-only: the viewer serves GETs and cannot write memory.

## License

Apache-2.0.
