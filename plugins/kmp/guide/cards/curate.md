Curate the relations of an about with Jev as a second reader; you write them.

1. Review. Nothing is written:

```json
{"mode":"review","about":"<about>",
 "dimensions":{"scope":"abouts","abouts":["<about>","<other-about>"]}}
```

2. Read `missing` (pairs nothing declares) and `suspect` (declared relations
   whose why Jev doubts). `suggested_rel` is Jev's choice among fixed options:
   a suggestion, never a reason. Pairs with `proposed_by:"jev"` carry no kernel
   signal, so read both memories before you believe them.
3. Page the same frozen review with `review_token` and `page.cursor` from
   `next_actions`. A new review reads and judges again.
4. To declare a missing relation, read both refs, then write it with
   `kmp_write_memory` `relations[]`, adding your own `why` and `evidence`. Across
   abouts only `same_event_as` or `same_entity_as` apply, with a relate proposal.

Jev is opt-in per store (`typesafe.json` beside it, `TYPESAFE_API_KEY` set).
Without it you get kernel pairs untyped and no audit. The selection's fact
text is sent to TypeSafe. Embedded store only.
Complete calls: `guide:kmp-agent:example:curate-review`.
