# E0 storage spike — redb vs fjall vs SQLite

Source of the numbers recorded in
[ADR-009](../../ADR-009-embedded-storage-engine.md). Run on 2026-07-21
(AMD Ryzen Threadripper PRO 5955WX, NVMe SSD, btrfs, Linux 7.0,
rustc 1.95.0).

[`bench.rs`](bench.rs) is a standalone binary (kept out of the workspace on
purpose — it is evidence, not product code). To reproduce:

```bash
cargo new spike-bench && cd spike-bench
cp <this dir>/bench.rs src/main.rs
cargo add redb@4.1.0 fjall@3.1.8
cargo add rusqlite@0.40.1 --features bundled
cargo run --release -- ./data
```

Result of the recorded run:

```
corpus: 102000 events (2000 per-event durable + 100000 batched x1000),
        20000 nodes, ~1KB event bodies

| engine | per-event durable (ev/s) | batched (ev/s) | reopen (ms) | point reads (r/s) | adjacency scans (scan/s) | edges seen | size (MB) |
| ------ | --- | --- | --- | --- | --- | --- | --- |
| redb   | 265 | 29088 | 2.9 | 846775 | 212957 | 10214 | 249.3 |
| fjall  | 294 | 32135 | 1329.9 | 1044037 | 140335 | 10214 | 263.1 |
| sqlite | 293 | 33038 | 0.3 | 176417 | 165140 | 10214 | 151.6 |
```

Stripped release binary sizes (linux x86_64 glibc), one minimal binary per
engine against a 344KB `println!` baseline: redb 1.05MB (+0.70MB), fjall
1.59MB (+1.25MB), SQLite bundled 2.48MB (+2.14MB).

Caveats: single run per engine; read phases hit a warm page cache; sizes are
apparent file sizes on btrfs; fjall reopen might improve with pre-close
compaction (not tuned — see ADR-009 for why that matters anyway).
