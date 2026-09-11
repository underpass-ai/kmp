# Dimensional admission and fair focused discovery

Next increment after PR705. Reuse DimensionSelection, LabelSelector and
EntryLabels rather than invent a label language. Add optional search.dimensions
(hard membership constraints on source and every path entry) and
search.prefer_dimensions (soft ordering, never membership exclusion). Both use
the existing shape within current_about. Missing clocks never borrow another
axis: construct labels only from coordinates admitted on the selected window.
The ref cut resolves independently before dimensional admission. No new writer
fields, no whole-graph histogram or body download.

Focused order is a frozen first model, not fitted coefficients:
choose maximal (matches_preference, -depth, -discovery_index) for three pops,
then the oldest queued state for one pop. This is deterministic under a fixed
snapshot and relation order. Keep every admitted state up to the existing S
limit; no beam pruning of discovery. The material selector remains M2. Preference
can take a longer path first, so it must not promise shortest routes; ordinary
calls retain BFS. With alternatives, states remain simple paths. One-path mode
retains first-visited semantics, with no global optimality guarantee.

The reserved FIFO turn makes nonpreferred bridges eligible for expansion, not a
guarantee under finite N/E/D/S. A full root adjacency may consume the work budget
before the queued states are expanded: preserve and measure this known limit,
then use it to decide whether resumable page-level exploration is justified.
Do not hide this with a fixture that has only helpful high-scoring labels.

Coordinate reads, including excluded labels/endpoints, consume existing N/E.
Reuse the same admission cache and ReadTx for time, labels and graph. Report
routing work and selected-route preference counts; they describe selection,
not correctness. Default calls must retain byte-equivalent output and work.

Controls must include a helpful focus, a misleading focus with a necessary
unlabelled bridge, temporal changes to memberships, multiple values and negative
selectors, infeasible hard constraints, a dense root, and 100-hop evidence.
Compare BFS, hard-filter-only and soft focus on new fixed inputs, and report all
read work plus full serialized context, not only target coverage. Same refs and
links mean the same proof; a temporal label must not steer an earlier search.
No parameters are tuned after inspecting these controls. Human Cala review still
gates independent readers; this increment uses only investigator controls.

## Native evidence, 2026-09-11

Frozen source 0f00ae31, binary SHA256
fc50f0f22ea814bb418d5e36c581b2bd73ec5bed28d056ce8efc551dd67e742e.
Primary run `trace-dimensions-20260911-v1`: 133 RPCs, 19 expected rejections,
zero API; five new registers, one warmup and five rotating measured triplets.
The supplemental `trace-dimensions-expansion-20260911-v1` has five RPCs and
checks an incomplete AND whose full expansion preserves BOTH hard dimensions
and soft preferences. No parameter tuning or engine changes after freezing.

| Control | BFS / hard / focus complete requirement | Focus N / E | Focus median ms |
| --- | --- | --- | ---: |
| Helpful hint | 0 / 1 / 1 | 7 / 13 | 3.28 |
| Misleading hint, necessary bridge | 1 / 0 / 1 | 8 / 15 | 1.76 |
| 100-hop AND, N limit 180 | 0 / 1 / 1 | 142 / 382 | 27.06 |
| Dense root, N limit 16 | 0 / 0 / 0 | 16 / 15 | 1.48 |
| Future membership at observed cut | 1 / 0 / 1 | 4 / 5 | 2.33 |

Hard filters change the admitted problem; their zeros are not engine errors.
Focus preserves the nonpreferred bridge with one FIFO pop. Deep focus uses 33
FIFO pops and preserves the 100-hop route, check and rule, with material 103.
BFS hits N180 and retains no complete AND; its 13.24ms/497-token output therefore
cannot be compared with focus's 27.06ms/13,197-token complete proof as equal work.
Focus adds 243 coordinate rows, included in E382. The bridge also costs more
than BFS: E4→15, JSON504→550, median1.35→1.76ms. It is optional, not a universal
speed or compression improvement. Current labels added after a page invalidate
that focused cursor while a new read preserves exact selected relation evidence.

831 focused Rust tests, workspace Clippy, guide/probes, surface parity and
capability inventory pass. The reviewed Trace definition grows 2,442→3,296 JSON
tokens; costs are informative. Operational evidence, full response metrics and
CSV/PNG/SVG/PDF live in kmp-eval `reports/TRACE-DIMENSIONS-20260911.md`.
Tokenization is sorted compact JSON with tiktoken0.14.0/o200k_base, not host
billing. Latency is exploratory on one local device; no Cargo during timing.

Dense-root failure is retained. Next compare page-level resumable exploration
against this base so an early discovered useful child can expand before the
parent's full adjacency consumes N/E. Preserve costs of label reads and shared
adjacency under alternatives. Histograms, calibrated type/role scoring, unknown
targets and graph/body snapshot work remain pending. Do not make preference
mandatory or quietly change hard constraints to pass a recovery control.
