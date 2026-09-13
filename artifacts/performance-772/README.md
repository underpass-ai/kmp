# SQLite lifecycle evidence for #772

The production code keeps the kernel database on WAL with `synchronous=FULL`,
`busy_timeout=10s`, `wal_autocheckpoint=1000`, and `quick_check(1)` at every
validated open. Quality telemetry is a separate database. Its writer keeps
ordinary batches at `NORMAL`, periodically uses `FULL`, and exposes an
explicit durable tail flush with a full checkpoint. Those are existing
settings; this work does not claim switching telemetry to `NORMAL` as a new
optimization.

`lifecycle.json` was produced by the workspace-locked runner on commit
`7af38d73a6bd2dcf5ea9e83e394838af8d612a6f`, with 21 warm samples per shape.
The runner SHA is `33845579c116e707f083ccab16590e6da259a9769939c9cbb77354bcc21743a0`
and the binary SHA is
`d3c6050b142289a0b88fff9662b1fe926bbd840e796a184d383dabce8992b8b5`.
Toolchain, profile, hardware, source hashes, and all raw samples are in the
JSON record.

The direct native lifecycle results were:

| shape | seed data | cold open p50/p95 (ms) | warm open p50/p95 (ms) | write p50/p95 (ms) | read p50/p95 (ms) | snapshot p50/p95 (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| empty | 0 events | 0.836 / 0.836 | 0.419 / 0.464 | 0.362 / 0.440 | 0.018 / 0.020 | 0.051 / 0.054 |
| medium | 128 × 32 KiB, 4.3 MiB | 3.940 / 3.940 | 1.574 / 3.007 | 1.756 / 3.158 | 0.015 / 0.232 | 0.040 / 0.044 |
| large | 512 × 32 KiB, 17.1 MiB | 6.147 / 6.147 | 4.240 / 4.526 | 1.330 / 6.895 | 0.265 / 0.485 | 0.120 / 0.135 |

The cold value is one separately timed first open per shape. Warm values use
21 repeated opens or operations and nearest-rank percentiles. These are
informational measurements, not CI thresholds.

The pinned-reader control committed 21 native writes while a raw read
transaction held its snapshot. A passive checkpoint reported `busy=0` and
checkpointed 45 of 91 frames while the reader was pinned; after releasing the
reader, a full checkpoint reported 91 of 91 frames. The reader retained 21
rows while the current store reached 42 rows. This demonstrates checkpoint
progress after reader release and does not indicate starvation requiring a new
policy.

The kernel PRAGMA inventory confirms WAL, `synchronous=2` (FULL),
`cache_size=-2000` pages, `temp_store=0` (SQLite default), and the default
1000-page autocheckpoint for every shape. There is no separate `snapshot.db`:
native snapshots are transaction-bound read connections to the kernel file.
The quality inventory is
explicitly from a fresh diagnostic connection; SQLite's synchronous setting is
connection-local, so it reports the default FULL value. The production quality
writer's source and existing tests establish its connection-local NORMAL and
periodic FULL schedule; the diagnostic connection must not be read as a claim
that the writer uses FULL for ordinary batches.

The control uses native production adapters for opens, writes, reads, and
snapshots. Raw `rusqlite` is used only for bounded fixture setup and explicit
diagnostic PRAGMA/checkpoint queries. Existing focused tests cover interrupted
SQLite writes and exact reopen recovery, two concurrent kernel writers,
quality telemetry persistence and multi-process coexistence. The added
`sqlite_lifecycle` tests cover corrupt input refusal and a pinned native reader
surviving a later writer commit.

Reproduce with:

```text
python3 scripts/performance/sqlite_lifecycle.py \
  --runner-binary target/debug/sqlite-lifecycle-runner \
  --scratch tmp/performance-772-lifecycle \
  --output artifacts/performance-772/lifecycle.json \
  --warm-samples 21
```

All synthetic files are disposable scratch below `tmp/`; no active user store
was opened, checkpointed, tuned, or deleted.
