Choose the clock that answers the question: occurred = event, observed = when
it was known, ingested = recorded by KMP, validity = when a state held. Do not
substitute a known observation date for an unknown event or validity start.

Use Goto for a position, Near for its neighborhood, and Rewind/Forward to move
through history. Copy the cursor or ref returned by KMP. Keep the same about,
clock and dimensions while finishing one selection.

For the sample R1 observation, use
`kmp_goto {"about":"project:sample","at":{"time":"2026-09-01T09:00:00Z"},"axis":"observed"}`.
For another position or range, consult the linked temporal verb contract before
adding fields; it supplies the sequence, ref and interval forms.
`next_actions` distinguish finishing a page from moving beyond the selection.
Finish pending proof before following the next historical position.

An empty selected interval says nothing about another clock or interval. Labels
and related memories can guide a declared zoom; do not silently widen scope.

More: `guide:kmp-agent:example:four-clocks` and
`guide:kmp-agent:example:dimensional-memberships`.
