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
4. Declare what you confirm with `mode:"apply"`, `review_token`, `actor` and
   `accepted:[{item_id, why, evidence}]`. The why and evidence are yours. Items
   Jev doubts return unwritten in `curate.doubted`; correct them or resend them
   with `confirm_doubted:true`. `needs_review` resumes through its next action.

Jev is opt-in per store (`typesafe.json` beside it, `TYPESAFE_API_KEY` set).
Without it you get kernel pairs untyped and no audit. The selection's fact
text is sent to TypeSafe. Embedded store only.
Complete calls: `guide:kmp-agent:example:curate-review`.
