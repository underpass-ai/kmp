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

## Validation and remaining work

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
coarse process CPU, peak RSS and complete response bytes. Allocation counts and
physical I/O are not measured. Raw traces belong in the workspace's `artifacts/`;
throwaway stores belong in `tmp/` and are removed after the run.
