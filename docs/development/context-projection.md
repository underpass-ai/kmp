# Shared passages in reads and composed contexts

This projection composes existing read packets. It does not select
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
contains an id, native packets, and their original explicit native
reads. A host groups a claim with the proof it requires; the composer does not
infer that semantic requirement. Native partial/selection/cap warnings stay
inside those packets. A group can be complete as supplied and still incomplete
against its sources: projection does not certify either assertion.

`compose(groups, max_bytes)` first expands each packet's own tables independently.
Equal p1/c1 names from different pages cannot collide. It shares identical prose and repeated uses of returned
canonical refs across admitted groups. The context owns one pair of tables.
It first admits all groups if they fit; otherwise it considers whole groups in
input order. It does not optimize a knapsack or shorten a record. Omitted group
ids retain their original read calls and `reason=byte_budget`. A manifest too
large for the budget is returned whole with a warning, rather than deleting the
path back to omitted evidence. A future read still needs normal authorization
and may see a later store state; a local handle is never permission or a snapshot.

`expand` reconstructs admitted groups with canonical inline packets, including
their declared spans. `expand_packet` restores one native shared packet for an
existing consumer. The installed CLI runs the same production composer on captured
groups without opening a store, executing their reads or calling a model:

```bash
kmp-mcp context project captured-groups.json > context.json
kmp-mcp context expand context.json > admitted-groups.json
```

Input is `{"groups":[{"id":"permission","packets":[...],"reads":[...]}],
"max_bytes":10000}`. Omit the file, or use `-`, to read stdin. JSON goes to stdout;
invalid input goes to stderr with exit code 2. Omitted max_bytes is unbounded;
an invalid type, negative budget or unknown request field is rejected.
Reads are original memory calls with explicit abouts, not
expiring continuations. They are not executed by the composer. Byte admission
has no tokenizer or model-provider dependency; the host measures its tokenizer,
framing and final context independently from native tool traffic.

## Known source spans

An optional `spans` list on a group binds a whole prose slot to a returned source:

```json
{
  "packet": 0,
  "pointer": "/evidence/0/text",
  "source_ref": "source:permit-A",
  "source_sha256": "<lowercase SHA-256 of the complete source text>",
  "start_utf8": 0,
  "end_utf8": 56
}
```

The packet index is local to the group; pointer is a JSON pointer into a known
text, why or evidence slot. The source must be a returned record definition
(`object`, `entries`, `facts`, `evidence` or `proof.evidence`) matching both ref
and text fingerprint. Bounds are half-open UTF-8 bytes of that exact text,
not characters or tokens. The entire quote must equal that slice. Wrong refs,
hashes, bounds, duplicate slot bindings and opaque metadata paths are rejected.
The host or extraction process supplies known offsets; a writer need not compute
them. Literal validation does not authenticate the host's attribution or create
a provenance relation. Keep the actual source and relation evidence.

When beneficial, the composer partitions the returned source at declared bounds.
A slot becomes `{"passage":["p1","p2"]}`: concatenate those table literals **in
order, without adding separators**. One fragment uses `{"passage":"p1"}`.
The full source and its overlapping quotes reuse the same fragments. Independent
records with equal text stay independent; their refs, authors and support survive.
The composer compares this representation with exact-only sharing and uses the
smaller byte representation. Short quotes often stay inline because the tables
and bindings cost more than the repeated words.

Dependency records returned by temporal `include.dependencies` are source
definitions too: `/proof/entries/*/text` participates in exact sharing and
checked source spans. Their canonical refs can shorten repeated relation and
support uses when the definition is present. `proof.groups.seed_ref` and
`member_refs` stay literal; group counts, clocks, sources and metadata remain
unchanged. Keep the complete native page sequence in its `ContextGroup`.
Composing a packet that is still partial does not complete its proof.

If budget admission omits the source group, its full text cannot reappear through
the passage table. The retained quote stays whole and can still use ordinary
exact sharing. Its source binding remains declared and the omission manifest
retains the source's original reads. Expansion reconstructs what was admitted;
it does not retrieve missing sources or certify that a proof is complete.

The following small fixture builds two overlapping quotes programmatically. Its
refs illustrate the format; a real host copies actual refs and calls from its
captured results. The short text demonstrates round-trip fidelity, not savings.

```python
import hashlib, json
source = "Only R7 is permitted. Local use only. R8 is excluded."
def group(ref, text):
    return {"id": ref, "packets": [{"object": {"ref": ref, "text": text}}],
            "reads": [{"tool": "kmp_inspect", "arguments": {
                "about": "project:example", "ref": ref}}]}
permission = group("source:permit-A", source)
quotes = group("claim:plan", "The plan uses permit A; completion is unknown.")
quotes["packets"][0]["evidence"] = []
quotes["spans"] = []
for i, quote in enumerate([
    "Only R7 is permitted. Local use only.",
    "Local use only. R8 is excluded."
]):
    start = source.encode().index(quote.encode())  # known fixture attribution
    quotes["packets"][0]["evidence"].append({
        "id": f"quote:{i}", "text": quote, "source": "source:permit-A",
        "supports": ["claim:plan"]})
    quotes["spans"].append({"packet": 0, "pointer": f"/evidence/{i}/text",
        "source_ref": "source:permit-A",
        "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "start_utf8": start, "end_utf8": start + len(quote.encode())})
print(json.dumps({"groups": [permission, quotes], "max_bytes": 10000}))
```

## Version and limits

The composed envelope is `kmp.context.passages.v2`. Expansion rejects other
versions explicitly; there is no v1 adapter. Use the frozen v1 binary to inspect
old measurement artifacts. Native single-packet sharing retains its existing
string passage names and output schemas; source spans belong to composition.

The native adapter shares exact complete literals in known prose slots. It never
interprets opaque metadata or raw JSON. Distinct partly overlapping quotations
remain intact unless the host supplies the checked bindings above: existing
native evidence has no canonical source-span contract. #538 owns evidence
selection and complete proof-group discovery, not this codec. A host must first
navigate, audit and declare its groups; the composer is a separate optional step.
No compatibility layer or new editorial CI rule is required for this opt-in mode.

When changing the representation, verify round-trip equality, whole-group
admission, explicit omissions, all native read families, unchanged continuation
execution, errors, raw and write receipts. Capture initialized/advertised schemas
with the mode enabled, not only the default catalogue. Update both guides and
measure startup, traffic and composed context separately. Never present synthetic
compression counts as effective host context or billing.
