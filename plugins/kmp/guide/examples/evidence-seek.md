# Find compatible evidence from a seed

The fictional register contains execution R, report V and permit P. The report
explicitly verifies R; P explicitly authorizes R. Their stored arrows are
R --verified_by--> V and P --authorizes--> R, each with its own why and source
evidence. V and P carry event E1, supported by their documents. Another permit
for E2 is also stored. These memberships describe events, not person identity.
Copy the about and R's ref from a receipt or a temporal read. In the calls below,
`about` and `ref-R` stand for those returned values; never send the placeholders.

```json
{"about":"about","from":"ref-R","search":{
  "seek":["verified_by",{"name":"permission","rel":"authorizes","direction":"incoming"}],
  "same_labels":["event"]}}
```

No destination refs are needed. `seek` requires both roles; paths within each
role are alternatives. The main relation reaches its witness: V for verification,
P for permission. KMP intersects the witnesses' event values jointly. E1/E1 can
form a group; E1/E2 cannot. If P has no event membership, the group requires review,
even though V knows E1. KMP does not fill P's missing label from V.

Read `seek.status`, `missing_roles` and `groups`. `compatible` means compatible
declared obligations, not true or sufficient evidence for any natural-language
question. `ambiguous` retains different known assignments; `review_required`
retains missing witness labels or link clocks. `missing_obligation` means a role
has no complete path in this selection. `incompatible_obligations` means its
individual paths cannot coexist under the requested constraints. `partial` means
work ended before the calculation completed. Never promote an unknown to known.

Finish every `next_actions` page. Append `trace`, `candidates`, then `groups`;
their indexes address complete tables. A candidate's `anchor` is the traversal
start of its main relation, after via/context; `witness` is the other endpoint,
where labels apply. Reference constraints may name either. `nodes` and
`edge_indexes` retain the entire path,
including source proof after that witness. Preserve original arrow, why, evidence
and clocks. Paths outside all groups cannot jointly satisfy this request.

To restrict verification to a documented production environment, replace its
string role with `{"rel":"verified_by","labels":{"env":["prod"]}}`.
This constrains V, not every bridge along the path. Label arrays contain allowed
values; sharing a key requires a common value across all role witnesses. Two
explicitly disjoint constant restrictions on a shared key are rejected as an
inconsistent request. A stored mismatch instead rejects that candidate.

## Ordered routes, identity and deep proof

Suppose a source gives R background B, a separate identity record establishes
B --same_entity_as--> I, and I --authorizes--> R. To require the same canonical
witness for identity and authorization:

```json
{"about":"about","from":"ref-R","search":{
  "seek":[
    {"name":"identity","via":["uses_background"],"rel":"same_entity_as"},
    {"name":"permission","rel":"authorizes","direction":"incoming"}],
  "same_ref":[["identity","permission"]]}}
```

Only an actual evidenced identity path can meet the first role. A shared alias,
an owner relation or the same label cannot substitute for it. If I has further
required source dependencies, add `"after":["depends_on","uses_background"]`
to the identity role. Those moves are ordered, and the witness remains I.
`via` is ordered before the main relation. Each move can be a relation string
(outgoing) or `{rel,direction}`. A role permits at most 1024 total moves. Repeated
relations need distinct role names. `same_ref` groups are disjoint; put all
endpoints that must be equal in one group. A role string means its witness.

Add `as_of:{"time":"2026-09-10T12:00:00Z"},axis:"observed"` to either call
to ask what links and memberships were observed by noon. A later identity link
is absent before its declaration even when both endpoints are old. An undated
link remains unknown. `interval` selects a half-open span instead; never combine
it with `as_of`. A returned `seek.review_context` is an optional new Goto read
focused on the seed at the same explicit cut. It is not a page continuation or
permission to widen the task. Without an instant cut, use the temporal guide to
choose the appropriate context read.

## Work versus result size

Seek shares `max_nodes`, `max_edges`, `max_states` and `max_depth` with Trace.
Use sufficient allowances to compare path correctness; no score prunes paths
because they cost tokens. N includes coordinate refs and E includes their rows.
Storage admission, reconstruction and joint combination still have finite work
ceilings. A cutoff is `partial`, including when some complete groups were found.
Larger allowances start a new selection; pages only finish the selected result.
For controlled deep cases, 4096 nodes, 32768 edges/states and depth 1024 are the
current maxima. These are safety ceilings, not a claim of unbounded production
search or of good performance for all graphs.

Do not mix seek with `to`, `follow`, `relations`, `direction`, `dimensions`,
`prefer_dimensions`, `select` or `paths_per_target`. Use role moves and witness
labels instead. Only work limits are shared. These calls discover declared proof
paths, not a question's missing requirements. Read the actual source bodies with
temporal verbs before relying on their text; Seek returns link proof, not an
automatic source-body batch. The example is a deterministic contract lesson, not
evidence that a separate agent understands the task.

## Discover the intermediate sequence from context

Suppose the seed is an incident note N. N uses background B; a correction C
points to B; C is verified by report V. The reader knows it needs verification,
but has not yet read the sequence leading to it:

```json
{"about":"about","from":"ref-N","as_of":{"time":"2026-09-10T12:00:00Z"},
 "axis":"observed","search":{"seek":[
   {"name":"verification","rel":"verified_by","via":"context"}]}}
```

KMP discovers `N --uses_background--> B <--corrects-- C --verified_by--> V`.
The candidate has `context_hops:2` and witness V. No intermediate relation names
or destination refs were supplied. `after` can still require further ordered
source dependencies; labels and witness-reference constraints still apply at V. Different witnesses
and equal-length prefixes remain alternatives joined against the other roles.

Discovery reads justified non-structural links in either direction inside the
same about and temporal cut. It keeps all minimum-hop prefixes per reachable
origin of the requested main relation. It excludes cycles and longer prefixes
to that same origin, without declaring those longer proofs irrelevant. A deep
chain remains discoverable without spelling out its moves. Work exhaustion
still means partial; this mode can read much more context than explicit via.

`seek.context_discovery:true` and `declared_obligations_complete:false` distinguish
these leads. Compatible discovered groups require review; missing, conflicting,
ambiguous and partial results remain explicit. Read the original source bodies
with the same temporal cut. V verifies C in the stored statement: KMP does not
infer that V verifies N, that every contextual relation composes logically, or
that the seed and C are the same event. `same_labels` constrains named witnesses;
`same_ref` constrains only the named endpoints, not an unbound seed. Require explicit evidence for that
connection when the question needs it. Discovery selects sequences for reading;
the reader still supplies the task's obligations and decides their relevance.

## Verify and authorize the same action

Two actions can share event E1 and the same authorizer. Verification of one
does not verify the other. For contextual roles, equate the traversal starts
of their main relations:

```json
{"about":"about","from":"ref-N","search":{
  "seek":[
    {"name":"verification","rel":"verified_by","via":"context"},
    {"name":"permission","rel":"authorizes","direction":"incoming","via":"context"}],
  "same_ref":[[
    {"role":"verification","at":"anchor"},
    {"role":"permission","at":"anchor"}]]}}
```

For R --verified_by--> V, anchor=R and witness=V. For P --authorizes--> R
traversed incoming, anchor=R and witness=P. Equality requires the exact same R,
not just matching event labels or the same P. A longer route to the authorized
R survives when the short contextual shortcut reaches a different action.
If no R has both relations, KMP returns no compatible group; individual candidates
remain diagnostic. If two actions satisfy the request, their different anchor
assignments remain ambiguous. Contextual groups still require source review.

An advanced identity request can also add the identity role above and keep
`["identity","permission"]` as another same_ref group. This equates their
witness P while the anchor group equates action R. The same role may participate
through its two distinct endpoints. Mix anchor and witness members when the
task requires one role's result to be another role's starting object. Each
endpoint belongs to one equality group; no variables or numeric positions are
needed. Explicit `via` still places the anchor immediately before the main rel;
`after` does not move either endpoint. The response prints `anchor`, `witness`
and binding `endpoints` when an anchor is involved. These are stored identities,
not inferred aliases, validity or approval of a different action.
