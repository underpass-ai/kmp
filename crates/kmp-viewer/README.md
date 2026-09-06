# kmp-viewer

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate is **ChronoLoom**, its local,
read-only visualizer: the same graph-temporal memory an agent reads, woven for
a human. Same facade, same semantics, no parallel read model.

## Memory you can see

ChronoLoom places each memory once on its owning about plane. Perspective orbit
and a flat camera share the same exact UTC window and selection. Up to five
additional abouts can be explicitly compared; ordinary labels filter entries.
The three semantic detail levels remain: Atlas bins, Episode clusters and Moment entries. The
application owns this bounded, paginated projection; the browser never walks a
whole about or a depth-four graph to manufacture it.

Ask Codex or Claude to show the memory behind a decision: the agent can focus
the relevant evidence and light up its proof path in ChronoLoom. The view stays
shared. A prominent human/agent panel names the last actor and the reason for
its move. Taking control checks the revision you saw, keeps the frame and
preserves undo of the actual semantic move. This attribution is not an exclusive
lock or a claim that an agent is connected.

Loading feedback covers clock changes, added or removed about layers, and
evidence reads. The previous content dims while reads finish, then fades into
the updated scene. Reduced-motion preferences disable the animation; the
status remains visible and is announced to assistive technology.

Four reading moves sit on top of the loom:

- **Travel** — a shared time window on the density strip; move or resize it
  to compare the same interval across abouts on the selected clock.
- **Audit** — two clicks pick a claim and where it should lead; every hop of
  the kernel's trace renders with its why, evidence and confidence, the path
  highlighted in the scene.
- **Search** — plain words or `kind:` / `dim:` / `id:`; Enter frames the
  hits and steps everything else back.
- **Lens and compare** — elapsed time remains proportional; event-density
  compresses silence on a non-uniform axis. Two pinned instants remain on
  screen with entry, relation, validity and evidence diffs.

Optional observability series share the same time range. Every
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
`127.0.0.1:7317`, unasked. When another session already owns that default,
the new process takes a free loopback port instead. The session's printed
link carries a random capability for that process only; `kmp_view_open` and `kmp_view_get_state`
return the same link when the host hides server output. The first browser
request exchanges it for an HttpOnly, SameSite cookie and redirects to the
clean URL. A local process with only the port gets `401`. `KMP_VIEWER_ADDR`
moves it to an explicit address (and refuses if that address is busy); `off`
declines it.
An MCP host that negotiates the Apps extension can instead open the identical
self-contained renderer from `ui://kmp/chronoloom.html`; no localhost browser
is required, and the bulk visual chunks stay in app structured content.

## A deliberately small surface

A hand-rolled HTTP/1.1 GET server bound to loopback, a UI compiled into the
binary with `include_str!`, and no dependency the embedded edition does not
already carry: no HTTP framework, no runtime bundler, no CDN, nothing fetched at
runtime. Three.js and OrbitControls are vendored as one pinned bundle. The render engine is vendored and hash-verified in
`ui/vendor/VENDOR.md`.

Memory reads use GET. Explicit POST routes mutate only the shared view aggregate,
including human handoff and undo; the viewer cannot write memory.

See the [architecture and verification map](../../docs/architecture/chronoloom-shared-scene.md).

## License

Apache-2.0.
