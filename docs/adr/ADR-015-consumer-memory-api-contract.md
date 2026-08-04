# ADR-015: `rehydration-memory-api` is the contract an embedding product compiles against

**Status:** Accepted
**Date:** 2026-08-04
**Context:** [KMP Embedded Edition Roadmap](../product/kmp-embedded-edition-roadmap.md), consumer integration after E4

## Decision

Products that embed the kernel have been importing `rehydration-embedded` and,
with it, the domain: `rehydration-domain` aggregates and
`rehydration-application` queries crossing into a consumer's dependency graph,
where every internal change of ours is a possible break of theirs. The kernel
promised its consumers stability, but there was no artifact whose version that
promise attached to.

`rehydration-memory-api` is a leaf crate holding the published consumer
contract, and nothing else. It is the sibling of `rehydration-plugin-api`,
pointing the other way: that crate is what a plugin may know about the kernel,
this one is what an embedding product may know.

- **Plain views** (`MemoryRecallView`, `MemoryNodeView`, `MemoryDetailView`,
  `RenderedMemoryView`) — projections a consumer can hold, log or map into its
  own vocabulary without importing the domain. The rendered context travels
  with its content hash, so a consumer can verify a model received exactly
  what the kernel rendered.
- **A capability report** (`ApiCapabilities`), stated by the implementation and
  checked by consumers at startup: two builds of one release can differ in
  features, and a version string cannot say so.
- **An error vocabulary** (`ApiError`) that publishes whether each failure is
  worth retrying, so consumers do not keep their own staleness-prone tables.
- **One trait** (`MemoryRecallApi`): `wake` and `ask`, reads only. Its methods
  return named `Send` futures rather than `async fn`, so the trait is
  consumable from generic code on multi-threaded runtimes without adding an
  `async-trait` dependency this workspace does not otherwise carry.

`CONTRACT_VERSION` moves on meaning, not on release: adding a capability keeps
the version, changing what an existing field or method means raises it.

Mutations are deliberately absent from v1. Ingestion, projection and export
have their own commands, idempotency and provenance inside the kernel; a
consumer that needs them coordinates through the kernel's own surfaces. The
contract grows by adding named capabilities, never by widening what an
existing one means.

The crate depends on nothing of the kernel. `rehydration-embedded` implements
the trait for `EmbeddedKernel`; the dependency arrow points from
implementation to contract, never back.

The answer policy's default is the strict one, `EvidenceOrUnknown`, and the
contract keeps it: a memory that answers beyond its evidence is worse than one
that says it does not know, and a consumer that wants a looser policy asks for
it by name.

## Consequences

- A consumer compiles against `rehydration-memory-api` plus an implementation
  crate, and is testable against a stub of the trait alone.
- The recall tiers are named in the contract (`Summary`, `CausalSpine`,
  `EvidencePack`) without exporting the domain's resolution ladder; what each
  rung costs remains the implementation's business.
- Everything the kernel does beyond this contract remains reachable through
  `rehydration-embedded`'s own surfaces, unversioned and unpromised. A
  consumer that keeps using those is choosing coupling, and the line between
  the two is now visible in its `Cargo.toml`.
- The known first consumer can bind its own memory port to this trait and stop
  importing kernel domain types; its deep adapter keeps working unchanged
  until it migrates.
