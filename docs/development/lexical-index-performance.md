# Revision-bound lexical collection reuse (#771)

Ask used to rebuild two BM25 field statistics maps and its term co-occurrence
association index for every question and every page. These are separate costs:
`AssociationIndex` computes PMI neighbours; it is not a BM25 index. The control
below measures both independently before timing the complete native operation.

Each embedded MCP backend and gRPC service now owns one `LexicalIndexCache`.
It holds only the immutable collection statistics, association neighbours and
an exact equality witness consisting of ordered content/direct term counts.
Evidence bodies, graph features, question results, bridge matches, eligibility,
confidence, ordering and `UNKNOWN` decisions are recomputed from the current
application result. Candidate preparation and query scoring remain work per
call. This change does not introduce a different ranking algorithm.

The application carries the existing `GraphReadRevision` from its pinned
operation into `GetContextResult`; this field is internal Rust read metadata,
not a new wire field. The key includes that revision (which distinguishes store
and reader incarnation), root about, requested scopes and temporal selection,
including the clock and cutoff. Exact equality of the admitted ordered term
counts additionally checks collection membership and normalization. No term is
truncated or replaced with a hashed identity. BM25/PMI constants are compiled
into the cache's process; there is no mutable scoring configuration. Question,
answer policy, lexical bridge and page are intentionally outside this key because
they affect subsequent query work, not these collection statistics.

A changed snapshot, scope, cutoff or term-count witness builds and replaces the
single entry. A reopened backend starts empty. A peer or raw SQLite write changes
the existing snapshot revision mechanism, with exact input equality as a second
guard. A backend unable to attest to a consistent revision uses the ordinary
uncached build. Old in-flight reads keep their immutable collection; publication
under a mutex cannot change it. Construction and ranking run outside the mutex.
Concurrent misses may duplicate a build; they cannot share incompatible statistics.

Retention admission allows at most 8,192 candidate documents and a conservative
32 MiB allowance for keys, maps, strings, neighbour vectors and equality witnesses.
A preliminary input allowance avoids cloning an oversized witness, and a second
check charges the actual constructed neighbours and string capacities before
retention. A large term repeated as many neighbours is included in that second
check. Oversized reads still process all their terms and evidence, then discard
the index. The allowance is not a process RSS or peak-allocation guarantee:
construction, active old readers and other application data also consume memory.
There is no persistent index, schema migration, new database write or write-time
index maintenance. The existing quality journal remains separate.

## Phase control

The informational ignored Rust control runs the current uncached algorithm and
the cached path in the same executable, alternating order, with three warmups
and 20 recorded samples per shape. All ranked results and complete mapped
responses must match. Fixtures use distinct vocabulary, with canonical text
retained in full. The profile is dev with the workspace Cargo configuration,
Rust 1.97.1 on Linux aarch64. These are synthetic unoptimized measurements,
not release or production timings. The bridge is silent in this control.

| Entries / extra vocabulary per entry | Preparation once (ms) | PMI build p50 (ms) | Both BM25 fields p50 (ms) | Cache lookup p50 (ms) | Full rank p50 fresh / cached (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 8 / 4 | 2.896 | <0.001 | 0.045 | 0.020 | 5.000 / 4.986 |
| 64 / 16 | 39.840 | 9.514 | 0.753 | 0.311 | 90.836 / 80.624 |
| 256 / 64 | 473.946 | 357.510 | 10.365 | 3.782 | 1264.813 / 900.668 |
| 32 / 256 | 222.830 | 660.459 | 5.282 | 1.746 | 1071.503 / 406.488 |

The first collection builds took 0.105 / 10.989 / 374.338 / 667.984 ms. The
retained structure allowances were 33,374 / 452,254 / 5,234,774 / 2,458,798 bytes,
excluding the small identity. Fewer than twelve candidates do not build learned
associations under the existing algorithm; the small case has little to save.
Preparation and repeated query work still dominate some larger cases. The
optimization removes repeated statistics construction, not all lexical work.

## Native comparison and correctness

The native runner compares main `fdb2020f` with this change in ABBA process order,
using copies of one closed seed per shape, three warmups after the first query
and 20 warm samples per process: 40 samples per side and shape. Complete MCP
results are compared byte-for-byte as parsed JSON, with no omitted or normalized
fields, and the first result must contain citations. Process startup is excluded
from per-query time; first-query results are recorded separately. OS caches are
not flushed. Only one benchmark process runs at a time, with no overlapping
builds. Process peak RSS includes initialization and all reads, not just the index.

The earlier `native/` attempt inherited expired fixture entries and returned
`UNKNOWN`. It was stopped after inspecting that evidence; its partial samples
remain diagnostic and are excluded from the final comparison. `native-final/`
removes the fixture expiry and requires actual citations before recording results.
This is a fixture correction, not a change to KMP eligibility or expiry semantics.

Focused tests compare exact floating-point BM25 score bits, floors, association
weights and complete answers against fresh collection statistics, including
changed counts and corpus membership under a deliberately reused identity.
They also cover revision/store/scope/clock changes, oversized/no-revision bypass,
concurrent publication and retention of an old index. The real SQLite race pauses
an Ask after its catalogue read, commits a peer write, and verifies that both its
bodies and its exported revision remain the complete previous state; the next
read observes the new revision. Existing lifecycle, graph, language, scope and
transport tests remain required.

The separate native replay checks policies, UNKNOWN, temporal cutoffs and
intervals across five clocks, whole-entry selectors, cross-about scope, repeated
queries and response budgeting against the original binary. Allocation counting
uses the existing Linux/glibc interposer separately, never for latency claims.
It measures cumulative allocation requests/bytes for startup plus one or five
queries, not live heap or physical disk I/O.

## Reproduction

Build the baseline MCP from `fdb2020f` and the candidate with the same toolchain
and Cargo settings. Save the baseline binary outside a target path that the
candidate build overwrites. The runners require Python 3, Linux `/proc`, and no
model, network service or token encoder. Raw JSONL traces, summaries, source and
binary hashes are kept in [performance-771](../../artifacts/performance-771).

```bash
cargo build --locked -p kmp-mcp
KMP_LEXICAL_BENCH_OUT="$PWD/artifacts/performance-771/phases.json" \
  cargo test --locked -p kmp-proto-mapping lexical_index_phase_control \
  -- --ignored --test-threads=1
python3 scripts/performance/lexical_index_cache.py \
  /absolute/baseline-kmp-mcp target/debug/kmp-mcp \
  artifacts/performance-771/native-reproduction tmp/performance-771-native 20
python3 scripts/performance/lexical_index_parity.py \
  /absolute/baseline-kmp-mcp target/debug/kmp-mcp \
  artifacts/performance-771/parity-reproduction tmp/performance-771-parity
cc -shared -fPIC -O2 -Wall -Wextra -Werror \
  -o tmp/performance-771-counter.so scripts/performance/native_allocation_counter.c
python3 scripts/performance/lexical_index_allocations.py \
  /absolute/baseline-kmp-mcp target/debug/kmp-mcp tmp/performance-771-counter.so \
  artifacts/performance-771/allocation-reproduction tmp/performance-771-allocations
bash scripts/ci/quality-gate.sh
```

Scratch stores are removed by the runners. Performance thresholds remain
informational; the ignored control has no absolute latency assertion. No guide
or schema regeneration is needed because the agent-facing contract and storage
format are unchanged.
