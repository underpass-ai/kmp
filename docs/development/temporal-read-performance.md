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

`TemporalMemoryResult::source_bundle` retains the scoped graph metadata and the
bodies required by its include options. ChronoLoom keeps its existing full
structured read because its projection consumes a different catalogue. Wake
and Ask are unchanged; ranking may require the complete canonical candidate
text, so a size/hash header cannot replace that text.

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

This is the body-materialization portion of #770. The graph catalogue is still
read and temporal coordinates are still sorted before selecting entries.
Indexed temporal page pushdown and recall-header admission remain separate
work; this change neither adds an index nor claims bounded graph discovery.
