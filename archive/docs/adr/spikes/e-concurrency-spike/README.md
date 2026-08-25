# Concurrency spike — can two agent hosts share one embedded store?

Source of the numbers recorded in
[ADR-018](../../ADR-018-multi-process-embedded-store.md). Run on 2026-08-16
(Linux 6.17, NVMe SSD, rustc via the workspace toolchain).

[`bench.rs`](bench.rs) is a standalone binary, kept out of the workspace on
purpose — it is evidence, not product code. It spawns **real OS processes**,
which is the part [ADR-009](../../ADR-009-embedded-storage-engine.md)'s
single-process benchmark could not answer.

To reproduce:

```bash
cargo new spike-bench && cd spike-bench
cp <this dir>/bench.rs src/main.rs
cargo add redb@4.1.0
cargo add rusqlite@0.40.1 --features bundled
cargo run --release -- ./data          # concurrency
cargo run --release -- ./data bench    # ADR-009 criteria, re-measured
```

## Concurrency: two processes, one store, 300 durable events each

```
## redb
  RESULT writer=1 engine=redb opened=false written=0 error=Database already open. Cannot acquire lock.
  RESULT writer=2 engine=redb opened=true written=300 seconds=1.009
  processes that opened the store: 1/2
  events durably written: 300/600

## sqlite
  RESULT writer=1 engine=sqlite opened=true written=300 seconds=2.824
  RESULT writer=2 engine=sqlite opened=true written=300 seconds=1.433
  processes that opened the store: 2/2
  events durably written: 600/600

## sqlite — reader concurrent with writer
  RESULT writer=1 engine=sqlite opened=true written=400 seconds=1.821
  RESULT reader=1 engine=sqlite reads=31843 last=400 monotonic=true
```

Under two-process contention SQLite costs ~4.7ms per durable event against
redb's ~3.4ms — and completes work redb cannot start. A reader alongside a
live writer saw 31,843 consistent, monotonic snapshots without blocking it.

One flaw worth recording, because it is a trap for the implementation:
`busy_timeout` must be armed **before** `journal_mode=WAL`. Switching
journal mode takes a brief exclusive lock, so two processes opening at the
same instant collide there — before WAL is in effect. The first run of this
spike crashed exactly that way.

### Correction, 2026-08-16: ordering is necessary and not sufficient

This spike concluded "the fix is ordering, not retry logic". That is wrong,
and the implementation inherited it. Measured against the switch while
another connection holds the database:

| the other connection | switching to WAL |
| --- | --- |
| holds a **write** lock, database still in its default journal mode | fails **immediately** — `busy_timeout` is not consulted |
| holds a **read** lock, database still in its default journal mode | waits the whole timeout, then fails |
| holds any lock, database **already** in WAL | succeeds; the switch is a no-op |
| merely connected, no lock | succeeds |

So the trap is narrower and worse than recorded: it is only the window
between a store file being created and its switch to WAL, and inside that
window an armed timeout buys nothing. The fix is ordering **and** a bounded
retry (`enter_wal`), which waits for the holder — another agent host doing
what this engine exists to allow — and still reports rather than hanging.

The row that matters for tests: once a store is in WAL, every later open
takes the no-op path. A test that creates the store before racing two
processes therefore cannot see this defect, which is why kmp#34 stayed
invisible while the two-writer test passed.

## Why this did not become a PR to redb

Asked before choosing an engine: `cberner/redb` already has three open PRs
(#1375, #1376, #1377, 2026-08-14, ~2,000 lines) adding a
`MultiProcessDatabase` behind `experimental-multiprocess`. None merged. #1375
says of itself that "this type has no advantage over `Database` yet" and
names the remaining work — "readers attaching to a database another process
is writing, and handing the writer role between processes" — as a later step.
The design notes record why it is the hard part: readers can only be admitted
once they can publish what they are reading, and on Windows a shared lock
denies writes to the holder, so all coordination has to move into separate
lock files. That is SQLite's `-shm` WAL-index, rebuilt.

## ADR-009 criteria, re-measured here

The original numbers are from different hardware and a different kernel, so
the trade-off is priced on the machine deciding today rather than quoted.

```
corpus: 20000 events, 1024B bodies, batches of 1000, 200000 point reads

| engine | batched write (ev/s) | reopen (ms) | point reads (r/s) | size (MB) |
| --- | --- | --- | --- | --- |
| redb 4.1.0 | 88427 | 1.93 | 4028475 | 64.3 |
| SQLite (WAL, synchronous=FULL) | 61594 | 0.18 | 781421 | 26.1 |
```

Caveats, matching the original spike's honesty: single run per engine, warm
page cache, apparent file sizes, one writer thread per process. The corpus
is 20k events rather than ADR-009's 102k, so absolute figures are not
comparable across the two documents — the **ratios** are what carry.

Adjacency scans are not re-measured here. ADR-009 found them nearly tied
(213k vs 165k scan/s, 1.3×) and that is the query shape the port surface
actually uses; point reads, where the gap is ~5×, are the shape it uses
least.
