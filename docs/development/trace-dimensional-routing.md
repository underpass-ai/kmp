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
