Choose the clock that answers the question: occurred = event, observed = when
it was known, ingested = recorded by KMP, validity = when a state held. Do not
substitute a known observation date for an unknown event or validity start.
On write, omitted/null `observed_at` uses the exact ingestion instant; unknown
`occurred_at` stays absent. An explicit source observation is preserved. Equal
observation times across a packet are valid; they do not imply equal event times.
Each semantic member's effective observation also dates its generated links and
evidence support; null clears the root default for all of them.

`kmp_time` takes a `move`: `rewind` reads newest first, strictly before `from`
or within `interval`; `forward` oldest first, strictly after `from` or within
`interval`; `goto` returns the state at `at`; `near` the neighbourhood around
`around`. A cursor takes `time`, `sequence` or `ref`; copy the one KMP returned. Keep the same about,
clock and dimensions while finishing one selection. Goto tests labels as of
that position: a label added later cannot match or appear in its coordinates.

For the sample R1 observation, use
`kmp_time {"about":"project:sample","move":"goto","at":{"time":"2026-09-01T09:00:00Z"},"axis":"observed"}`.
For another position or range, consult the linked temporal verb contract before
adding fields; it supplies the sequence, ref and interval forms.
Execute `next_actions` unchanged: they finish this packet's pages first, then
continue history with the same clock, dimensions and interval.
Finish pending proof before following the next historical position.

`READ_INCOMPLETE` leads the result when selected proof is still being delivered.
For example, `selection.has_more:false` with two entries can coexist with
`page.has_more:true` and seven relations pending. Execute the returned read action;
the two entries alone do not complete that packet. Conversely, a complete page
may offer a new history position because `selection.has_more:true`. That is a
new selection, not a continuation of missing proof. Neither establishes that
the evidence is sufficient for the question.
`page.entries` sizes a page in records (entries and proof items);
`limit.entries` bounds the selected entries.

An empty selected interval says nothing about another clock or interval. Labels
and related memories can guide a declared zoom; do not silently widen scope.

More: `guide:kmp-agent:example:four-clocks` and
`guide:kmp-agent:example:dimensional-memberships`.
