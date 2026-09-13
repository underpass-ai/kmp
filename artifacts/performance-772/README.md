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

| shape | seed data | first open, one sample (ms) | warm open p50/p95 (ms) | write p50/p95 (ms) | read p50/p95 (ms) | snapshot p50/p95 (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| empty | 0 events | 0.836 | 0.419 / 0.464 | 0.362 / 0.440 | 0.018 / 0.020 | 0.051 / 0.054 |
| medium | 128 × 32 KiB, 4.14 MiB DB | 3.940 | 1.574 / 3.007 | 1.756 / 3.158 | 0.015 / 0.232 | 0.040 / 0.044 |
| large | 512 × 32 KiB, 16.33 MiB DB | 6.147 | 4.240 / 4.526 | 1.330 / 6.895 | 0.265 / 0.485 | 0.120 / 0.135 |

The cold value is one separately timed first open per shape in a fresh process,
after a separate seed process has closed. OS caches were not flushed. The DB
sizes above are measured after the workload; payload totals are 4/16 MiB. Warm values use
21 repeated opens or operations and nearest-rank percentiles. These are
informational measurements, not CI thresholds.

The pinned-reader control committed 21 native writes while a raw read
transaction held its snapshot. A passive checkpoint reported `busy=0` and
checkpointed 45 of 91 frames while the reader was pinned; after releasing the
reader, a full checkpoint reported 91 of 91 frames. In the empty-store case, the reader
started with 21 rows and the current store reached 42 rows. The separate
native snapshot regression verifies retention of the earlier snapshot. This demonstrates checkpoint
progress after reader release in this bounded run. It is not a long-duration
starvation test. Medium and large cases checkpointed 46/92 frames while pinned
and 92/92 after release; their counts advanced 149→170 and 533→554.

Both PRAGMA inventories use fresh diagnostic connections. They observe WAL
and default connection-local values: `synchronous=2`, `cache_size=-2000`,
`temp_store=0` and 1000-page autocheckpoint. Kernel FULL durability is established
by the production connection setup, not by these separate connections. A
negative cache size specifies KiB rather than pages. See the
[SQLite cache-size contract](https://sqlite.org/pragma.html#pragma_cache_size).
The measured JSON retains the historical key `cache_size_pages`; interpret
its -2000 value as a roughly 2000-KiB cache suggestion. The current runner
corrects the key/unit and connection-scope metadata. Its output metadata differs
from the measured runner hash above; the native timing path is unchanged. There is no separate `snapshot.db`:
native snapshots are transaction-bound read connections to the kernel file.
The quality inventory is
explicitly from a fresh diagnostic connection; SQLite's synchronous setting is
connection-local, so it reports the default FULL value. The production quality
writer's source and existing tests establish its connection-local NORMAL and
periodic FULL schedule; the diagnostic connection must not be read as a claim
that the writer uses FULL for ordinary batches.

The control uses native production adapters for opens, writes, reads, and
snapshots. Raw `rusqlite` is used for the pinned reader and explicit diagnostic
PRAGMA/checkpoint queries; event fixtures are seeded through the native adapter. Existing focused tests cover interrupted
SQLite writes and exact reopen recovery, two concurrent kernel writers,
quality telemetry persistence and multi-process coexistence. The added
`sqlite_lifecycle` tests cover corrupt input refusal and a pinned native reader
surviving a later writer commit.

Reproduce with:

```text
python3 scripts/performance/sqlite_lifecycle.py \
  --scratch tmp/performance-772-lifecycle \
  --output artifacts/performance-772/lifecycle.json \
  --warm-samples 21
```

All synthetic files are disposable scratch below `tmp/`; no active user store
was opened, checkpointed, tuned, or deleted.

## One and two independent client processes

`clients.json` supplements the single-client lifecycle with two processes
sharing each database. Each case is prepared in a separate seed process with
128 events (kernel: 4-KiB payloads) or 128 quality observations. Each client
opens the production adapter, waits at a common start barrier, and performs 40
acknowledged writes and 40 reads. Kernel clients use distinct logical roots in
one database, exercising the normal SQLite writer serialization. Quality
clients use independent reader connections, one observation per batch, the
existing FULL cadence every 16 batches, and a durable tail flush.

| Database / clients | Write p50 / p95 ms | Read p50 / p95 ms | Per-client VmHWM KiB | Maximum observed WAL bytes |
| --- | ---: | ---: | --- | ---: |
| kernel / 1 | 1.011 / 1.560 | 0.151 / 0.400 | 6272 | 379,072 |
| kernel / 2 | 1.657 / 3.693 | 0.220 / 0.382 | 5812, 5812 | 758,112 |
| quality / 1 | 0.038 / 0.568 | 0.162 / 0.173 | 5668 | 510,912 |
| quality / 2 | 0.130 / 0.712 | 0.363 / 0.386 | 5672, 5652 | 1,030,032 |

The two-client percentiles pool 80 operations; per-process arrays remain in the
artifact. Pacing sleeps of 1 ms, barriers, fixture construction, WAL reads and
RSS observation are outside operation timings. First-open times and quality
tail-flush times are recorded separately per client. OS caches are not flushed.
VmHWM is each process's lifetime peak, not simultaneous aggregate memory or
live heap. The run is a bounded contention control, not a sustained-load or
starvation forecast; WAL growth during acknowledged writes is expected.

Every case reopens in a new process and verifies all per-client acknowledgements:
168 stored events/observations for one client, 208 for two. The two-client
kernel write tail rose to 3.69 ms versus 1.56 ms; quality write p95 was 0.71 ms
versus 0.57 ms. The observed costs do not justify changing the existing
checkpoint or durability policy. Existing interruption and pinned-reader tests
remain the recovery/coexistence controls.

Reproduce after building separately from the quiet measurement:

```sh
python3 scripts/performance/sqlite_clients.py --build-only \
  --scratch tmp/sqlite-clients-build --output tmp/unused.json
python3 scripts/performance/sqlite_clients.py \
  --runner-binary target/debug/sqlite-clients-runner \
  --scratch tmp/sqlite-clients-run --output artifacts/performance-772/clients-new.json
```

The current Rust/Python runner hashes and measured source commit are in
`clients.json`; production adapter source is unchanged from the earlier
lifecycle measurement. The 32-sample smoke ran during development and is
excluded; its semantic assertions passed before the final 40-sample run.
