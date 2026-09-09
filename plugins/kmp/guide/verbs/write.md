## What to write, and what never to write

Write when something is **decided, constrained, or concluded**. Decisions,
constraints, outcomes — each with coordinates and evidence.

Never write transcripts. Memory is not a log of the conversation; it is the
durable shape of the work. A transcript makes later traversal worthless.

Label what you write. Every label is a `(key, value)` membership within its
about. `scope.process` supplies the primary process; `scope.task` and
`scope.episode` supply optional primary memberships. `labels` uses arrays,
even for a single value, and may add further values under any key:

```json
"labels": {
  "alias": ["neb", "Nébula Cache"],
  "component": ["neb"],
  "environment": ["production"]
}
```

Each pair becomes its own coordinate with the memory's clocks. Two aliases
under `alias` are valid, as is `neb` under both `alias` and `component`.
The key distinguishes them. Repeating the exact pair, including a primary
scope membership, is an error. Values may contain spaces, punctuation and
Unicode. The server returns canonical refs: copy them when an exact graph
address is needed. Do not construct refs or infer entity identity from labels.

Read the `labels` catalogue in `kmp_wake` to reuse existing vocabulary.
Responses report labels written and created. A new spelling under the same
key, such as `component=kmp_viewer` when `component=kmp-viewer` exists, is
refused in strict mode or reported in `labels.resembling` in lax mode.
Sharing a value across different keys is valid and produces no resemblance
warning. `options.labels_new: ["component"]` confirms intentional new values
under that key; nothing is silently renamed or merged.

Use `kmp_relabel` to change memberships later. Its `add` and `remove` use the
same arrays. For example, `add: {"alias": ["Nébula Cache", "NC"]}` adds two
aliases, and `remove: {"alias": ["NC"]}` later removes only that membership.
Give the memory's returned `ref` and a source-backed `why`. The kernel refuses
adding an existing pair, removing an absent pair, placing a pair on both
sides, and removing the last label. Other values under the key remain.
Added memberships inherit the memory's clocks; the relabel's actor, receipt
time and rationale are recorded separately. Read back with `kmp_inspect`
and `include.raw` to verify the memberships and evidence.

Filter a facet with `dimensions.selectors`, for example
`{"key":"alias","op":"in","values":["neb"]}`. To require both an alias and
an environment, supply one selector for each key. Shared labels locate
candidates; only justified relations declare that two memories concern the
same entity or event.

Write in the language of the work, and give the memory an English rendering
for search: `current.summary_en`, plain English a reader would ask with, every
number, identifier and acronym kept exactly as written (`v0.7.0`, `#469`,
`kmp-mcp`, `ADR`). `kmp_ask` searches it and never cites it — the citation is
`current.summary`, byte for byte — so an English question reaches a memory
written in Spanish, and jargon (`rollout slipped`) is found by the words
people ask with (`launch postponed`). A strict write requires it when the
memory is not written in English and refuses one that fails the lint: wrong
language, too thin, a copy of the text, a dropped identifier. Fix the summary
the error names; never alter the text to fit it. A cited item that was reached
through its summary says so with `matched_via: summary` and `summary_terms`.

Memories written before summaries existed still owe one. `kmp-mcp summaries
pending [<about>] [--json]` lists them, with the text to render and, for a
summary the lint refuses, what is wrong with it; `kmp-mcp doctor` counts them.
Attach each with `kmp_write_memory`, intent `record_summary`, `current.ref`
and `current.summary_en` only: the text, kind and coordinates are read from
the store and cannot be supplied, no relation is written, and a rendering
that fails the lint is refused with every fault named. Do it once per store;
an exact retry is a no-op.

Normal writes are one call: omit `options.dry_run` or set it to false.
The relation guide documents explicit previews and validation before commit.

Use one `idempotency_key` per logical write. Replaying the same accepted write
returns its success rather than duplicating memory. A `conflict` can also mean
the store advanced before this attempt committed: follow the error message,
rebase, and retry the same logical write with the same key when it says the
conflict is retryable. A key already accepted with different content must not
be reused.

For a relation, read `guide:kmp-agent:advanced:relations`. For event and observation timestamps read `guide:kmp-agent:verb:time`; for English search summaries read `guide:kmp-agent:advanced:summary`. Read the lifecycle and cross-about topics when the write uses those features. These are exact refs in `guide:kmp-agent`; reuse bodies already in context.
