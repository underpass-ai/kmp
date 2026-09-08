## Reading across projects

Abouts are **not** joined by relations, and that is the design rather than a
gap. An edge between two projects would bake the link into the graph: anyone
traversing MADE would drag KMP along whether they wanted it or not, and the
frontier an about exists to bound would stop being bounded.

There is exactly one exception, and it is a declaration with proof rather
than a join: `same_event_as` or `same_entity_as`, written with
`kmp_write_memory` to a ref of another about, class `evidential`, `why`,
`evidence`, and the proposal `kmp_relate` returned in
`read_context.relate_proposals`. The edge lives in the about that writes it;
the other about does not change. `kmp_relate` then shows it as declared,
`kmp_ask` cites through it across abouts the way it cites a declared
restatement (`restated_from`, `restated_via: same_event_as`), and
`kmp_trace` between the two abouts walks it. Raw `kmp_ingest` never crosses
an about, and any other relation pointing outside it is refused.

So the join lives with the reader, at read time, and `dimensions.scope` is
where you make it:

```json
{
  "about": "project:made",
  "question": "Why does the store conversion copy rows instead of replaying?",
  "dimensions": { "scope": "abouts", "abouts": ["project:made", "project:kmp"] }
}
```

One call, both projects, and the evidence comes back attributed to each — the
ADR-018 reasoning that lives in `project:kmp` delivered into a conversation
about MADE, without either project's graph growing an edge it did not ask for.

Reach for this whenever a decision made in one project governs work in
another: a shared contract, an ADR that both sides implement, a constraint one
repository imposes on its sibling.

Scope is auditable, never implicit:

- omitted → `current_about`
- `abouts` → requires a non-empty list
- `all_abouts` → traverses every memory anchor, explicitly; with
  `mode: only` and `include`, only the anchors that carry a dimension of that
  kind, and with `scope_ids`, only those that carry that exact scope

The proof says which abouts were read: `proof.abouts_selected`, the current
one first. Each cited ref carries its about already; the list is what makes
silence from an about distinguishable from never having looked. Bounded by
`as_of` or `interval`, `proof.abouts_empty_in_selection` names the abouts
read that had nothing inside the span or in effect at the instant.

Widen deliberately. `all_abouts` on a large store is a real cost, and an
unscoped sweep buries the answer you wanted — but scoping to the two or three
abouts that actually bear on the question costs almost nothing and is the
whole point of the mechanism.

### Selectors: filter by what an entry is catalogued as

`dimensions.selectors` reads the labels an entry stands in as a map from
key to values and keeps the entries that satisfy every predicate:

```json
{
  "about": "project:kmp",
  "question": "What did the north outage teach about the store?",
  "dimensions": {
    "selectors": [
      { "key": "incident", "op": "in", "values": ["north-outage"] },
      { "key": "task", "op": "notexists" }
    ]
  }
}
```

Four operators: `in` (one of the entry's values under `key` is listed),
`notin` (none is — and an entry without the key passes, as in Kubernetes),
`exists`, `notexists` (the key is present or absent; `values` stays empty).
Values are the bare label values `kmp_wake` lists in `labels`; a namespaced
scope id is read as its bare value.

This is a different grain from `mode`, `include`, `exclude` and
`scope_ids`, which read one coordinate at a time and keep an entry when
*one* of its coordinates passes. `exclude: ["task"]` keeps every entry that
also stands in a process, because the process coordinate passes;
`{ "key": "task", "op": "notexists" }` keeps only the entries with no task
label at all. The two are not duals. Reach for `exclude` to drop a lane
from a picture; reach for a selector when the question is about what an
entry is catalogued as. Both are hard filters: what they hide is invisible,
never ranked low, so read the catalogue first.

With `scope: all_abouts`, `exists` and `in` narrow the anchors through the
index the way `include` and `scope_ids` do; `notin` and `notexists` cannot
name anything to pick by, so alone they sweep every anchor and exclude
afterwards.

One thing a selector is not for. A `sequence` is a counter **per label**:
`sequence: 3` in `task=a` and `sequence: 3` in `task=b` are two different
places. A sequence cursor over more than one label still moves, so page
continuations keep working, but the order it walks across labels is not one
you defined; the temporal verbs say so in `warnings`, naming the labels.
Pin one with `scope_ids` — adding `mode: only` and `include` when the same
value serves two kinds in the about — and the order is defined. A selector
does not pin it: it keeps whole entries, and an entry's other coordinates
keep their own counters.
