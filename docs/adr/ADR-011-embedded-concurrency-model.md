# ADR-011: Single-writer store with fail-fast lock; daemon as the documented evolution

**Status:** Accepted
**Date:** 2026-07-21
**Context:** [KMP Embedded Edition Roadmap](../product/kmp-embedded-edition-roadmap.md), milestone E0

## Decision

v1 of the embedded edition uses **one exclusive owner process per data
directory** (roadmap option a):

- the first `kmp-mcp` embedded process to open a data directory takes
  an exclusive advisory lock on a `lock` file at the data-dir root and owns
  the store (reads and writes) until exit;
- a second process opening the same data directory **fails fast at startup**
  with an explicit, actionable error (which process holds the lock, since
  when, and what the user can do) — never silent empty memory, never a
  corrupted store, and never blocking-wait by default;
- the lock is advisory OS file locking (`flock`/`LockFileEx` semantics via a
  crate that abstracts both), so a crashed owner releases it automatically
  with the process — no stale-lock cleanup protocol.

A tiny local daemon owning the store, with stdio shims connecting to it
(roadmap option c), is the **documented evolution path** if concurrent
sessions prove common; per-session stores with merge-on-read (option b) are
**rejected**.

## Why

- **The failure mode to design for is two agent windows on one repo.** Two
  Claude Code sessions on the same project is realistic but occasional; v1
  optimizes for correctness and an explicit UX in that case, not for
  concurrent throughput.
- **The engine already enforces it.** redb
  ([ADR-009](ADR-009-embedded-storage-engine.md)) is single-process by
  design; the composition-root lock does not add a restriction, it moves the
  collision to startup where it can produce a domain-shaped, fail-fast error
  instead of an engine error surfacing mid-session.
- **Fail-fast is a roadmap non-negotiable.** "Locked store" is explicitly
  listed alongside corrupt store and version mismatch as an error that must
  be explicit.
- **Merge-on-read (b) is rejected on semantics**, not implementation cost:
  two independent stores would each assign event order, and merging them
  breaks known-at-time reads, replay determinism, and relation proof — the
  properties that are the product. A wrong-memory bug from a bad merge is
  strictly worse than a clear "store is in use" error.

## Evolution path: local daemon (c)

Recorded now so it is designed toward, not improvised later:

- the stdio binary's embedded backend already isolates the kernel behind the
  MCP tool dispatch seam; a daemon splits that same seam across a local
  transport (unix socket / named pipe), with stdio shims per session;
- the daemon owns the store lock, so the v1 locking contract is unchanged —
  the daemon is just the one owner process;
- trigger for building it: E4 host-integration testing showing that
  concurrent sessions on one store are a common workflow, not an edge case.

Until then, the second session's error message is the UX (E4 deliverable
documents it per host).

## Consequences

- **Positive:** no corruption window, no merge semantics, no lock-recovery
  protocol; crash of the owner releases the lock automatically.
- **Positive:** the conformance suite (E1) needs no concurrency scenarios
  beyond one writer + snapshot readers, matching what the engine guarantees.
- **Trade-off:** a second simultaneous session on the same project does not
  get memory access; it gets a clear error. Accepted for v1 and measured
  against real usage in E4.
- **Trade-off:** advisory locks do not protect against a user copying the
  data directory while open; the store format's crash safety covers the
  copy-while-writing case (a copy is at worst a crashed store, which replays).

## Next Step

E2 implements the lock in `kmp-adapter-embedded`'s composition
surface with the error taxonomy (locked / corrupt / version mismatch); E4
validates the two-session UX on Claude Code and Codex CLI and records the
daemon trigger evidence.
