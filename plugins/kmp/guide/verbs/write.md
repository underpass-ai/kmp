## What to write, and what never to write

Write when something is **decided, constrained, or concluded**. Decisions,
constraints, outcomes — each with coordinates and evidence.

Never write transcripts. Memory is not a log of the conversation; it is the
durable shape of the work. A transcript makes later traversal worthless.

Label what you write. `scope.process` is the label every write carries;
`scope.task` and `scope.episode` are the two well-known ones; `labels`
names any other facet the about is catalogued by — `component`, `release`,
`customer`, `risk` — as `key: value`. Every label becomes a coordinate with
the same clocks, so a later filter, a `relate` across abouts and a lane in
the viewer can stand on it. Before naming one, read the `labels` catalogue
`kmp_wake` returned and reuse what exists: a filter hides what it misses,
and a value already used under another key is refused, because within an
about a scope id names one label and keeps the kind of its first use. The
response says which labels were written and, after a commit, which were
created; a created label is vocabulary growing, so make sure it was meant.

The kernel asks the same question you should: does one resemble it? A new
label is folded — only case and separators forgiven — and compared with the
catalogue. A strict write that names `component=kmp_viewer` where
`component=kmp-viewer` exists, or `repo=kmp-viewer` where that value already
stands under `component`, is refused, and the error names both labels. A lax
write goes through and says so in `labels.resembling`. When you read the
catalogue and still mean a new label, say so with `options.labels_new:
["component"]`; the kernel leaves that label alone. Nothing is renamed in
silence, and nothing is guessed: the threshold forgives spelling and nothing
else.

A label decided late goes on with `kmp_relabel`, never with a rewrite: give
the memory's `ref`, the pairs to `add` and to `remove`, and a `why` a later
reader can check. The kernel reads what the memory stands in and refuses,
naming it, a label it already stands in, one it does not, a value already used
under another key, and taking its last label off — a memory stands in at
least one, which is where its time lives. The same resemblance check runs
against the catalogue, with the same `options.strict` and
`options.labels_new`. The added label inherits the memory's clocks, so
`kmp_rewind` and `kmp_forward` still find the memory where it was; the change
itself is on the edge it added (`method: kmp_relabel`, your `why`) and in the
event log, with who did it when. Undoing a relabel is another relabel the
other way; nothing is deleted. Read the memory back with `kmp_inspect` and
`include.raw` to see every label it stands in now.

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
