Use **Curate** to find the relations an about is missing and the declared ones
that do not hold, before you rely on its relations or after a batch of writes.
A review reads the selection the way Relate does and writes nothing.

The work is split by what each reader can do. TypeSafe Jev answers only closed
questions: it picks one option from a list or gives a probability. It cannot
write a word. So Jev suggests a type and doubts a reason, and you write every
`why`, every `evidence` and any new label.

1. Review with `mode:"review"`, `about`, and optionally `dimensions` to select
   several abouts, plus `interval`, `axis` and `max_pairs` (default 12, at most 40).
2. `missing` lists pairs of current facts with no declared relation between
   them:
   - `proposed_by:"kernel"` pairs share checkable signals: a rare identifier,
     matching summaries, a proper name or a label. `pairing_why` says which.
   - `proposed_by:"jev"` pairs were chosen by meaning for facts nothing else
     paired. They carry no signal to check.
   - `suggested_rel` is Jev's choice among the relation types a writer may
     declare, or `contradicts` when the two cannot both be true. Across abouts
     only `same_event_as` and `same_entity_as` are offered.
3. `suspect` lists declared relations where `support`, Jev's probability that
   the stored why and evidence hold, is low, or where Jev would choose another
   type. They are reported only: this verb never retracts a relation.
4. Page with the returned `review_token` and `page.cursor`. The review is frozen
   under that token, so later pages call nothing and cannot change. A token
   that has expired asks for a fresh review.
5. To declare items, read both refs with Inspect and decide for yourself. Then
   call `mode:"apply"` with `review_token`, `actor` and `accepted`: each item is
   `{item_id, why, evidence}` and may add `rel`, `confidence` or `reverse`.
   Before anything is written, Jev reads your why and evidence once more:
   - An item it doubts comes back in `curate.doubted`, unwritten. Correct it,
     or send it again with `confirm_doubted:true`. Jev can ask for another look;
     it cannot refuse a write.
   - The rest goes through `kmp_write_memory`'s own plan and neighbourhood
     review. A `needs_review` answer's next action resumes this same apply.
   - Only items whose `from` belongs to `about` are written. Others come back
     in `curate.rejected` with the reason.
   - Written evidence carries `curated_by: kmp_curate` and the Jev model.

Jev is opt-in for each store: put `typesafe.json` beside it
(`{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.13.0","timeout_ms":20000}`)
and set `TYPESAFE_API_KEY`. The text of the selected facts, cut to 2000
characters each, leaves the machine. Without the opt-in the review still returns
kernel pairs, untyped, with a warning. It runs against the embedded store only.
