# ChronoLoom startup and intent waterfall (#777)

This evidence compares production `main` at
`26956aba5d19aa4165e3216e1fd7af5a8f350a2f` with the #777 candidate at
`0eaa095a12407bebd25e4f30b1e89196379ff03b`. The baseline includes the merged
#776 asset work from PR #794, so no asset-delivery gain is attributed to #777.

Both executables use the same Cargo dev profile and workspace
`.cargo/config.toml`, with no profile override. The baseline is 107,205,216
bytes with SHA-256
`2beef5a0b53151c8624946aa003f54296d7f99895857b44b50a4316b663ae0a7`;
the candidate is 107,205,192 bytes with SHA-256
`2996f988a61b458d6f72ad4c4f6fd1951c17fae2e31997687bc672fdfeac8f01`.

## Reproduced waterfall

The production baseline confirmed duplicate orchestration rather than a fixed
five-request sequence:

- Agent open made four projection reads: the Episode extent, Atlas navigator,
  Moment scene, and the same Moment scene again while applying the aggregate.
- A same-clock focus plus direct proof trace made two identical node batches
  and two identical Moment projections.
- A rich-state reload made five projections on the current clock. Changing to
  observed or ingested first repeated an Episode/Atlas/Moment clock reload,
  increasing the reload to eight projections.
- The 32-entry fixture did not trigger the separate density-rung retry. The
  projection's serialized `level_of_detail` is now the authoritative applied
  rung. The existing compatibility retry remains when the response density
  resolves a different rung.

## Method

`scripts/performance/chronoloom_startup.py` ran eight samples per side from
00:02:41 through 00:03:20 UTC on 2026-09-14 in an exclusive quiet slot. Every
sample used a fresh synthetic embedded store, `kmp-mcp` process and Chromium
context. It ingested 32 non-personal memories with deterministic occurred,
observed and ingested clocks, plus 11 bounded proof relations and no expiry.

Each sample measured three complete operations:

1. `kmp_view_open` through a usable HTTP scene in a fresh browser context.
2. `kmp_view_get_state` plus `kmp_view_apply_intent` through adopted revision,
   selected ref and rendered trace. Samples rotate occurred, observed and
   ingested axes.
3. A browser reload in the same context and process, joining the existing
   focus, trace, selection, clock and revision.

Usable means the stage reports `loading=false`, the scene is nonempty, view
polling is live, intent application is settled, the expected revision and
facets are present, and two animation frames have completed. A 75 ms grace
captures responses completed at that boundary. Long polling is counted when
started and is not used as a `networkidle` condition.

Timings are uninstrumented local wall time. The p95 of eight samples is the
largest sample and remains informational. Chrome DevTools request sizes include
response body and headers. Dynamic state bytes remain present; the reductions
come from requests that no longer repeat. Linux process current/peak RSS and
cumulative CPU ticks were sampled after each operation. No allocator preload
was used, so the report makes no allocation-count claim.

## Results

| Complete operation | Baseline p50 / p95 | Candidate p50 / p95 | HTTP request change | Completed response bytes |
| --- | ---: | ---: | ---: | ---: |
| Agent open | 751.591 / 791.100 ms | 725.886 / 749.973 ms | 41 -> 39 | 346,169 -> 314,421 |
| Focus + trace, mixed clocks | 652.158 / 694.831 ms | 637.596 / 662.133 ms | 7 or 10 -> 5 or 7 | see clock table |
| Rich-state reload, mixed clocks | 897.350 / 945.295 ms | 888.307 / 931.773 ms | 45 or 48 -> 42 | see clock table |

The p50 changes are about 25.7 ms for open, 14.6 ms for focus/trace and 9.0 ms
for warm reload. They are small local samples, while the request and byte
shapes below are deterministic within each clock.

| Operation and clock | Projection reads | Node-batch reads | Total requests | Completed response bytes |
| --- | ---: | ---: | ---: | ---: |
| Focus + trace, occurred | 2 -> 1 | 2 -> 1 | 7 -> 5 | 66,423 -> 37,569 |
| Focus + trace, observed/ingested | 5 -> 3 | 2 -> 1 | 10 -> 7 | 102,598 -> 48,014 |
| Rich reload, occurred | 5 -> 3 | 2 -> 1 | 45 -> 42 | 123,383 -> 68,800 |
| Rich reload, observed/ingested | 8 -> 3 | 2 -> 1 | 48 -> 42 | 159,558 -> 68,800 |

Peak-RSS medians changed from 36,520 to 36,484 KiB for open, 64,678 to
64,900 KiB for focus/trace and 64,704 to 64,952 KiB for reload. The largest
observed candidate peak was 65,084 KiB versus 64,764 KiB baseline. Median CPU
ticks were 7 -> 6, 35 -> 34 and 38 -> 36. These process-level samples are
reported as neutral local variance, not an allocation improvement.

All 24 operation pairs matched exactly after excluding the transport-local
view revision counter from content parity. The raw revisions were nevertheless
the same in every pair: 1 for open and 2 for focus/trace and reload. Compared
content includes about, clock, selected ref, trace refs and edges, projection
content hash and revision, resolved LOD, visible window, scene count, total and
every projected ref.

## Correctness coverage

The implementation changes only ChronoLoom's private browser and MCP Apps
orchestration. `/api/info`, `/api/abouts` and the initial view read begin
together. MCP Apps coalesces catalogue and initial-view lookup onto one
in-flight `kmp_view_get_state` call. The authoritative clock, labels, layer
abouts and requested rung are adopted before the extent probe. A clock change
from an empty scene is owned by the one awaited snapshot load and cannot launch
a competing background load.

A trace frames its authoritative proof-path ref set once through the existing
4,096-ref, 64-ref batch and 32,768-edge budgets. A focus-only ref outside a
successful path does not widen the final window; a subsequent outside
selection still performs the prior reveal and center operation. Focus refs
remain the fallback if the trace cannot be read or framed. This preserves the
old final-window and missing-node behavior while removing the earlier focus
projection that a successful trace immediately replaced.

The failure path retains the original meaning: a trace that cannot be read or
framed falls back to the focus refs, an empty focus falls back to the full
projection, missing nodes remain named, snapshot conflicts retry once, and an
independent catalogue failure remains visible even when no initial aggregate
exists. A primary about repeated in `projection.abouts` is normalized out. A
single-about full scene supplies its own exact navigator bins; layered and
narrowed views retain the separate full navigator projection.

No HTTP, MCP tool, guide, authorization, capability, cache or public viewer
semantics changed. Existing capability and HTTP smoke coverage remains the
security boundary validation for this change.

## Evidence files

- `measurement-final/environment.json` records toolchain, hardware, binary and
  runner hashes, fixture shape, sample count, clocks and observables.
- `measurement-final/results.json` contains summaries and exact parity.
- `measurement-final/waterfalls.json.gz` contains every raw operation, request,
  timing, size, scene state and process resource snapshot.
- `measurement-final/compressed-manifest.json` records raw-equivalent and
  compressed sizes plus the compressed SHA-256.
- `measurement-pre-review-superseded/` preserves the earlier experiment. Its
  candidate preceded the empty-clock and exact trace-window corrections, so
  those timings are excluded from the final comparison.
- `experiment-provenance.json` identifies both experiments and why only the
  final one supports the result.
- `source-files.sha256` binds the implementation, focused tests and runner used
  for the accepted measurement.
