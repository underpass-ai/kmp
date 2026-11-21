# Reproducing KMP and ChronoLoom performance

Issue [#765](https://github.com/underpass-ai/kmp/issues/765) supplies one local
runner over synthetic stores. The reported Fable timings (Wake 300–365 ms, Ask
475–520 ms, viewer about 946 ms) came without executable provenance or raw
traces. They are hypotheses, not the baseline or a CI threshold.

## Existing controls

The runner complements these committed controls; it does not replace their
focused semantic or allocation checks:

| Control in `scripts/performance/` | Measured boundary |
| --- | --- |
| `temporal_body_selection.py`, `recall_body_selection.py` | Paired native temporal/Wake/Ask admission and full-result parity |
| `dimensional_lookup.py`, `dimensional_read_parity.py` | Dimension lookup and selector parity |
| `node_batch_framing.py` | Bounded node framing and snapshot handling |
| `visual_projection_cache.py` | Revision-bound visual projections |
| `chronoloom_scene.py`, `chronoloom_scene_control.js`, `chronoloom_scene_checks.js` | Isolated real Three.js scene allocation, render and disposal; injected scene, no HTTP startup |
| `lexical_index_cache.py`, `lexical_index_parity.py`, `lexical_index_writes.py` | Native revision-bound lexical-index reuse and write invalidation |
| `lexical_index_allocations.py`, `native_allocation_control.py`, `native_allocation_counter.c` | Separate Linux/glibc allocation-request controls |

Existing reports are `temporal-read-performance.md`,
`dimensional-read-performance.md`, `visual-projection-performance.md`,
`chronoloom-scene-performance.md`, and `lexical-index-performance.md` in this
directory. Their profiles, fixture shapes and scopes differ: do not combine
their medians into an end-to-end latency.

## Run

Build the compared revisions with the same toolchain, features and Cargo
configuration. Preserve each binary before building the next revision. The
runner records its SHA-256; `--commit` and `--profile` are explicit declarations
of its source and build provenance. Inside the maintainer's workspace the
parent `.cargo/config.toml` also applies. Do not override it with profile
environment variables.

```sh
uv venv tmp/performance-venv
uv pip install --python tmp/performance-venv/bin/python tiktoken==0.14.0 playwright==1.58.0
tmp/performance-venv/bin/python -m playwright install chromium
cargo build --locked -p kmp-mcp
tmp/performance-venv/bin/python scripts/performance/baseline.py \
  target/debug/kmp-mcp artifacts/performance-baseline/run-1 tmp/baseline-run-1 \
  --commit FULL_SOURCE_COMMIT --profile 'dev, inherited workspace Cargo configuration' \
  --samples 20 --warmups 2 --browser --browser-samples 5
```

The output and scratch directories must be new. The runner cleans only the
scratch directory it created. All stores are generated there; it does not open
the user's memory. Failure traces are retained as diagnostics and must be
excluded from reported measurements. Use `--samples 1 --warmups 0 --shapes small
--browser-samples 1` for a smoke test before reserving an otherwise idle host.

Four bounded shapes cover 8 entries/256-byte sources, 32 entries/1-KiB sources,
64 entries with up to 8 outgoing proof edges, and 8 entries/32-KiB sources.
Coordinates span occurred, observed and ingested clocks and two dimensions.
The current positive fixture has no expiry; historical navigation explicitly
selects the observed clock. Each shape is seeded and closed, then reopened in
a fresh process. OS caches are not flushed. Relation observation and ingestion
clocks are fixed too, so independently seeded binaries produce comparable
responses. Wake/Ask use a 2-MB byte ceiling and at most eight selected entries;
the generous byte ceiling lets the high-degree fixture return its selected
evidence on the first page instead of filling that page with relations alone.

Wake, Ask, Goto and Forward each record one first-operation sample, optional
warmups and the requested warm samples. Every repeated complete native result
must equal its first result, and positive recall must contain evidence.
Sequential `kmp_write_memory` calls create distinct accepted memories after
the read controls. Startup is reported separately from read latency.

The browser path uses the actual HTTP server and bundled application. It
measures MCP open through the first drawn nonempty scene, then reloads within
one browser context, and finally measures get-state/apply-focus-and-trace
through the browser adopting that revision. Completion requires a settled
loading indicator, matching about, revision, selection and proof trace, followed
by two animation frames. A new context has a cold browser cache; server and OS
caches may already be warm. One first-open sample is not a stable p95.

The waterfall retains requests, response sizes and Resource Timing entries,
including dynamic requests and view polling. No `networkidle` heuristic waits
for a long poll to stop. MCP and HTTP call counts are reported separately.
Chromium uses software rendering in this environment; these figures describe
that environment, not desktop GPU performance.

## Accounting and attribution

`environment.json` names the source commit, binary and runner hashes, profile,
hardware, Python, Chromium, concurrency and sample counts. Raw JSON-RPC
responses are kept losslessly compressed. `manifest.json` gives compressed
and original hashes. The named encoder is `cl100k_base` from tiktoken 0.14.0,
counting compact complete JSON-RPC responses after native latency measurement.
These are reference transport token counts, not billed model input.

Native CPU comes from Linux process ticks, at the recorded clock frequency.
`VmHWM` is the server's lifetime high-water mark; it is not a per-call heap
delta. Browser CDP metrics are separate cumulative browser metrics. Linux
`rchar/wchar` and syscall counters include pipes and cached I/O;
`read_bytes/write_bytes` are process storage accounting. None is presented as
SQLite cache hits, SQLite page reads or a physical device trace. SQL statement
counts are unavailable in this runner; MCP calls and HTTP requests are exact.

Embedded Wake/Ask now expose their existing application timing breakdown at
the opt-in tracing target `kmp_mcp::query_phases=debug`. The runner saves these
events in `phases.json`: graph/catalogue and selected-node loading, selected
detail loading, bundle assembly, role count and materialization batch size.
These phases exclude ranking, prose rendering, telemetry and transport. Their
sum is not the complete latency. No timings are added to the MCP contract.

Allocation measurement runs separately, because instrumentation changes time:

```sh
cc -shared -fPIC -O2 -Wall -Wextra -Werror \
  scripts/performance/native_allocation_counter.c -o tmp/native-allocation-counter.so
tmp/performance-venv/bin/python scripts/performance/baseline.py \
  target/debug/kmp-mcp artifacts/performance-baseline/allocations tmp/baseline-allocations \
  --commit FULL_SOURCE_COMMIT --profile 'same dev build; allocation instrumentation' \
  --samples 5 --warmups 0 --allocation-counter tmp/native-allocation-counter.so
```

The preload counter covers native process startup, reads and writes, excluding
seeding and the Python/browser processes. It reports allocation requests and
cumulative requested bytes, not peak live allocation. Do not interpret timings
from this run. Run one and five samples separately when isolating marginal
repeated-work allocations; the individual hotspot controls provide narrower
attribution.

After collecting comparable uninstrumented runs, verify complete read parity:

```sh
python3 scripts/performance/baseline_compare.py \
  artifacts/performance-baseline/before artifacts/performance-baseline/after
```

Performance remains informational. Semantic differences, missing proof or
failed journeys stop the runner instead of being reported as faster queries.

## Reads at scale: one about of 10^3 to 10^5 memories (1.0)

Before 1.0 a read cost grew faster than the about: one about of 100,000
memories took 22 s to wake and 90 s to ask. The measurement and the fixes
are recorded here; the method is reusable for any read.

**Store and calls.** The memory bench's cached synth-v1 seed 7 mono stores
(10^3, 10^4, 10^5 entries in `synth:mono-a000`), a fresh copy per process,
default settings. Per size one stdio process: startup, a first `kmp_wake`,
then seven each of `kmp_wake`, `kmp_ask` (a question built from three words
of the wake text) and `kmp_time` rewind over the whole interval, five
single-memory writes; p50 of the seven, peak RSS (`VmHWM`). Release builds,
cores 5–9, no build running. The bench's gold questions (`kmp_ask` and
`kmp_wake` of the ≤ 10^4 levels, every second one, plus the first
continuation page) were replayed on both binaries and compared call by call.

**Profile.** `perf` needs a lower `perf_event_paranoid` than shared hosts
allow, so two in-process tools were used: stage timers around each stage, and
a SIGPROF sampler (`setitimer(ITIMER_PROF)`, glibc `backtrace()` into a static
ring, symbolized with `addr2line -i` against a release build with line
tables). Neither is in the tree; both are throwaway patches.

**Where the time went (10^4, before).**

| read | largest terms |
|---|---|
| first wake, 2.1 s | render 1.1 s: every section of the whole about tokenized with cl100k, though a wake reads only the summary tier, and the flat sections only while fewer than five live memories fill its state; catalogue 0.4 s (two SQL statements per node); node materialization, response, JSON and paging 0.1–0.2 s each |
| ask, 6.4 s | SQLite re-parsing the sidecar's `relation` statement for every edge (28 %); the ranker tokenizing every candidate again (27 %), though the sidecar row holds the same counts; four top-k rounds (142 → 380 → 1,337 → 3,153 candidates), each reading and ranking from nothing; at 10^5 a scan of every relation per lifecycle end in the plan (10 s) |
| time, 0.64 s | the whole catalogue read on every call (the temporal index cache keeps catalogues of at most 8,192 nodes) |

**What changed (answers unchanged).**

- A wake renders the summary tier alone (`RenderDemand::Summary`) and renders
  the flat sections from the same bundle only when its state needs them.
- Every SQLite connection of the store and the sidecar sets the query
  planner stability guarantee: a cached statement is no longer re-planned and
  re-parsed when its parameters change. No plan depends on a bound value.
- The sidecar indexes `lex_relation(about, type, target)`, so an about's
  lifecycle relations, and what supports each end, are found without
  scanning its relations.
- The top-k rounds of one ask share their point reads (`PartsMemo`), and a
  round that found too few eligible candidates reads enough for its depth at
  the rate it found them (`next_limit`) instead of four times as many.
- The ranker takes an indexed candidate's counts from its sidecar row.
- A node header is read with one statement instead of two.

Before (integration/v1.0 at 6dd25e97) → after, one about, p50 of seven
calls, release builds on the GX10 (aarch64), 21 Nov 2026:

| memories | first wake | wake | ask | time (rewind) | write | peak RSS |
|---:|---:|---:|---:|---:|---:|---:|
| 1,000 | 279 → 156 ms | 192 → 79 ms | 260 → 250 ms | 15 → 15 ms | 8 → 1 ms | 115 → 115 MB |
| 10,000 | 2,065 → 981 ms | 151 → 153 ms | 6,271 → 1,979 ms | 625 → 491 ms | 7 → 3 ms | 644 → 650 MB |
| 100,000 | 22.6 → 11.1 s | 22.7 → 11.0 s | 92.4 → 27.8 s | 7.1 → 5.5 s | 5 → 7 ms | 4.7 → 5.1 GB |

A repeated wake at 10,000 is served from the frozen read (150 ms on both);
at 100,000 it is not kept, so every wake pays the read. Writes are within
noise. Gold questions at 10,000 (148 asks and their first continuation
pages, every second question of the bench's ≤ 10^4 levels): ask p50
1,220 → 755 ms, all 148 asks 444 → 231 s, first wake 2.1 → 0.94 s; 133 asks
answered from the index and 31 that read the whole about on both binaries.

**Parity.** Every gold call (206, continuation pages included) is equal
between the two binaries after the replica's `store.path` and the
per-process `read_<hex>` handle; with `KMP_LEXICAL_INDEX=verify` every
indexed answer (157) equals the whole about's; the retrieval, Jev and relate
baselines hold unchanged. `quick-a` against the base binary is `neutral`:
parity 1.0 on the real store (124 questions) and on synth 10^3 (88), quality
equal. The bench's normalizer now treats `structuredContent.store.path` as the
replica-dependent value it is. A continuation page's `used_bytes` counts that
path too, and the replica's directory embeds the variant's name, so compare
variants with names of equal length.

**What remains.** Every read above still reads the whole about once; what changed is the
work per memory and what was done several times. The 1.0 targets of
500 ms at 10^4 and 3 s at 10^5 need reads that follow what they return:

- **Ask.** A question whose words reach a third of the about (the measured
  one plans 3,153 of 10,000 candidates) reads and ranks every one of them:
  top-k cannot certify an answer because eligibility (the strict focus) also
  counts words a candidate's relations and supported claims carry, which its
  row does not hold, so too few of the best prefixes are eligible. Bounding it
  needs that focus indexed per candidate. The plan still reads the df of every
  term the reached rows hold; 31 of 148 gold asks at 10^4 cover more than
  35 % of the about and read the whole about, as before.
- **Wake.** It reads the whole about (catalogue, nodes, bodies), orders all
  its evidence by lifecycle and relation signals, and renders the whole
  packet to JSON before cutting a page. Following the page needs that order
  kept per about and the page materialized from it.
- **Time.** Every call reads the whole catalogue: the temporal index is kept
  only for catalogues of at most 8,192 nodes and 64 MiB. A position index per
  clock in the store, or keeping larger indexes, would bound it.
- **Memory.** Wake and the about path hold the whole about several times
  over; peak RSS at 10^5 stays above 4 GB.

## Reads at scale, second pass: what a read cannot return is not read (1.0)

The first pass made each memory cheaper; this one stops reading what an
answer cannot use. Same stores, same script (`latency_direct.py`), same
gold questions; the base is `integration/v1.0` at a9b6e4d7.

**Where the time went (10^4, before).** Stage timers and the SIGPROF
sampler of the first pass, on a profiling build of the base.

| read | largest terms |
|---|---|
| ask, derived question, 2.1 s | the plan 0.72 s: df of 22,630 terms (0.14 s), the associations counted twice over 4,931 seed rows (0.18 s), every reached row's terms collected (0.09 s), the prefixes (0.21 s); then a first top-k round that certifies nothing (no candidate is eligible) and a second that reads and ranks all 3,153 planned candidates (1.1 s) |
| ask, gold anchored question, 0.98 s | the plan 0.6 s (the same terms), one round 0.3 s |
| first wake, 1.0 s | the catalogue: one JSON projection statement per node (`json_extract` × 7 and `json_object`), then every node read again in full once admitted; the cl100k table loaded inside the first read (50 ms); label references parsed and re-encoded per entry; render and projection of the whole packet |
| time, 0.5 s | the catalogue read on every call (the temporal index is kept only below 8,192 nodes), label references parsed per entry, then the proof plan and the mapping |

**What changed (answers unchanged).**

- **Reach (`lex_reach`).** Each candidate's row is joined by the words its
  graph can lend a strict focus: the slugs of the refs it supports, the
  details of its refs, the explanatory relations the selection keeps at
  them. Top-k leaves out a candidate that cannot clear the floor, or whose
  focus over its direct field and its reach stays below two thirds of the
  question's focus concepts: no reading can find it eligible. The derived
  question plans 3,153 candidates and none is eligible; top-k now reads the
  55 it must and certifies in one round. The sidecar is `lexical-index-6`.
- **The plan reads only the df an ask weighs.** The question's words, its
  anchors, the associations and the words a question word shares three seed
  rows with (22,630 → 4,693 terms on the derived question); the pairs are
  counted once per ask, in a hash map, and shared with every ranking of it;
  the prefixes are read from the rows in place; a candidate's id and row
  come in one statement.
- **Headers are decoded from the stored JSON** by serde, which skips what it
  does not name, instead of a seven-path `json_extract` per node.
- **A read with no dimensions of its own reads each node once**: whole in
  the catalogue, instead of a header and then the node again once admitted.
- **The temporal index of a 10^4 about is kept** (up to 16,384 nodes,
  65,536 relations, 256 MiB of its conservative allowance).
- **Label references** are parsed once per thread and kept (16 of them).
- **The cl100k table** loads beside startup instead of inside the first read.

TABLE

GOLD

**Profile after (10^4, profiling build, other builds running).** The
measured question: plan 0.30–0.36 s (its rows 0.06 s; seed df, counting the
pairs and reading the partners' df 0.19 s; prefixes and the reaches of 3,153
candidates 0.09 s), then one round reading the 55 candidates it must
(0.05 s of parts, 0.03 s of ranking). A gold anchored question: plan 0.2 s,
one round 0.4 s, most of it the gate reading its 224 candidates. The plan is
now bounded by the rows the postings reach (4,931 here; 49,219 at 10^5,
where it takes 2.2 s). A first wake: the catalogue and the bodies 0.3 s,
`wake_response_rendering` 0.12 s, the JSON of the whole packet
(`wake_value`) 0.16 s and its page 0.11 s. A rewind served from the kept
index: the dimension filter over the index's whole bundle and its
validation, the proof labels of every entry, and the traversal (together
about 70 % of the call). Raw: `read-perf-2/profile-final/`.

**Parity.** PARITY

**Sidecar upgrade.** UPGRADE

**What remains.**

- **First wake at 10^4** (target 500 ms). It still reads the whole about
  and renders the whole packet to JSON before its page is cut; what is left
  is spread over the read, the response, `wake_value` and the projection.
  Following the page needs that order kept per about and the page
  materialized from it, as the first pass said.
- **Rewind at 10^4** (target 100 ms). The index is kept, but every call
  filters the index's whole bundle by dimension, validates the bundle it
  builds and reads the proof labels of every entry. For a read with no
  boundary these are the same on every call of one index and could be kept
  with it.
- **10^5.** The plan reads every row the postings reach (tens of thousands
  for a common word); an anchored question reads every candidate that names
  an anchor; wake and time read the whole about; the temporal index is not
  kept past 16,384 nodes.
- **Peak RSS of a long session.** On the gold replay at 10^4 the peak grows
  ask by ask on the heavy anchored questions (the index now answers them
  instead of the whole about). The single-process latency run peaks lower
  than before.
