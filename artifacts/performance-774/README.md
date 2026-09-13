# Store-lease accumulation evidence for #774

The reported 9,289 empty lock files were an unverified hypothesis. A
read-only observation during this run found 10,045 `*.lock` files in the
machine's global lease directory, with zero payload bytes and 1,175,552 bytes
of directory metadata. That directory belongs to the running development
environment and was not changed; the observation is context, not a cleanup
target. See `global-readonly-observation.json`.

The lease implementation derives one SHA-256 pathname from the canonical store
path, then opens and locks that exact file. Startup's remembered-store index
and store catalog do not enumerate `store-leases`. Unlinking a lease pathname
would be unsafe: an older KMP host could still hold the original inode while a
new host opens a recreated inode. PID liveness cannot prove inode ownership or
handle PID reuse. The implementation therefore retains empty lease files and
keeps mutual exclusion stable across versions.

The synthetic control uses a temporary Rust runner linked to the workspace's
locked `kmp-mcp` dependencies. It creates 0, 1,000, and 10,000 unrelated empty
lock files under `tmp/`, then measures one first-use claim separately from 23
repeated claims for one canonical store. The timed claim results were:

| unrelated files | files after setup | directory metadata | cold (ms) | warm p50 (ms) | warm p95 (ms) |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 4,096 B | 0.038928 | 0.012640 | 0.014912 |
| 1,000 | 1,001 | 36,864 B | 0.035231 | 0.024288 | 0.029856 |
| 10,000 | 10,001 | 335,872 B | 0.044240 | 0.012608 | 0.013952 |

The audited run used 23 warm samples per cardinality with runner source based
on `531761ead49d50f60d0b259f2a127ba1d3115dee` plus the raw-sample/percentile
fix in this PR. Its runner SHA is `02a194e4f9d8554a6ba197eaddb66f35a32257fd065ce4256040a1f2453f2ebd`;
the locked runner binary SHA remains
`0dec7b2c57971ad97fbe1d581dc9fc9168abc94f9833484f5ae2802659bd59a7`, built from
`0f4b73023378388a42458c602d8909e3d1c1dc3c` (lease runtime unchanged).

`cardinality.json` retains all raw nanosecond samples and computes nearest-rank
percentiles. `cardinality-initial-summary.json` preserves the earlier run,
which omitted raw samples and used a floor-index percentile; it is excluded
from the table above. The rerun changes the measurement report, not lease
behavior.

These are direct `StoreSessionLease::acquire` timings, with directory setup
outside the timed region. They establish the per-store lookup cost and the
absence of a cardinality-dependent scan; they are not a full MCP process
startup benchmark and are not a CI threshold. `StoreSessionLease` is the
shared session lease used by removal protection; the kernel's separate
single-writer store lock remains in force. The complete machine, toolchain,
source, binary, and hardware hashes are in `cardinality.json`.

`crates/kmp-mcp/tests/store_lease_safety.rs` covers current shared owners,
live pre-lease owners holding the SQLite file, abrupt process termination while
holding a lease, canonical alias identity, stable inode reuse on Unix, and
distinct store identities. The focused test command passed all five tests.

Reproduction:

```text
cargo test --locked -p kmp-mcp --test store_lease_safety
python3 scripts/performance/store_lease_cardinality.py \
  --scratch tmp/performance-774-cardinality-run \
  --output artifacts/performance-774/cardinality.json \
  --warm-samples 23
```

The runner's build-only mode is useful when a quiet measurement slot is
required. All synthetic stores and lock files are disposable scratch beneath
`tmp/`; no user lease files are removed or rewritten.
