A rich link, local ones included, answers `status:needs_review` with
`accepted:false`: nothing is written yet. Review the returned neighbourhood of
stored memories; the review is yours, not a request for human approval. Then
send the returned `continuation` alone (or with the same `about`/`actor`), or
correct the proposal and send it again. Resuming rechecks the context before
commit, and context that changed meanwhile refreshes the review instead of
writing. Never invent a date, alias or proof to pass.

`read_context` records which stored refs you read before choosing a relation:
an audit you assert, which KMP cannot verify. `review_token` binds a proposal to
the context it was reviewed against; prefer the continuation, and omit the
token on a new or corrected proposal.

More: `guide:kmp-agent:example:first-decision`.
