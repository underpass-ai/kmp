Choose the clock that answers the question: occurred = event, observed = when
it was known, ingested = recorded by KMP, validity = when a state held. Do not
substitute a known observation date for an unknown event or validity start.
On write, omitted/null `observed_at` uses the exact ingestion instant; unknown
`occurred_at` stays absent. An explicit source observation is preserved. Equal
observation times across a packet are valid; they do not imply equal event times.

Use Goto for a position, Near for its neighborhood, and Rewind/Forward to move
through history. Copy the cursor or ref returned by KMP. Keep the same about,
clock and dimensions while finishing one selection. Goto tests labels as of
that position: a label added later cannot match or appear in its coordinates.

For the sample R1 observation, use
`kmp_goto {"about":"project:sample","at":{"time":"2026-09-01T09:00:00Z"},"axis":"observed"}`.
For another position or range, consult the linked temporal verb contract before
adding fields; it supplies the sequence, ref and interval forms.
`next_actions` distinguish finishing a page from moving beyond the selection.
Finish pending proof before following the next historical position.

`READ_INCOMPLETE` leads the result when selected proof is still being delivered.
For example, `selection.has_more:false` with two entries can coexist with
`page.has_more:true` and seven relations pending. Execute the returned read action;
the two entries alone do not complete that packet. Conversely, a complete page
may offer a new history position because `selection.has_more:true`. That is a
new selection, not a continuation of missing proof. Neither establishes that
the evidence is sufficient for the question.

An empty selected interval says nothing about another clock or interval. Labels
and related memories can guide a declared zoom; do not silently widen scope.

More: `guide:kmp-agent:example:four-clocks` and
`guide:kmp-agent:example:dimensional-memberships`.
