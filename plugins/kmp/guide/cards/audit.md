Inspect a returned ref with `kmp_inspect {"about":"<about>","ref":"<ref>"}`.
Copy actual abouts/refs from memory results; replace every placeholder below.
Inspect returns the current object, evidence and typed links. A receipt instead
contains the historical accepted command. For past state, use temporal verbs.

Choose ONE Trace mode:

| Need | Arguments | Search options |
| --- | --- | --- |
| Connect known destinations | `from` + `to` | `follow` or `direction`/`relations`, dimensions, `paths_per_target`, `select` |
| Discover evidence without destinations | `from` + `search.seek` | role relations/sense/via/after/labels; optional `same_labels`, `same_ref` |

Work limits and optional `proof:true` are shared search options.
Never combine `seek` with destination options or `to`. A mixed-mode error names
all conflicting fields to remove together; other validation can still fail.

For evidence known by a given instant, set the observed clock explicitly:

```json
{"about":"<about>","from":"<seed-ref>",
 "axis":"observed","as_of":{"time":"2026-09-10T12:00:00Z"},
 "search":{"seek":[
   {"name":"verification","rel":"verified_by","via":"context"},
   {"name":"permission","rel":"authorizes","direction":"incoming","via":"context"}]}}
```

Choose the task's clock and cut; this example date is not a default. Omitted
clock/cut reads the current graph. `as_of` is effectiveness at a cut, not exact
timestamp equality. Use Goto for a purely temporal question; UNKNOWN from Ask
does not diagnose its cause.

Context discovers minimum-hop prefixes through justified links, preserving ties
and stored arrows. Read `context_hops`, candidate `anchor` and `witness`, and
joint `groups`. These are leads: `review_required` and
`declared_obligations_complete:false` require source review. A neighbor's
verification does not verify the seed; work cuts remain partial.

A verified copy and an authorized publication are distinct actions: do not
add anchor equality for that question. Only if the question requires the SAME
action, add `same_ref:[[{"role":"verification","at":"anchor"},{"role":"permission","at":"anchor"}]]`.
Anchor is the main relation's traversal start after context; incoming permission
starts at its stored target action. Witness is the other endpoint. A role string
in `same_ref` names its witness. `same_labels` intersects witness labels, never
establishing identity. On incompatibility, review bindings and the question;
do not remove a required equality just to obtain a group.

Use `search.proof:true` to fetch selected entry bodies and their typed sources
in the same search snapshot. Join `objects`, `supports` and `gaps` across pages.
`proof.complete_groups` describes fetched declared sources; retain missing
obligations, unknown clocks and `seek.review_required`. Materialization shares
N/E with discovery. A body fetched later by Inspect is a new read.

Finish every returned continuation before resolving path indexes or claiming
proof complete. Page completion is distinct from semantic sufficiency. Preserve
source text, link why/evidence and clocks. For the worked cases expand
`guide:kmp-agent:example:evidence-seek`. Known-destination alternatives, material
selection and hard/soft dimensions are in `guide:kmp-agent:example:bounded-trace`.
