# Structured temporal reads

Temporal navigation and ChronoLoom projections consume structured memory rather
than a discarded rendered prompt. The graph and body readers share the
operation snapshot configured by `QueryApplicationService::read_snapshot`.
Embedded reads pin that snapshot before resolving abouts; provider errors do
not fall back to live reads. Separate adapters without a snapshot provider
retain their existing best-effort consistency.

## Selecting bodies for a temporal page (#770)

Goto, Near, Rewind and Forward now read the scoped graph catalogue without
canonical detail bodies. The existing domain traversal selects entries using
the same clocks, ties, intervals, entry predicates, limits and continuation
positions. Temporal positions carry identities and coordinates; sorting and
partitioning no longer copy each entry's text. The node lookup borrows the
catalogue, and only returned entries acquire owned text.

After temporal membership admission and dimension filtering,
`TemporalProofPlan` selects the requested page's proof identities. Optional
dependency groups use the existing two-hop, eight-member policy. Their clock
boundary is the interval's exclusive end or Goto's stricter inclusive cursor;
antecedents may precede the interval start. The application and protobuf
projection share this domain policy, including unavailable and omitted counts.
The projection reads extra entry text only for selected group members.

Canonical details are then fetched in one ordered batch through the existing
`NodeDetailReader` port. Only selected entries, dependency members and their
supporting evidence are requested. Raw refs require the selected entry bodies;
entries and relations alone require none. Missing bodies stay missing. A body
can still be large, and a selected entry can still have many declared sources:
this avoids unrelated body loading, not an allocation or byte cap. Sources
whose receipt or support relation is outside the temporal selection remain
subject to the unchanged final evidence-admission checks.

`TemporalMemoryResult::source_bundle` carries the scoped admission catalogue
and canonical nodes, support relations and bodies for the selected proof. It
is not a full graph export: unused source payloads are absent. ChronoLoom keeps
its full structured read because its projection consumes a different catalogue.

## Indexed catalogues and recall admission

Temporal navigation builds an immutable `TemporalMemoryIndex` over admission
headers, ordered on the requested clock. Clock and ordinary interval selection
seek that ordered index. Selection borrows positions; it no longer clones every
coordinate or sorts each candidate page. Exact totals, whole-entry predicates,
validity intervals and ref continuations retain the same domain policy. Some
of these operations still scan eligible coordinates to compute complete totals.

The application retains at most one index, keyed by the complete graph-and-body
revision, ordered about roots, traversal depth and clock. Page, interval and
label predicates run against the full retained catalogue, so changing them
cannot reuse a previously filtered subset. Changing an index input replaces
the retained index. Admission allows at most 8,192 nodes, 32,768 relationships
and a conservative 64 MiB accounting allowance for strings and containers;
this is not a measured RSS limit. Oversized catalogues are read without being
retained. Active requests retain their own immutable references until completion.

`GraphNeighborhoodReader` exposes optional snapshot revision and admission-header
reads through backend-neutral ports. Embedded execution observes SQLite
[`data_version`](https://www.sqlite.org/pragma.html#pragma_data_version) on one dedicated connection that never writes. The observer's
incarnation distinguishes connections and stores. A revision is reusable only
when observations immediately before and after pinning the operation snapshot
match. A concurrent commit keeps the pinned read valid but disables reuse for
that operation. This observes peer and older-binary writes without a schema
migration, trigger or write-side counter. Adapters without a certified revision
perform ordinary uncached reads.

SQLite projects the named node header fields directly from the stored JSON.
Large source summaries and payload metadata do not cross the engine seam until
the application admits their identities. The storage primitive is internal;
there is no new user query language. Canonical node projections and support
explanations are then loaded for the selected proof in the same snapshot.
Support clocks are materialized even when only relations were requested, since
a late source can affect a path without its body being displayed. Relation
why/evidence remain complete in returned results. Detail bodies still use the
ordered batch introduced in the first delivery.

Wake and Ask now merge admission catalogues and apply whole-entry dimensional
selection before loading canonical nodes and details or rendering. Every
admitted candidate retains its full text for ranking. Temporal recall admission,
nearest-outside diagnostics and ranking remain downstream and unchanged; headers
never substitute for the canonical text they require. Query phase timings
account for graph/header reads, selection/assembly and admitted detail loading.

Index construction still requires an initial catalogue read and sort after an
invalidating write, clock change, eviction or process start. This implementation
uses a bounded in-memory index instead of adding a persisted temporal index;
it does not claim constant-cost cold discovery or physical-I/O savings.

## Validation

`temporal_body_selection` uses a recording adapter over a real SQLite store. A
one-entry page among 64 independent entries reads two bodies; raw-only reads
one; entries and relations alone read none. A dense dependency fixture keeps
its eight admitted members and 16 bodies while excluding a future declaration
before the member limit. Existing deterministic races cover peer writes,
multiple abouts, cancellation and snapshot refusal. Native MCP and gRPC tests
cover clocks, dependencies, lifecycle, fields and revision-bound continuation.

Run `scripts/performance/temporal_body_selection.py` with same-profile baseline
and candidate binaries, a new artifact directory and a disposable scratch
directory. It seeds synthetic stores once, copies them after closing the
writer, compares complete MCP results and proof pages, then measures 20
alternating before/after pairs after three warmups. Raw RPC traces include
latency, bytes, coarse process CPU ticks and process peak RSS. First-query
latency excludes process initialization; OS page caches are not flushed.
There is no model call or blocking performance threshold.

`temporal_index` checks complete cached/uncached results for all clocks,
changing selections, interval pages, peer writes, cross-about invalidation and
index replacement. A deterministic acquisition-race test refuses a reusable
revision if a commit occurs while the snapshot is pinned. A raw SQLite writer
checks invalidation without relying on a new projection hook. The header test
returns the same small JSON object with 1 KiB and 1 MiB discarded source fields;
this measures bytes crossing the engine seam, not SQLite's physical page reads.

`recall_body_selection` verifies that selectors can use another lane's labels
before fetching bodies and that admitted source text remains complete. Its
native runner compares Wake/Ask responses, including dated questions and
UNKNOWN, on copied synthetic stores before measuring alternating before/after
pairs. Both runners report first-query latency separately from warm p50/p95,
coarse process CPU, peak RSS and complete response bytes. These latency runs do
not instrument allocations or physical I/O. Raw traces belong in the workspace's
`artifacts/`; throwaway stores belong in `tmp/` and are removed after the run.

`scripts/performance/native_allocation_control.py` runs a separate Linux/glibc
control with `native_allocation_counter.c` preloaded into each native process.
It counts allocation requests and cumulative requested bytes across startup,
initialization and four identical queries, including the first catalogue build.
It does not measure retained heap, physical I/O or successful allocations; its
instrumented times must not be mixed with the paired latency measurements.
Every response is compared against the uninstrumented reference. The counter's
known-allocation control verifies three requests totaling 400 bytes and two
explicit frees. Compile it with the command in its source header and pass both
binaries, the shared library, paired artifact root, paired scratch root, a new
output directory and a new scratch directory to the Python runner.

## Recorded comparison (2026-09-13)

Native MCP, Linux/aarch64 on a 20-core ARM host, Rust 1.97.1, identical dev
profiles (own code `line-tables-only`, dependencies without debuginfo,
incremental disabled). Baseline is main `69defbb7`; candidate Rust code is
`b58853f9`. The binaries ran against copies of one closed synthetic seed store,
with one request in flight, 20 alternating before/after pairs and three warmups.
No Rust builds or workspace tests ran concurrently. This was a desktop host with ordinary
background activity; OS caches were not flushed. These are local informational
measurements, not production throughput or latency guarantees.

Temporal queries return three entries and all three canonical sources. Recall
uses a task lane and a selector carried by another lane, admitting one of every
16 entries. Full responses match for 84 temporal cases, 39 recall cases and 87
proof pages, as well as every warmup and timed repetition. Four dated recall
axes and UNKNOWN controls preserve the same evidence and answer behavior.

Times below are milliseconds. Each cell is baseline → candidate. Peak RSS is
process VmHWM in MiB, including preceding correctness controls. First-tool time
excludes process initialization; the first Ask follows Wake in the same process
and therefore is not process-cold. The small temporal first-query measurement
regressed (26.8 → 35.5 ms); the warm result alone does not describe that cost.

| Operation | Entries / source bytes | First tool | Warm p50 | Warm p95 | Peak RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Goto | 32 / 1024 | 26.8 → 35.5 | 16.1 → 13.6 | 20.5 → 15.1 | 33.4 → 33.6 |
| Goto | 256 / 16384 | 220.1 → 170.1 | 145.7 → 30.5 | 146.2 → 32.1 | 89.9 → 46.9 |
| Goto | 1024 / 32768 | 1050.1 → 1012.4 | 904.0 → 90.5 | 905.4 → 92.4 | 452.0 → 120.7 |
| Wake | 32 / 1024 | 762.4 → 309.7 | 523.8 → 58.1 | 525.4 → 59.0 | 60.1 → 59.5 |
| Ask | 32 / 1024 | 576.7 → 103.5 | 580.1 → 97.1 | 580.7 → 97.7 | 60.6 → 59.9 |
| Wake | 128 / 2048 | 3233.8 → 496.1 | 3023.8 → 263.7 | 3035.0 → 265.7 | 67.9 → 62.8 |
| Ask | 128 / 2048 | 3157.2 → 400.0 | 3155.0 → 400.1 | 3180.9 → 401.7 | 69.2 → 63.2 |
| Wake | 256 / 4096 | 10639.3 → 999.9 | 10389.3 → 733.3 | 10399.3 → 748.8 | 84.4 → 66.3 |
| Ask | 256 / 4096 | 10776.9 → 1112.4 | 10768.4 → 1097.6 | 10787.2 → 1114.4 | 84.6 → 67.5 |

The largest temporal median improves 10.0×; the largest Wake and Ask medians
improve 14.2× and 9.8×. At 1,024 entries, temporal CPU across the 20 measured
queries falls from 18,040 to 1,740 ms (coarse process ticks). Response bytes are
unchanged. The recording-adapter tests additionally prove zero repeated
catalogue reads for a warm revision while clock, scope and write invalidation
remain enforced; excluded recall entries and their sources never reach the
body reader. This is not a claim of zero database calls or physical I/O.

The local full quality gate passes: 2,327 Rust tests, four existing ignored
tests, contracts, viewer JavaScript tests, formatting, workspace clippy with
warnings denied, rustdoc with warnings denied, and the native binary build.
Native stdio fixtures now feed requests while draining output to avoid a
full-pipe deadlock. gRPC fixtures expose their existing neighbor data through
the point-batch port; their evidence assertions are unchanged. Test temporaries
were outside Git, within the workspace; Cargo's shared target was configured
only on the checkout so nested release fixtures retained their own targets.

Raw RPCs, source fixtures, environment, source hashes and validation logs are
retained under `artifacts/performance-770-indexed/` (ignored). The two paired
runner scripts and the allocation control are checked in for reproduction.

Binary SHA-256:

- Baseline: `57807d65471c2150493e83a463667e2001e6b95e311f0e8afdf29b5c6f39841f`.
- Candidate: `753c7fefab22bf63075a95f2a4bcd8fb020733022499511d7974432f52203e02`.

A separate uninstrumented repeat of the largest temporal case, without
concurrent builds, tests or scratch cleanup, gives warm p50 904.2 → 86.7 ms
and p95 909.8 → 88.7 ms over another 20 alternating pairs after three warmups.
All 46 responses again match the reference. This repeat checks the warm result
independently of the desktop activity during the original run.

Allocation control totals below include startup, initialization and four
identical queries per process. Bytes are cumulative requested GB (decimal),
not a live-heap measurement. All 48 instrumented responses match their
uninstrumented references. The substantial first-read work remains visible in
these totals, especially for the large temporal catalogue.

| Operation | Entries / source bytes | Allocation requests | Requested GB |
| --- | ---: | ---: | ---: |
| Goto | 32 / 1024 | 196,555 → 158,989 | 0.041 → 0.032 |
| Goto | 1024 / 32768 | 3,364,124 → 2,013,960 | 11.928 → 4.090 |
| Wake | 32 / 1024 | 2,234,607 → 705,317 | 0.242 → 0.094 |
| Ask | 32 / 1024 | 2,422,477 → 846,898 | 0.257 → 0.104 |
| Wake | 256 / 4096 | 33,521,640 → 3,166,693 | 3.924 → 0.902 |
| Ask | 256 / 4096 | 34,686,480 → 4,279,781 | 3.985 → 0.953 |
