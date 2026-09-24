# Review two abouts' relations, then declare one yourself

Replace **every `<placeholder>`** with values from your own results. Refs,
tokens and cursors are never invented.

## 1. Review

```json
{"tool":"kmp_curate","arguments":{
 "mode":"review","about":"<about>",
 "dimensions":{"scope":"abouts","abouts":["<about>","<other-about>"]},
 "max_pairs":12,"page":{"entries":8}}}
```

The answer carries `missing`, `suspect`, `review_token`, `page` and, when there
is more, a `next_actions` call. An item looks like:

```json
{"item_id":"m0","from":{"ref":"<ref-a>","about":"<about>","excerpt":"…"},
 "to":{"ref":"<ref-b>","about":"<about>","excerpt":"…"},
 "suggested_rel":"supports","proposed_by":"kernel","signals":["identifier"],
 "pairing_why":"both cite #4711","jev":{"confidence":0.82,"top":[["supports",0.82],["causes",0.1],["none",0.08]]}}
```

## 2. Page the same review

```json
{"tool":"kmp_curate","arguments":{
 "mode":"review","about":"<about>","review_token":"<review_token>",
 "page":{"entries":8,"cursor":"<next_cursor>"}}}
```

## 3. Read both memories before deciding

```json
{"tool":"kmp_inspect","arguments":{"about":"<about>","ref":"<ref-a>"}}
```

## 4. Apply it, in your words

```json
{"tool":"kmp_curate","arguments":{
 "mode":"apply","about":"<about>","actor":"<your-name>","review_token":"<review_token>",
 "accepted":[{"item_id":"m0","rel":"supports",
   "why":"<one checkable sentence you wrote>","evidence":"<what in the sources shows it>"}]}}
```

- If Jev doubts your why, the item comes back in `curate.doubted`, unwritten.
  Reread it, then correct it or add `"confirm_doubted":true`.
- A rich relation answers `needs_review` with its neighbourhood. Run the
  returned `next_actions[0]` unchanged: it is this same apply, carrying
  `write_review_token`.
- A `suspect` item is a prompt to reread, not a verdict. Nothing here retracts
  a stored relation.
