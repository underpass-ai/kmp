Use **Inspect** to read one exact ref and its evidence. Use **Trace** to examine
connections. Copy returned refs; naming a foreign ref creates no connection.

Choose one Trace mode:

- Known destinations: `about`, `from`, `to`; destination search options can
  choose directions, alternatives and dimensions.
- Evidence from a seed: `about`, `from`, `search.seek`; each role names a
  relation and direction. Do not combine `seek` with `to` or destination options.

A single `to` string with no search or time options follows every stored
relation with a bounded search from both ends (256 refs, 2048 rows, 128 hops).
A path it returns is the shortest one. When a limit stops it first, `trace`
is empty and `search` names the stop, `direction:"bidirectional"` and the
destination in `unreached_targets`. That is not absence. `search.widen` is an
optional larger search you may run instead; it is not a page continuation.

Set the task's clock and cut explicitly for historical questions. Unknown
clocks, missing obligations and `review_required` survive a successful search.
A route or a compatible group does not establish truth or answer completeness.

**Read selected proof:** set `search.proof:true`. To see descriptors before
loading bodies, also set `proof_refs:[]`. Finish each operation's page actions,
then execute its body expansion calls and join canonical text by ref under one
`proof.manifest_id`. Body actions are new deliveries, not page continuations.
Compare selected refs against fetched refs; no remaining action does not prove
that an arbitrary sequence of named batches read every body. Changed selection
requires a fresh read, never mixing old paths with new bodies.

**Reuse a short view:** after reading a canonical body, use the Condense topic.
Its card is derived orientation, never fetched proof. A fresh compact Trace
uses `proof:true,compact:{language:...}` without named expansion or old cursor.

Expand only the detail you need:

- `guide:kmp-agent:example:reader-cards`: descriptors, byte ceilings, exact
  expansion, Condense and compact reuse, with complete call shapes.
- `guide:kmp-agent:example:evidence-seek`: roles, contextual discovery, identity
  joins and historical evidence.
- `guide:kmp-agent:example:bounded-trace`: destination alternatives, AND/OR
  requirements, hard dimensions and soft focus.
- `guide:kmp-agent:advanced:audit-reference`: Inspect object reuse/receipts,
  pagination, work limits, clocks and detailed search semantics.
