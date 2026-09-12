# Read canonical evidence, then reuse a short card

Keep the graph query, clock, dimensions and work limits. Body delivery changes
what text you read, not which paths or refs were selected. These four calls
show the workflow. Replace **every `<placeholder>` and illustrative number**
with values from your memory results; digests and manifests are never invented.

## 1. Discover descriptors

```json
{"tool":"kmp_trace","arguments":{
 "about":"<about>","from":"<from-ref>","to":"<to-ref>",
 "search":{"proof":true,"proof_refs":[]}}}
```

Finish this operation's `next_actions` pages. Each selected object with a stored body retains its
ref, descriptor and `required_record_bytes`, but no canonical body text.
Missing bodies remain explicitly missing; they do not acquire descriptors.
Stored relationship `why`/`evidence` remains. Keep `proof.manifest_id` and the
selected-ref table. Inspect and legacy Trace do not provide `record_digest`;
Trace requires `proof:true` plus an active body option for descriptors.

## 2. Read bodies against that manifest

Execute the returned `proof.expand_bodies` call without reconstructing it.
It retains the query and any supplied record-byte ceiling; without a ceiling,
it prices the planned batch. Named batches contain at most 64 distinct refs.
This is an illustrative complete shape:

```json
{"tool":"kmp_trace","arguments":{
 "about":"<about>","from":"<from-ref>","to":"<to-ref>",
 "search":{"proof":true,"proof_refs":["<selected-ref>"],
 "expect_selection":"<copied manifest>","max_body_record_bytes":8214}}}
```

Copy the actual allowance from the action; `8214` is not a default. If you
choose a ceiling yourself, present records are admitted in sorted-prefix order.
An oversized record stays deferred with its exact price. Use the explicit
`expand_oversized_body` action only when that larger read is intended.

Finish **each expansion's pages**, retain canonical bodies by ref under the
same manifest, and compare fetched refs with your selected table. Body plans
advance a suffix: expanding an arbitrary late subset can leave earlier refs
unread even when the last result has no body action. Request any remaining refs
explicitly, bound to the manifest, rather than declaring completion.

If `expansion_refusal.code` is `read_selection_changed`, no old-selection body
or card text is returned. Run its `fresh_read`, then rebuild the table; do not
join different manifests. `unknown_expansion_refs` rejects the whole batch,
including otherwise valid refs. Correct the refs from the selected table.

## 3. Condense the body you actually read

Copy `revision` and `record_digest` from that body's descriptor:

```json
{"tool":"kmp_condense","arguments":{
 "about":"<owning-about>","ref":"<read-ref>","language":"en","actor":"<reader-name>",
 "scope":"node_body","card":"<your faithful phrase about this body only>",
 "source":{"revision":3,"record_digest":"<copied digest>"},
 "expect":{"absent":true}}}
```

Replace `3` with the observed revision. Without `context_id`, `actor` is required;
with a valid context, an omitted actor defaults to the persistent agent name.
Card text must be nonempty, at most
4096 UTF-8 bytes and strictly shorter than its body. Entries and evidence
sources can both be condensed. Do not summarize a path or neighborhood into
one body's card. KMP checks the declared dependency, not the prose's truth.

`expect` guards the card separately: use `absent:true` initially, or the observed
`card_revision` when replacing it. Source changes or concurrent card writes
refuse the operation; read the current body/card state before revising. The
public `content_hash` is not a substitute for the exact `record_digest`.

## 4. Start a fresh compact read

```json
{"tool":"kmp_trace","arguments":{
 "about":"<about>","from":"<from-ref>","to":"<to-ref>",
 "search":{"proof":true,"compact":{"language":"en"}}}}
```

Keep the original graph selection and task clocks. **Remove `proof_refs`,
`expect_selection` and the old page cursor.** Named refs take precedence over
compact and request canonical bodies, even when a card exists.

| Returned state | Meaning |
| --- | --- |
| `loaded` | Canonical body fetched; retain it by ref under the manifest. |
| `not_requested` | Body intentionally unread; not absent evidence. |
| `deferred_budget` | Present record did not fit; exact expansion price supplied. |
| `compact` | Valid derived card in `card.text`; canonical body still unread. |
| `missing` / `missing_body` | Selected body unavailable in the store. |

Absent, stale, post-cut or wrong-language cards give status, descriptor and
price, **no prose and no automatic body fallback**. Expand canonical evidence
explicitly when needed. A card never closes a proof group; fetched groups still
do not prove semantic sufficiency. Preserve missing/unknown/review signals.

## Cost and history

- `required_record_bytes` and `max_body_record_bytes`: stored Details records.
  `rerun_record_bytes` includes the previously admitted prefix;
  `named_record_bytes` prices the single pending record in a named batch.
- `selected_body_bytes`: total selected canonical UTF-8 body bytes.
  `proof.body_bytes`: canonical UTF-8 bytes actually loaded in this operation.
- Neither count means response bytes, tokens or RAM. Relationship prose and
  provenance still cost context. Cards can save repeated large-body reads;
  writing and reusing a small card can cost more overall.

Authorship is kernel-stamped. A historical read never shows a card written
after its cut; a reader cannot backdate new knowledge into past proof.
