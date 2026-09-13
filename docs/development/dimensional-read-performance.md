# Dimensional lookup performance (#775)

`ALL_ABOUTS` resolves candidate anchors through `MemoryAboutIndexReader` before
the application applies whole-entry label predicates. The old SQLite adapter
scanned anchors, loaded each anchor, decoded its entire outgoing adjacency, and
then loaded dimension records until one matched. Unrelated relation explanations
and node prose crossed the storage boundary just to discover an about.

The adapter now pages an ordered join of the existing anchor and typed adjacency
indexes. Each page returns at most 256 links with only `node_id`, `node_kind` and
`properties.dimension_kind` from their endpoints. SQL parameters carry field
paths, relation kind and continuation keys. No relation explanation is loaded.
The adapter retains the original union of full reference, canonical decoded
value and stored kind matches, and returns deduplicated, sorted anchor keys.

Every page uses the operation's existing read transaction. The application still
owns scope admission, whole-entry selectors, current-about priority, temporal
visibility and deterministic bundle merging. There is no new persisted index,
cache, schema version, public contract, invalidation path or write amplification.
Existing format-3 stores and peer writers use the same tables and write code.

## Controlled comparison

Baseline: `c95a92d11d3b55c2c847c1d653dcfdd687132c58`. The same native example was
built on both trees with the dev profile and the workspace Cargo configuration,
Rust 1.97.1, Linux aarch64. Binary, example and runner hashes, CPU description,
raw samples and process resource reports are in the accompanying
[`performance-775` evidence](../../artifacts/performance-775/paired-final).

Each shape uses two before and two after processes in ABBA order, without
overlapping builds or benchmark processes. Every process seeds synthetic nodes,
then checks the exact expected ordered abouts before recording each sample.
There are 30 warm samples plus one first lookup per process (60 warm samples per
side and shape). The first lookup is **not** an OS-cold-cache measurement: the
same process just wrote the fixture. Snapshot acquisition is recorded separately.

| Shape | Anchors | Dimensions/anchor | Other edges/anchor | Prose bytes/record | Warm p50 ms before/after | Warm p95 ms before/after |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Small | 4 | 4 | 2 | 128 | 0.726 / 0.596 | 1.346 / 0.842 |
| Medium | 256 | 4 | 4 | 256 | 24.613 / 15.849 | 25.265 / 16.438 |
| High degree | 256 | 4 | 64 | 2,048 | 251.092 / 42.399 | 254.556 / 43.346 |
| Large body | 64 | 4 | 8 | 32,768 | 147.894 / 65.721 | 149.005 / 66.159 |

First-lookup microseconds, before / after: small `[881,834] / [835,826]`, medium
`[24362,24486] / [16114,16133]`, high degree `[250719,254681] / [43351,43857]`,
large body `[149055,148921] / [67088,67490]`. Small-case timings show greater relative process variation.

For the high-degree fixture, the old loop executes 1,441 logical SQL reads
(one anchor scan, 256 anchor reads, 256 adjacency scans and 928 dimension reads).
The new path executes five joined pages, including the empty terminal page.
These counts follow from the fixture and executed loop structure; they are not
physical disk I/O measurements. The storage test measures under 512 returned
header bytes for two links despite multi-megabyte discarded endpoint prose and
relation values. SQLite still parses the selected nodes' JSON internally; the
optimization does not claim zero underlying page reads or constant CPU cost for
arbitrarily large JSON values. Page size bounds row count, not arbitrary strings
inside a header or the complete output's number of abouts.

## Concurrent readers and writes

Each process also runs four readers, each issuing 12 independently pinned reads,
while a separately opened writer commits 12 whole-set dimension-type changes.
Every result must equal the entire selected set or the empty set. A mixed state
fails the run. Times include snapshot acquisition. Two processes give 96 read
and 24 write samples per side and shape.

| Shape | Read p50 ms before/after | Read p95 ms before/after | Write p95 ms before/after |
| --- | ---: | ---: | ---: |
| Small | 1.053 / 1.244 | 2.116 / 2.604 | 4.816 / 5.073 |
| Medium | 27.631 / 25.645 | 61.319 / 48.623 | 8.949 / 11.884 |
| High degree | 274.455 / 128.819 | 305.925 / 168.761 | 7.045 / 12.628 |
| Large body | 153.602 / 73.772 | 158.941 / 89.360 | 14.313 / 14.013 |

High-degree throughput rises from 14.1–14.3 to 29.3–29.4 reads/s; large-body
throughput rises from 25.8–25.9 to 52.5–53.4 reads/s. Small concurrent reads and
the small, medium and high-degree write tails regress. The implementation preserves FULL synchronization
and introduces no extra writes; faster reads do not establish an improvement in
write latency under contention. These regressions remain part of the result.

Process peak RSS includes seeding, all lookup samples, the scheduling experiment
and the mixed workload, rather than attributing memory to an individual query:

| Shape | Peak RSS KiB before | Peak RSS KiB after | Process CPU seconds before / after |
| --- | ---: | ---: | ---: |
| Small | 6,272–6,372 | 6,692–6,916 | 0.08–0.11 / 0.08–0.11 |
| Medium | 21,344–24,956 | 20,000–25,200 | 2.49–2.60 / 2.02–2.07 |
| High degree | 116,940–117,036 | 115,624–117,228 | 25.66–25.76 / 11.49–11.96 |
| Large body | 70,536–71,420 | 67,464–69,036 | 15.90–16.16 / 9.13–9.19 |

## Scheduling decision

The independent scheduling control compares serial reads with batches of four
spawned reads over the **same** pinned store, for up to 32 roots. Six rounds per
process reverse the order on alternate rounds. It checks root order after joining.
The medium case improves (candidate median 18.138 to 16.000 ms), but high degree
regresses (128.399 to 133.816 ms). The large-body change is small (191.838 to 188.337 ms).
Small-case results vary between processes and implementations even though their
neighborhood reader is identical. This microcontrol does not measure complete
application journeys or justify enabling concurrency across backends.

`SqliteSnapshot` serializes access to its one pinned connection. Separate root
transactions would lose the operation's snapshot contract. Consequently this
change keeps root scheduling and cancellation/error propagation intact. The
stdio loop also still reads one request and awaits its response; moving just its
input read to a dedicated thread would not make that dispatcher concurrent.
Concurrent dispatch would need independent evidence about request ordering,
write/read interaction, cancellation and backpressure. No stdio throughput claim
or scheduling change is included here.

## Reproduction and correctness

Copy `crates/kmp-adapter-embedded/examples/dimensional_lookup_benchmark.rs` onto
the baseline checkout, then build that example in both trees:

```bash
cargo build --locked -p kmp-adapter-embedded --example dimensional_lookup_benchmark
python3 scripts/performance/dimensional_lookup.py \
  /absolute/baseline/target/debug/examples/dimensional_lookup_benchmark \
  target/debug/examples/dimensional_lookup_benchmark \
  artifacts/performance-775/reproduction 30
```

The example creates disposable stores under `tmp/performance-775` and removes
them on normal completion. Run benchmarks serially, with the same compiler,
profile and example source. The runner requires Linux `/usr/bin/time` and
`/proc/cpuinfo`. It uses no model, token encoder or network service; these are
native storage measurements, not MCP context-token or complete-journey claims.

Focused tests cover full refs, bare values, kind selection, punctuation/Unicode,
empty input, duplicates and ordering, more than two header pages under one
anchor, shared labels, placeholders, non-dimension targets, relation removal,
source/target type changes, peer writes, reopen and old snapshots. Engine tests
cover missing endpoints, exclusive keyset continuation, field projection,
zero-sized pages, incompatible key shapes, parameter quoting and corrupt JSON.
A 10,000-link fixture also verifies that a late page seeks inside the typed
adjacency index (114 SQLite VM steps for its final two rows), including an empty
target key on a later root, instead of rescanning earlier links.
The existing whole-entry, temporal, scope and transport tests remain the semantic
backstop. Performance results are informational; no latency CI gate is added.

A separate native MCP replay compares copies of the same four-about store:
**28 complete results are identical** across Wake, Ask and Goto, including
positive/negative whole-entry predicates, explicit about selection and all five
clocks (56 read tool calls). No fields are removed or normalized for comparison.
The [native replay evidence](../../artifacts/performance-775/native-parity)
contains its summary and losslessly compressed original JSONL traces; its timings
are not interpreted because correctness replay overlapped other validation.
Reproduce it with:

```bash
python3 scripts/performance/dimensional_read_parity.py \
  /absolute/baseline/target/debug/kmp-mcp target/debug/kmp-mcp \
  artifacts/performance-775/native-reproduction tmp/performance-775-native
```
