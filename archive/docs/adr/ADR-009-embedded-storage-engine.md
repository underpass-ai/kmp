# ADR-009: redb as the embedded edition storage engine

**Status:** Accepted
**Date:** 2026-07-21
**Context:** [KMP Embedded Edition Roadmap](../product/kmp-embedded-edition-roadmap.md), milestone E0

## Decision

The embedded edition's storage engine is **redb** (pure-Rust, single-file,
MVCC, typed B-tree tables). All embedded port implementations in
`kmp-adapter-embedded` (E2) persist through one redb database file.

SQLite (via `rusqlite`) is the documented fallback if redb hits a wall during
E2; fjall is rejected for this use case.

## Candidates

Evaluated per the E0 deliverable: `redb` 4.1.0, `fjall` 3.1.8, SQLite via
`rusqlite` 0.40.1 (`bundled`). Criteria: single-file/single-dir layout, crash
safety, Windows support, binary size, license — plus one criterion the
roadmap's product direction makes load-bearing: **reopen time**, because the
MCP stdio binary is launched fresh by the agent host per session, so store
open cost is paid at every session start.

## Spike results

Benchmark source and raw output: [spikes/e0-storage-spike/](spikes/e0-storage-spike/README.md).

Workload shaped like the embedded write path — per event: ~1KB event body
append (`ContextEventStore`) + dedup index entry (`ProcessedEventStore`) +
~400B node detail upsert + two ~150B adjacency edge inserts (synchronous
projection). Corpus: 102,000 events (2,000 committed durably one by one,
100,000 in batches of 1,000), 20,000 distinct nodes, ~204,000 edges — the
E2 exit-criteria scale.

| engine | per-event durable (ev/s) | batched (ev/s) | reopen (ms) | point reads (r/s) | adjacency scans (scan/s) | size (MB) |
| --- | --- | --- | --- | --- | --- | --- |
| redb 4.1.0 | 265 | 29,088 | 2.9 | 846,775 | 212,957 | 249.3 |
| fjall 3.1.8 | 294 | 32,135 | 1,329.9 | 1,044,037 | 140,335 | 263.1 |
| SQLite 0.40.1 (WAL, synchronous=FULL) | 293 | 33,038 | 0.3 | 176,417 | 165,140 | 151.6 |

Stripped release binary cost over a 344KB baseline binary (linux x86_64,
glibc): redb **+0.70MB**, fjall +1.25MB, SQLite bundled +2.14MB.

Environment: AMD Ryzen Threadripper PRO 5955WX, NVMe SSD, btrfs, Linux 7.0,
rustc 1.95.0, single-threaded writer. Adjacency scans returned identical edge
counts (10,214 over 1,000 scans) on all three engines, cross-checking
workload equivalence.

Caveats recorded with the numbers: single run per engine; reads hit a warm
page cache; sizes are apparent file sizes; fjall's reopen might improve with
explicit pre-close compaction, but that is tuning the embedded composition
root should not have to own.

## Why redb

- **Reopen is session-start cost.** 2.9ms is indistinguishable from zero;
  fjall's 1.3s journal/manifest recovery is two orders of magnitude worse
  and would be paid by every agent session on a corpus that only grows.
- **Write throughput is not the differentiator.** Per-event durable ingest is
  fsync-bound and statistically identical across engines (~265–294 ev/s —
  far above interactive `kmp_write_memory` rates). Batched ingest
  (~29–33k ev/s) replays a 100k-event log in ~3.5s on all three.
- **Fastest at the query shape that matters.** The port surface is
  neighborhood-shaped ([ADR-010](ADR-010-embedded-graph-representation.md));
  redb's typed tuple keys (`(&str, u64)`) give native adjacency prefix ranges
  and won the scan benchmark.
- **Single file, pure Rust.** One `kernel.redb` file satisfies the data-dir
  contract trivially; no C toolchain in the build, so static musl and Windows
  targets stay simple, and the binary-size budget (E3: single-digit MB
  stripped) keeps ~1.4MB of headroom that SQLite would spend.
- **Durability model matches runtime guarantees.** Commits are fsync-durable
  by default (matching the "no data loss beyond the in-flight event" E2 exit
  criterion) and MVCC gives snapshot-isolated readers over one writer.
- **License:** MIT/Apache-2.0, compatible with distribution plans (E5).

## Why not the others

- **fjall:** LSM strengths (write-heavy workloads, background compaction
  threads) do not fit a short-lived, single-writer stdio process; reopen cost
  and multi-file layout are both product regressions here. Rejected for this
  use case, not on general quality.
- **SQLite:** excellent numbers (best reopen, smallest file) and the fallback
  of record, but it is the only candidate with a C dependency — complicating
  the static musl/Windows story the roadmap explicitly flags — costs 3× the
  binary size of redb, and was the slowest at point reads on this workload.
  The roadmap's stated bias ("SQLite only if the spike shows the pure-Rust
  options fail requirements") is not triggered: redb failed nothing.

## Consequences

- **Positive:** one-file store, ms-level session start, no C toolchain,
  smallest binary cost, fsync-durable commits by default.
- **Trade-off:** largest on-disk footprint of the three (249MB vs 152MB for
  SQLite at 102k events, ~65% B-tree overhead over logical data). Accepted at
  this scale; compaction/rebuild tooling is already an E2 deliverable.
- **Trade-off:** redb is younger than SQLite. Mitigated by the append-only
  event log being the source of truth: projections are rebuildable, and the
  store format version is stamped and fail-fast checked
  ([ADR-012](ADR-012-embedded-data-directory.md)).
- **Bounded blast radius:** all redb types stay inside
  `kmp-adapter-embedded`; the conformance suite (E1) is
  engine-agnostic, so revisiting this decision is an adapter swap, not a
  redesign.

## Next Step

E1 conformance suite first (the keystone), then E2 implements the embedded
adapters on redb and re-validates the E2 exit criteria (kill -9 replay,
100k-corpus reopen/size) on the real adapter code rather than this spike's
approximation.
