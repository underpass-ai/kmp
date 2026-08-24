# kmp-viewer

A local, read-only web viewer over
[KMP by Underpass](https://github.com/underpass-ai/kmp) memory: the graph, its
notes, the timeline and causal traces — rendered for a human the way
`kernel_wake`, `kernel_inspect`, `kernel_near` and `kernel_trace` render them
for an agent. Same facade, same semantics, no parallel read model.

## Why in-process

The embedded store is single-writer, so a separate viewer process could never
watch a live agent session — it would be locked out of the file the session
owns. Mounted inside the session's own process, the viewer sees every write
the moment it projects.

`MemoryViewerServer` wraps one memory facade and serves it on a listener you
bind — generic over the same stores as the application service, so the
embedded edition mounts it over redb today and another edition can mount it
over its own composition unchanged.

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
