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

Before (`integration/v1.0` at c517577b) → after, one about, p50 of seven
calls (first wake: one call; write: five), release builds without
`eval-hooks`, cores 5–9, both binaries back to back under the build lock,
21 Nov 2026; other agents' processes kept the load average at 9–12:

| memories | first wake | wake | ask | time (rewind) | write | peak RSS |
|---:|---:|---:|---:|---:|---:|---:|
| 1,000 | 180 → 140 ms | 86 → 62 ms | 257 → 233 ms | 16 → 13 ms | 8 → 7 ms | 130 → 119 MB |
| 10,000 | 1,032 → 742 ms | 156 → 155 ms | 2,039 → 357 ms | 506 → 153 ms | 9 → 7 ms | 823 → 557 MB |
| 10,000, shipped table | 1,012 → 813 ms | 151 → 151 ms | 3,927 → 3,132 ms | 505 → 145 ms | 4 → 29 ms | 796 → 856 MB |
| 100,000 | 11.4 → 10.4 s | 11.0 → 8.0 s | 28.0 → 3.0 s | 5.6 → 4.0 s | 143 → 15 ms | 5.1 → 3.9 GB |

Repeated first wakes with the table: 995–1,027 ms for the base and
743–911 ms here. With the table the measured question reaches 6,948 candidates;
the floor leaves 4,839 and the reach sets 4,805 of them aside, but about
4,780 carry a word the table bridges to, and top-k always reads those (a
bridged word may rescue them): 4,815 candidates, more than 35 % of the
about, so both binaries read the whole about for it (`verify` on the final
build, equal answers). The gold questions with
the table (a replay beside another, so indicative): ask p50 2,623 → 992 ms,
first wake 1,429 → 717 ms.

Writes at 10^4 after the sidecar is built (30 single-memory writes, twice
per binary): p50 9.4 and 82.9 ms → 3.1 and 3.2 ms (p90 5–7 ms); the base's
second run met a burst of disk traffic from elsewhere.

**Gold questions at 10^4** (148 asks and their first continuation pages,
every second question of the bench's ≤ 10^4 levels), same conditions: ask
p50 744 → 415 ms, p90 4.9 → 2.1 s, all 148 asks 232 → 118 s, asks within
500 ms 34 → 95; first wake p50 942 → 696 ms; the rewind 492 → 339 ms (one
call, not kept yet); peak RSS of the replaying process 1,165 → 1,011 MB
(inconclusive across runs, below).
The index answers 180 of the 184 indexed-shaped reads (133 before; the rest
read the whole about because their candidates cover too much of it), and
with the shipped table 149 of 174 (110 before); counted from the debug log
on the two merges before c517577b, whose read code is the same.

**Profile after (10^4, a profiling build of this branch before the
a9b6e4d7 merge, other builds running).** The
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

**Parity** (against the merged base, `integration/v1.0` at c517577b).

- Gold calls at 10^4, call by call (148 asks with their first continuation
  pages, the five wakes, the rewind): 206 of 206 equal; with the shipped
  lexical bridge table (`KMP_LEXICAL_BRIDGE`), 207 of 207. At 10^5, every
  40th question with its first page: 15 of 15 equal.
- `KMP_LEXICAL_INDEX=verify` (the `eval-hooks` build of this branch) over
  the gold asks at 10^4: 157 of 157 indexed answers equal the whole about's, without the
  table and with it; and of the 65,044 (83,031 with the table) candidates
  the whole about found eligible, none is among the 373,709 (516,093) the
  index set aside as unable to be (`unanswerable_eligible = 0`).
- `quick-a`, `neutral`: real store 124 of 124 questions scored, parity 1.0;
  synth 10^3, 88, parity 1.0; the retrieval and Jev judged corpora hold.
- `retrieval-baseline.sh` (including the shipped-bridge rows),
  `jev-baseline.sh` and `relate-baseline.sh` hold with no refresh.
- A store went this build → v0.27.1 → this build, every call answered (the
  sidecar is rebuilt each time its version changes).

**Sidecar upgrade.** A sidecar written before `lexical-index-6` is emptied
and built again on the next ask (`lexical-sidecar.md`). Under the build lock
(load about 10): 4.2 s at 10^4 and 51 s at 10^5, against 5.2 s and 37 s for
the `-5` build of the same stores in the same run; the first ask after the
upgrade pays it once (4.6 s and 55 s).

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
- **Peak RSS of a long session: inconclusive.** Three replays of the gold
  questions at 10^4 gave 935 → 1,252, 797 → 983 and 1,165 → 1,011 MB
  (different merges of the base, other load on the host); in each the peak
  grows ask by ask on the heavy anchored questions. The single-process
  latency run peaks lower than before (823 → 557 MB at 10^4, 5.1 → 3.9 GB at
  10^5).

## Reads at scale, third pass: the table a user runs with (1.0)

Users run with the lexical bridge table `kmp setup` installs, and with it
an ask at 10^4 still read the whole about. This pass makes the indexed ask
with the shipped table read what an answer can use, fixes a continuation
bug the second pass found, and speeds up wake pages. Base: `integration/v1.0`
at a5e92284 for the timings; the parity below was taken again on 154259ff,
the base last merged (it changes no read with Jev off). Evidence: `artifacts/kmp-1.0-2026-11-20/read-perf-3/`
(maintainer workspace).

**Why the table's ask read everything.** The lead was the language rule of
#942: the table vouches for a focus concept only across two languages. That
is true of the focus, but the second pass's bound already set aside 4,807 of
the 4,839 planned candidates on the measured question. What forced the read
was the bridged rescue: a ranking rescues up to three rejected candidates
that carry a word the table bridged the question to, in any language and
through any word (`bridged_candidates`), so top-k read every carrier of a
bridged word, about 4,800 of them, past the 35 % bound. A stored language
lean could not silence that rescue, so none is stored.

**What changed (answers unchanged except the fix in the first bullet).**

- **Lazy continuations with causal hops (fix).** An ask's mapping joins its
  proof path to the head's evidence (P14), but its JSON joined the path
  again to every evidence item the reading carried, tail included. A hop
  whose relation evidence ranked in the tail changed with the depth read
  (inline `evidence` on one page, `evidence_refs` on the next). A hop a page
  places ahead of the evidence (causal such as `depends_on`, procedural,
  every type but the four below) is paged at every depth, so its cursor no
  longer matched and every continuation past that depth was refused
  `READ_SELECTION_CHANGED`, with or without the lexical index (reproduced on
  80c51e15 and 2ffd4957, the bases this pass started from). Those hops now
  keep the head's join in the JSON and in the typed read-back; a hop placed
  after the evidence (`supports`, `has_evidence`, `records`,
  `contains_entry`, structural) is withheld until a reading carries its whole
  ranking, so it is still joined to every evidence item, as before; a
  reading that carries its whole ranking by construction (the entries cap,
  semantic retrieval, re-ranking) joins every hop to all of it in the
  mapping, as its JSON rendered it. What changes: a hop placed ahead of the
  evidence on a continuation page, where it met an evidence item outside
  the head, which is where the cursor broke. A first version of the fix
  joined every hop to the head only; on the frozen real store it moved one
  working continuation page (one more `supports` hop rendered inline, one
  item fewer on the page), which is why the after-evidence hops keep their
  join.
- **The best carriers of a bridged word.** Top-k reads the carriers by the
  direct score their rows give, at least 16 and down to the fourth that no
  reading can find eligible, and certifies from what every ranking's rescue
  took (`BridgedRescues`): three, each strictly above the best carrier left
  unread (`lexical-sidecar.md`). The focus bound counts the table's
  translations only, and none when the question leans in no language. On
  the measured question with the table: 4,815 candidates read → 108, and the
  ask is answered from the index instead of the whole about.
- **Wake pages.** Reading a projected page back onto the typed response
  rendered and joined all 23,199 hops of a 10^4 wake's proof path to find
  the few the page carried (70–77 ms of every page). Only the hops sharing
  endpoints and type with a carried item are now joined and rendered.

Before (a5e92284) → after, one about, p50 of seven calls (first wake: one
call; write: five), release builds without `eval-hooks`, cores 5–9, back to
back under the build lock, 21 Nov 2026, load average 3–4:

| memories | first wake | wake | ask | time (rewind) | write | peak RSS |
|---:|---:|---:|---:|---:|---:|---:|
| 1,000, shipped table | 71 → 67 ms | 57 → 53 ms | 256 → 258 ms | 12 → 12 ms | 1 → 2 ms | 136 → 136 MB |
| 10,000, shipped table | 695 → 647 ms | 150 → 100 ms | 3,132 → 460 ms | 143 → 143 ms | 5 → 4 ms | 857 → 598 MB |
| 100,000, shipped table | 8.3 → 7.7 s | 8.0 → 7.5 s | 32.2 → 5.0 s | 4.0 → 4.0 s | 3 → 4 ms | 6.4 → 4.4 GB |
| 1,000 | 76 → 73 ms | 58 → 54 ms | 227 → 230 ms | 13 → 13 ms | 2 → 2 ms | 119 → 119 MB |
| 10,000 | 695 → 650 ms | 150 → 101 ms | 346 → 343 ms | 142 → 143 ms | 5 → 3 ms | 546 → 546 MB |
| 100,000 | 8.1 → 7.8 s | 7.9 → 7.3 s | 2.98 → 2.99 s | 4.0 → 4.0 s | 4 → 3 ms | 3.9 → 3.9 GB |

The first wake at 10^3 moves between 70 and 117 ms from run to run on both
binaries.

**Gold questions at 10^4** (148 asks with their first continuation pages,
every second question of the bench's ≤ 10^4 levels), each binary alone
under the build lock: with the shipped table, ask p50 1,003 → 499 ms (502
ms on a second run of the candidate; 991 → 492 ms on bec68380 before the
last merge), p90 3.6 → 2.4 s, all 148 asks 216 → 145 s, asks within 500 ms
24 → 76, first wake p50 707 → 668 ms; without it, ask p50 385 → 385 ms, p90
1.66 → 1.67 s, 148 asks 103.3 → 103.1 s, first wake p50 697 → 652 ms. Peak
RSS of a replaying process is inconclusive, as the second pass found: at
10^5 the same binary peaked at 4.3 GB alone and 6.2–7.4 GB beside another
replay, base and candidate alike.

**Where the first wake goes now (10^4, timers in a throwaway build).** The
kernel read 290 ms (graph load 165, details 21, bundle 92),
`wake_response_rendering` 97 ms, the JSON of the whole packet 165 ms (its
proof path of 23,199 hops alone 109 ms), a clone of the response and its
JSON kept for continuations 40 ms, the projection 40 ms; reading the page
back now a few ms.

**Parity** (against a5e92284; gold at 10^4, `quick-a` and the judged
baselines again against 154259ff).

- Gold calls at 10^4, call by call (148 asks with their first continuation
  pages, the five wakes, the rewind): 206 of 206 equal without the table,
  207 of 207 with the shipped table, and the candidate's two table runs equal
  each other. At 10^5, every 20th question with its first page: 32 of 32
  equal; with the table, every 40th: 15 of 15 equal.
- Whole readings the entries cap carries (`budget.max_entries` 100 and 20,
  every sixth gold ask at 10^4): 100 of 100 equal, with and without the
  table.
- `KMP_LEXICAL_INDEX=verify` (the `eval-hooks` build of this branch) over
  the gold asks at 10^4: 157 of 157 indexed answers equal the whole about's,
  without the table and with it; of the 65,044 (83,031 with the table)
  candidates the whole about found eligible, none is among the 373,709
  (516,968) the index set aside (`unanswerable_eligible = 0`).
- `quick-a` (eval-hooks builds, variant names of equal length): `neutral`;
  real store 124 of 124 questions scored, parity 1.0; synth 10^3, 88,
  parity 1.0; the retrieval and Jev judged corpora hold. The real store's
  abouts sit below the index's size threshold and the gold questions are
  monolingual: the bound on bridged carriers across languages is proved by
  `maxscore_parity_tests` (a store where half the memories say a word in
  Spanish, some leaning in Spanish, some replaced or expired; questions in
  English, with Spanish words, and with no function word at all) and by
  `verify`.
- `retrieval-baseline.sh` (the judged collection, and with the shipped
  table both the judged collection and the stress negatives),
  `jev-baseline.sh` and `relate-baseline.sh` hold with no refresh;
  `shipped_lexical_bridge.rs` passes, and so do the cross-language judge's
  pinned pool and recorded verdicts (`cross_language_judge.rs`).

**Writes, sidecar, round trip.** No index version change: the sidecar stays
`lexical-index-6` and its size is unchanged (39.7 MB at 10^4, 389.3 MB at
10^5); a sidecar the base built is read as it is (first ask 387 ms at 10^4,
3.2 s at 10^5, the same answer). Writes at 10^4 with the table after the
sidecar is built, 30 single-memory writes per binary: p50 3.7 → 2.9 ms, p90
6.0 → 6.0 ms. The first write, the first event either binary appends to the
store copy, which also writes the `held-memory` proof file of #948, takes
16.4 → 16.8 ms; the writes after it 3.6 → 2.9 ms at the median, with one
write of the 30 at 20.1 → 19.0 ms on both binaries (the same position in
both runs). A store went this build → v0.27.1 → this build, every call
answered (v0.27.1 rebuilds its `-4` sidecar, this build its `-6`).

**Targets.**

| target | result |
|---|---|
| 10^4 ask ≤ 500 ms with the shipped table, the measured question | **met**: 460 ms (3,132 before) |
| 10^4 ask ≤ 500 ms with the shipped table, gold p50 | **on the line**: 499 and 502 ms in two runs (1,003 before) |
| nothing slower without the table or at 10^3 | **met** (within run-to-run noise) |
| writes ≤ 10 ms | **met at p50 and p90** (2.9 and 6.0 ms); the first write of a store copy (16.8 ms) and one write in 30 (19 ms) exceed it on both binaries |
| 10^4 first wake ≤ 500 ms | **not met**: 647–650 ms (695 before). The kernel read (about 290 ms) and the mapping (about 100 ms) alone use most of the budget; the JSON of the whole packet (165 ms, its 23,199-hop path 109 ms) is rendered before the page is cut, and its response and JSON are cloned for continuations (40 ms) |
| 10^4 time ≤ 100 ms | **not met**: 143 ms, unchanged. Not attempted beyond what the second pass found: each call filters the kept index's whole bundle by dimension, validates it and reads every entry's proof labels |
| 10^5 with the table | ask 32.2 → 5.0 s, peak RSS 6.4 → 4.4 GB; wake and time unchanged (whole-about reads) |
