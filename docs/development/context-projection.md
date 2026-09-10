# Shared passages in native read results

The first increment of #537 composes existing read packets. It does not select
new evidence or consolidate stored memories. The host opts in with
`KMP_MCP_PASSAGES=shared` (`inline` is the default), or with
`KernelMcpServer::with_shared_passages(true)`. Stdio, including its viewer path,
and HTTP environment setup select the same mode. No input schemas or verbs change.
The initialized host advertises the representation and its read output schemas
include the optional `passages` and `citations` tables. A host embedding the server chooses its
mode explicitly; construction does not read global process state.

A typed prose slot can contain `{"passage":"p1"}`. `passages.p1` in that same
packet holds its complete original text. A graph-reference slot can contain
`{"citation":"c1"}`; `citations.c1` maps it to the exact canonical ref of a record
defined in the packet. Record definitions remain literal. A reference whose
record is absent stays literal too. Source and metadata fields, clocks, lifecycle
states, raw records and read actions are untouched. Two sources sharing p1 remain
two independent attestations. Passage names identify display text; citation names
resolve record refs, never entity equivalence. Cite that canonical ref, and keep
each packet with both of its response-local tables when paging.

Sharing runs after native selection, budgets and contextual guidance. It never
increases a packet. Required/floor byte estimates still describe the native inline
selection; recall used_bytes is recalculated for the actual output. No page is
stored. Empty or unhelpful tables are not emitted. Errors, writes, guide commands
and viewer data keep their usual shapes. Inspecting a stored guide node is still
a memory read and can share repeated text like any other inspection.

## Composition by a host

The reusable API is `kmp_proto_mapping::context_projection`. A `ContextGroup`
contains an id, canonical native packets, and their original explicit native
reads. A host groups a claim with the proof it requires; the composer does not
infer that semantic requirement. Native partial/selection/cap warnings stay
inside those packets. A group can be complete as supplied and still incomplete
against its sources: projection does not certify either assertion.

`compose(groups, max_bytes)` shares identical prose and repeated uses of returned
canonical refs across admitted groups. The context owns one pair of tables.
It first admits all groups if they fit; otherwise it considers whole groups in
input order. It does not optimize a knapsack or shorten a record. Omitted group
ids retain their original read calls and `reason=byte_budget`. A manifest too
large for the budget is returned whole with a warning, rather than deleting the
path back to omitted evidence. A future read still needs normal authorization
and may see a later store state; a local handle is never permission or a snapshot.

`expand` reconstructs admitted original groups. `expand_packet` restores native
shared packets before composition or before an existing consumer processes them.
The example below runs the production composer on captured groups without opening
a store or calling a model:

```bash
cargo run --locked --quiet -p kmp-proto-mapping --example context_projection \
  < captured-groups.json > context.json
cargo run --locked --quiet -p kmp-proto-mapping --example context_projection \
  -- --expand < context.json > admitted-groups.json
```

Input is `{"groups":[{"id":"permission","packets":[...],"reads":[...]}],
"max_bytes":10000}`. Reads are original memory calls with explicit abouts, not
expiring continuations. They are not executed by the composer. Byte admission
has no tokenizer or model-provider dependency; the host measures its tokenizer,
framing and final context independently from native tool traffic.

## Limits and next work

The native adapter shares exact complete literals in known prose slots. It never
interprets opaque metadata or raw JSON. Distinct partly overlapping quotations
remain intact: existing native evidence has no canonical source-span contract
that would justify combining their boundaries. This first increment therefore
does not close #537. Known-span overlap and usable complete proof-group assembly
remain to be settled and validated against the full issue. #538 owns evidence
selection, not this codec.
No compatibility layer or new editorial CI rule is required for this opt-in mode.

When changing the representation, verify round-trip equality, whole-group
admission, explicit omissions, all native read families, unchanged continuation
execution, errors, raw and write receipts. Capture initialized/advertised schemas
with the mode enabled, not only the default catalogue. Update both guides and
measure startup, traffic and composed context separately. Never present synthetic
compression counts as effective host context or billing.
