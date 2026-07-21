# ADR-013: One binary — `rehydration-mcp` gains an `embedded` backend behind a cargo feature

**Status:** Accepted
**Date:** 2026-07-21
**Context:** [KMP Embedded Edition Roadmap](../product/kmp-embedded-edition-roadmap.md), milestone E0

## Decision

The embedded edition ships as the **existing `rehydration-mcp` binary with a
third backend**, not as a separate binary or fork:

- `REHYDRATION_MCP_BACKEND=embedded` joins `live` (gRPC) and `fixture` in
  `rehydration-mcp/src/backend.rs`, implementing the same
  `KernelMcpToolBackend` seam;
- the in-process kernel composition (application services + embedded
  adapters + synchronous projection) lives in a new **`rehydration-embedded`
  composition-root crate**, so `rehydration-mcp` stays a thin protocol
  binary and the wiring is testable on its own;
- the backend is gated by a cargo feature **`embedded`**, enabled by default
  for the installable binary (`cargo install rehydration-mcp` gets it), so
  the embedded dependency graph is compile-time excluded from server-only
  builds;
- **forbidden-dependency budget, enforced in CI (E3):** with `embedded` on
  and `live` off, the binary must not link `tonic` servers, `neo4rs`,
  `async-nats`, or any infrastructure adapter crate — checked via
  `cargo tree` in CI next to the recorded binary-size budget;
- **branding is deferred to E5**, as the roadmap prescribes. If distribution
  wants a friendlier command, it ships as an alias/wrapper of this same
  binary (candidate name: `kmp`) — never as a second implementation.

## Why

- **"Same product, not a fork" must be structural.** One binary, one MCP tool
  surface, one dispatch seam means edition parity is enforced by the compiler
  and the conformance suite (E1), and a client config switches editions by
  changing one environment variable — the promotion story (E6) verbatim.
- **The seam already exists.** `backend.rs` dispatches `fixture` vs `live`
  behind `KernelMcpToolBackend` today; embedded is a third implementor, which
  is the smallest honest change.
- **A separate binary would drift.** Two binaries mean two registration
  recipes per host (E4), two release artifacts per platform (E5), and a
  standing invitation for tool-schema divergence — the exact bug class the
  roadmap calls out.
- **Feature-gating protects both directions:** server images don't carry the
  embedded engine, and the embedded binary can't quietly grow
  infrastructure clients (roadmap non-negotiable "small surface"; the redb
  cost measured in [ADR-009](ADR-009-embedded-storage-engine.md) keeps the
  single-digit-MB budget realistic).

## Consequences

- **Positive:** zero new public surface — hosts, docs, and the existing
  [mcp-stdio](../operations/mcp-stdio.md) operations story extend rather than
  duplicate.
- **Positive:** `rehydration-embedded` gives E2/E3 a home for data-dir
  resolution (ADR-012), locking (ADR-011), and synchronous projection wiring
  without widening `rehydration-mcp`'s responsibilities.
- **Trade-off:** feature flags add build-matrix complexity; contained by CI
  jobs building both feature sets plus the forbidden-dependency check.
- **Trade-off:** default-on `embedded` grows the default build slightly for
  contributors; accepted, since the default `cargo install` artifact is the
  product the roadmap is aiming at.

## Next Step

E3 lands the `embedded` arm in `backend.rs`, the `rehydration-embedded`
crate, the feature split, and the CI size + forbidden-dependency gates; E5
revisits naming with real distribution artifacts on the table.
