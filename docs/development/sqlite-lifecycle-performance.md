# SQLite lifecycle evidence (#772)

KMP opens the kernel database with `PRAGMA quick_check(1)`, enables WAL, and
keeps kernel durability at `synchronous=FULL`. The open path also validates the
schema and rejects unsafe input before the store is used. Quality telemetry is
a separate `telemetry/quality.sqlite3` database: its ordinary batches use
`NORMAL`, every configured durable interval switches to `FULL`, and an explicit
tail flush runs `BEGIN IMMEDIATE; COMMIT; PRAGMA wal_checkpoint(FULL)` before
returning. The periodic quality policy is existing behavior and is not claimed
as a new optimization here.

The reproducible control in `scripts/performance/sqlite_lifecycle.py` uses a
temporary Rust runner built from the workspace lockfile. It prepares bounded
synthetic stores through `EmbeddedKernelStore`, measures one separate first
open and repeated opens, and records native write, read, and pinned snapshot
operations. A separate raw `rusqlite` diagnostic observes PRAGMA values, WAL
size, and passive/full checkpoint frame counts. The raw diagnostics do not
stand in for native application operations and do not change production
settings.

The control includes an empty store, 128 events with 32 KiB payloads, and 512
events with 32 KiB payloads. It uses at least 21 warm samples per shape. The
result includes source, binary, toolchain, profile, host, hardware, input
counts, WAL timelines, and checkpoint contention under a pinned reader. The
recorded results are in `artifacts/performance-772/lifecycle.json`.

There is no separate `snapshot.db` in the embedded layout. Read snapshots are
transaction-bound connections to `store/kernel.sqlite3`; the control therefore
reports the kernel database plus the separate quality telemetry database and
measures snapshot acquisition against the real kernel connection.

The safety coverage retains the existing crash-recovery and two-client tests:
an interrupted SQLite writer loses at most its in-flight transaction and
reopens cleanly; two writers preserve all acknowledged events; and quality
telemetry readers and writers coexist. The focused `sqlite_lifecycle` tests
add refusal of corrupt kernel input and prove that a native pinned reader sees
its original committed snapshot while a later writer commit is visible to the
current store.

The evidence is used to decide whether any bounded checkpoint or cache policy
has a measured benefit. It does not justify changing durability PRAGMAs,
removing startup integrity checks, or checkpointing every write without a
measured reason and recovery proof.

The companion `sqlite_clients.py` control measures one and two independent
client processes against each database, with per-operation latency, per-client
VmHWM, WAL growth and exact acknowledgement counts checked after reopening.
Its 40-operation samples, process barriers, native quality writer cadence and
measurement limits are documented alongside `artifacts/performance-772/clients.json`.
