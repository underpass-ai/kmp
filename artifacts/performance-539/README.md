# ChronoLoom selected-reference batching — #539

This increment implements the ChronoLoom follow-up requested on 2026-09-13 in
issue #539. Framing previously fetched an Inspect response for each uncached ref,
including graph/proof/body work that framing discarded. HTTP `/api/nodes` also
ran serial Inspect calls. The new shared ReadNodes operation resolves only scoped
headers and coordinate metadata from one pinned SQLite snapshot.

Up to 64 refs share a request. Longer foci use up to 4096 refs, chunked by 64,
under one 32768-edge budget. Every following batch repeats the first snapshot
identity; writes or a reopened observer reject the entire focus before its window
changes. No transaction is held between requests. Missing/foreign refs remain
separate from budget omissions and incomplete coordinate sets. There is no
sequential fallback for backends that cannot certify consistency.

This is metadata for framing, not complete canonical proof. Existing Inspect,
Trace, bounded body admission and named proof expansion remain the audit paths.
Their previously delivered batching/snapshot work is not counted as a new gain.
Header and coordinate bytes are not bounded by a canonical-body ceiling; no fixed
process-memory guarantee is claimed. A later scene projection is a separate read.
A fully cached focus formerly avoided metadata I/O; it now needs a fresh batch to
avoid combining coordinates from different revisions. That additional read is a
consistency cost; the timings below cover the affected uncached-reference path.

## Reproduction

Run from the repository root with the normal shared Cargo configuration:

```sh
cargo build --locked -p kmp-viewer --example node_batch_benchmark
python3 artifacts/performance-539/measure.py artifacts/performance-539-reproduction
```

Choose a new output directory for every run; the script refuses to overwrite
existing evidence and removes only its own temporary stores.

The runner compares complete node metadata and raw coordinates to individual
Inspect on four quiescent synthetic stores before recording timings. The code
uses projection mutations with fixed timestamps and no personal memory. The
large-body fixture deliberately separates short headers from long Details records;
canonical-ingest headers can contain additional text in their properties, so this
fixture does not predict their byte ratio. Unit/integration tests separately
cover exact clocks and origin metadata, missing/foreign/duplicate refs, shared
budgets, concurrent writes, changed bodies, reopened observers, real gRPC parity,
real HTTP parity and late/partial/changed-revision browser responses.

Eight independent benchmark processes run in a reversed second-round order,
20 warm frames per fixture in each process. `first_read_us` follows fixture
creation and is not cold OS-cache latency. Five reopened engines per process
report opening and reading separately; the OS cache is uncontrolled. For HTTP,
viewer bootstrap/authentication is excluded from the read time. Each request uses
a fresh loopback TCP connection, parses JSON and counts HTTP response bytes.
Native mode includes the application read, mapping and JSON serialization;
its `transport_trips` field counts logical application calls, not network packets.
All runs use the same dev binary. Environment, raw samples, first reads, reopened
reads, command time and whole-process peak RSS are retained alongside this file.
RSS includes fixture construction and all fixtures; it is not allocation-per-read.
This task runs no compilation or tests concurrently with the timed processes. Normal
desktop background activity remains possible. These are local informational
measurements, without release, production, WAN or OS-cold-cache claims.

## Results

### Native application + serialization

| Fixture | p50 ms before → after | p95 ms before → after | Response bytes before → after | Calls before → after |
|---|---:|---:|---:|---:|
| small | 0.797 → 0.341 | 1.937 → 0.376 | 1,817 → 1,084 | 1 → 1 |
| medium | 18.984 → 2.028 | 28.458 → 2.779 | 60,616 → 11,885 | 8 → 1 |
| high_degree | 399.422 → 81.919 | 475.480 → 83.584 | 2,167,360 → 600,895 | 64 → 1 |
| large_body | 191.562 → 2.151 | 203.393 → 2.987 | 2,133,960 → 11,885 | 8 → 1 |

### Actual loopback HTTP round trips

| Fixture | p50 ms before → after | p95 ms before → after | Response bytes before → after | Calls before → after |
|---|---:|---:|---:|---:|
| small | 0.966 → 0.638 | 1.404 → 0.884 | 2,121 → 1,388 | 1 → 1 |
| medium | 15.912 → 2.992 | 18.803 → 4.454 | 63,048 → 12,190 | 8 → 1 |
| high_degree | 431.179 → 116.785 | 532.621 → 118.414 | 2,186,880 → 601,201 | 64 → 1 |
| large_body | 218.751 → 2.562 | 282.710 → 3.169 | 2,136,408 → 12,190 | 8 → 1 |

Each p50/p95 combines 40 warm observations. No fixture is omitted, including the single-ref case. Reopened-engine timings and complete first-read values are retained in each raw JSON; whole-process RSS is summarized in `summary.json`. These measurements do not isolate per-phase store operations or allocations.

Peak process RSS increased. Native runs measured 19,940–20,628 KiB before and
26,396–26,896 KiB after; HTTP runs measured 23,472–24,880 KiB before and
34,780–44,848 KiB after. Batching holds a whole response at once. This observation
is not a causal allocation profile: RSS covers fixture creation, all four shapes,
allocator retention and reopened engines. The result supports fewer calls and
lower latency on these fixtures, not a memory reduction.


## Verification and issue scope

The workspace quality gate covers contract lint/breaking parity, architecture,
registry, browser tests, formatting, workspace Clippy, rustdoc, workspace tests and
MCP build. The CLI selection test needs a temporary directory without a Git
ancestor: the successful full run uses a dedicated sibling scratch directory
inside `~/Documents/ai`, removed afterwards. The earlier in-repo-scratch failure is
retained locally; its test and product code were not changed.

The full gate passed on the implementation at 0.18.0 (2,333 Rust test executions,
including the repeated contract tests; four ignored; 96 browser tests). The branch
was then rebased on the version-only 0.18.1 preparation. Every changed Rust,
JavaScript and proto file still matches `source-hashes.json`; the guide was
regenerated with the 0.18.1 binary. Contract, tool-surface, gRPC and HTTP checks
passed again after the rebase. See `validation.json`.

The model-facing catalogue fixture is byte-identical to the base. Its hash is in
`catalogue.json`; fixture bytes are not a host-visible token count. The new tool
is app-only. The operation performs no model generation, embedding, scoring or
canonical-body read. Domain reader tests assert deduplicated node reads and exact
coordinate edge counts; they fail if body reads are attempted.

This report does not claim to close #539's broader proof-group profiling matrix:
per-phase storage/encoding/ranking/proof timings and whole-proof memory/cold-cache
comparisons beyond the previously delivered evidence remain separate acceptance
work. The new measurements concern the reported serial ChronoLoom framing chain.
