# ADR-017 — In-process web viewer over the embedded kernel

Date: 2026-08-06. Status: accepted.

## Context

KMP memory is navigable by construction — wake, inspect, near, trace — but
until now only agents could navigate it. A human wanting to see what an agent
session knows had to read MCP tool JSON or the rendered context text. The
product needs an Obsidian-like window: the graph with its typed relations,
the note behind each node, and the two things Obsidian has no equivalent for —
known-at-time temporal movement and causal traces between nodes.

Two constraints shape where such a viewer can live:

- **ADR-011 single-writer.** The embedded store admits one process. A separate
  viewer binary can never watch a live agent session; it could only open the
  store the agent is *not* using.
- **ADR-013 small surface.** The embedded binary's dependency budget is
  guarded; an HTTP framework, a bundler, or a runtime asset fetch would all
  grow surface that the packaging decision deliberately excludes.

## Decision

- A new **`rehydration-viewer`** crate serves a read-only web UI over
  [`KernelMemoryApplicationService`], generic over the same store parameters,
  so it mounts over the embedded composition today without owning a parallel
  read model: `/api/graph` is `wake`, `/api/node` is `inspect`,
  `/api/timeline` is `temporal`, `/api/trace` is `trace`, `/api/abouts` is the
  facade's about index.
- **Mounted in-process** in `rehydration-mcp`: setting
  `REHYDRATION_VIEWER_ADDR` on an embedded session serves the viewer over
  that session's already-open kernel — the only live-view arrangement ADR-011
  admits. A `viewer [addr]` subcommand covers the offline case (no agent
  session holding the store), resolving the data dir exactly as
  `export`/`import` do.
- **Off by default, env-gated, not feature-gated.** ADR-013 describes a cargo
  feature discipline, but the current tree ships `rehydration-mcp` with a
  single dependency set and no CI dependency budget; a `viewer` feature would
  add build-matrix cost without an enforcement mechanism. The env var is the
  gate; revisiting a compile-time gate belongs with the ADR-013 CI work.
- **No HTTP framework.** A hand-rolled GET-only HTTP/1.1 loop over
  `tokio::net` (~250 lines) serves JSON and three embedded assets. Binding is
  refused unless the address is loopback; the `Host` header must name
  localhost (DNS-rebinding defense); every response carries
  `Cache-Control: no-store`, `X-Content-Type-Options: nosniff` and a CSP that
  forbids all non-self sources.
- **UI compiled into the binary** (`include_str!`): hand-written HTML/CSS/JS
  plus one vendored render engine, **pixi.js 8.19.0**, pinned by hash and
  supply-chain-verified in [`ui/vendor/VENDOR.md`](../../crates/rehydration-viewer/ui/vendor/VENDOR.md)
  — obtained as a plain registry artifact, never via `npm` (no lifecycle
  scripts execute; that was the propagation vector of the 2025–2026
  Shai-Hulud worm waves), checked against OSV/GitHub advisories and the
  published compromised-package lists before adoption.

## Why

- **Same facade, same semantics.** Every panel in the viewer is one of the
  kernel's own reads; the viewer cannot drift from what agents see because it
  has no read path of its own. Recalls it triggers carry `role=viewer` into
  the kernel's quality telemetry like any other consumer.
- **In-process is the honest deployment.** Anything else contradicts ADR-011
  or requires the daemon evolution that ADR speculates about; the viewer
  should not force that decision.
- **A vendored, hash-pinned asset is auditable; a package manager run is
  not.** The npm attack surface is the install step and the mutable registry
  pointer, not the artifact itself. One recorded sha512/sha256 pair and a
  documented re-verification procedure keep the artifact reviewable in diff
  form forever.

## Consequences

- **Positive:** live observation of an agent session's memory with zero new
  runtime dependencies in Rust (`serde`/`serde_json`/`tokio` were already the
  binary's floor).
- **Positive:** the temporal and trace surfaces get their first human client,
  which exercises exactly the semantics the MCP tools promise.
- **Trade-off:** the embedded binary grows by ~0.9 MB of embedded assets
  (0.8 MB of that is pixi.js). Acceptable against the ADR-009 size budget;
  recorded here so the next size audit knows where to look.
- **Trade-off:** a hand-rolled HTTP loop means no keep-alive, no streaming,
  GET only. For a loopback viewer over an in-process store, all three are
  features, not gaps.
- **Deferred:** authentication (loopback-only stands in for it), server
  editions (the generic crate compiles against any composition, but mounting
  it there is its own decision), and write operations of any kind.
