# Reading several deep paths without paying for every body twice

A trace over long paths returns the canonical body of every selected entry and
of every evidence source behind it. On a shared source that is the largest
record on the path, and it comes back on every read of every path that leans
on it. This is how to read those paths under a body budget, and how to write a
card so the next read costs less — without ever claiming something the store
cannot back.

Three moves, in order. None of them changes which paths are selected, how they
rank, or which refs are in the proof.

## 1. See what the bodies cost before reading any of them

Ask for the proof table with an empty named expansion. This is a
descriptor-only read: the whole selection, every ref, every support arrow,
every coordinate, and no canonical text at all.

```json
{"tool": "kmp_trace", "arguments": {
  "about": "project:kmp", "from": "project:kmp:decision:cutover",
  "to": "project:kmp:claim:rollback-window",
  "search": {"proof": true, "proof_refs": []}
}}
```

Each object comes back with `body_state: "not_requested"`, its `descriptor`
and its `required_record_bytes`. The response also carries
`proof.manifest_id`: the identity of this selection, computed before any body
was read. Copy it. It is what lets a later call join its bodies to this table.

`proof.delivery.selected_body_bytes` is what the whole selection would cost;
`proof.body_bytes` is what this response actually loaded, which is zero.

## 2. Take the bodies you want, in bound batches

`proof.expand_bodies` is a complete call: the same bound query, the refs still
pending, the manifest, and an allowance equal to the exact record bytes of
that batch. Run it as given.

```json
{"tool": "kmp_trace", "arguments": {
  "about": "project:kmp", "from": "project:kmp:decision:cutover",
  "to": "project:kmp:claim:rollback-window",
  "search": {"proof": true, "proof_refs": ["evidence:project:kmp:incident-log"],
             "expect_selection": "4694d5c4…", "max_body_record_bytes": 8214}
}}
```

Set `max_body_record_bytes` yourself when you want a ceiling of your own. The
response admits a sorted prefix and defers the rest; each deferred object
names `required_record_bytes`, and `proof.delivery` reports the two different
next-step minimums. `rerun_record_bytes` is what rerunning this whole query
would need — it pays for the admitted prefix again. `named_record_bytes` is
what a named batch of that one record needs. They are not the same number and
neither is "raise the budget to the total".

A record larger than your ceiling stays `deferred_budget` with its exact
price. It is never reported missing, and it is never read to be discarded.

If the store moved, the call comes back with
`expansion_refusal.code = "read_selection_changed"`, the expected and actual
manifest, no canonical text, no card text, and nothing of the old selection.
Run the `fresh_read` action it carries and start from the new manifest. The
store keeps only current bodies, so there is no old version to recover.

## 3. Write a card, then read the path compactly

Condense the bodies you had to read. A card stands for one body version, and
the write declares which one, copied from the descriptor — never constructed.

```json
{"tool": "kmp_condense", "arguments": {
  "about": "project:kmp", "ref": "evidence:project:kmp:incident-log",
  "language": "es", "scope": "node_body",
  "card": "Ventana de reversión 30 min; el log del incidente la fija en 14:05Z.",
  "source": {"revision": 3, "record_digest": "sha256:9c3e5a71…"},
  "expect": {"absent": true},
  "actor": "reader-agent"
}}
```

`expect` is compare-and-set on the card, separate from the body. Declare
`absent` for the first card; declare the `card_revision` you read to replace
one. Another reader who got there first is refused with the stored revision
named, so you re-read and write again rather than overwrite.

The write is refused if the body moved under you — the digest, not the public
`content_hash`, decides, because a write can change the text without changing
that token. Sources can be condensed as well as entries, which is the point:
the shared record is the expensive one.

Then read the same paths with `search.compact: {"language": "es"}`. Objects
whose card still describes the stored body come back with
`body_state: "compact"` and the card in `card.text`, tagged as derived. A card
that is stale, absent, written after your historical cut, or in another
language returns its state, its descriptor and its expansion price — and no
prose. Nothing falls back to the canonical record behind your back.

## What a card is not

A card is your derived view. It is not canonical memory, it is not evidence,
and it never closes a proof group: `complete_groups` counts fetched canonical
bodies, so a path read compactly stays incomplete until you expand it. Nothing
verifies that the prose is faithful; the contract fixes only which body
version it answers for, and `scope` admits nothing but that body, so a card
cannot stand on a path, a neighborhood or a clock it never declared.

A compact read is cheaper in canonical bodies, not in everything: support
arrows keep their stored `why` and `evidence`, which is a deliberate residual
cost of this increment. `max_body_record_bytes` bounds detail records, not the
whole response and not memory.

A historical read shows no card written after the instant you stand at, and
authorship is stamped by the kernel, so a card cannot be backdated into an
answer that could not have seen it.
