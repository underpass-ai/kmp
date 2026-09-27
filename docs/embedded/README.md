# Embedded KMP

Embedded KMP is the default product path. The coding-agent host starts
`kmp-mcp` as a local child process, communicates over stdio and runs the kernel
inside that process. No separate KMP server, database server, account or API
key is required.

## Install

Use the native KMP plugin where available:

```bash
# Codex CLI
codex plugin marketplace add underpass-ai/kmp --ref marketplace
codex plugin add kmp@underpass
```

```text
# Claude Code
/plugin marketplace add underpass-ai/kmp@marketplace
/plugin install kmp@underpass
/kmp:setup
```

For Codex, ask the agent to run `kmp-setup`. Restart the host once after a
setup or plugin update, because a running session keeps the MCP inventory it
started with. Then run `kmp-guide` in Codex or `/kmp:guide` in Claude Code
to synchronize the installed guides into the selected store. This explicit
step writes memory; setup only installs the assets. A fresh store can report
`GUIDE_UNAVAILABLE` until it has matching guides.

The plugin must be the single MCP owner. Do not combine it with standalone
global `mcp_servers.kmp` or retired `mcp_servers.kernel-memory` wiring.

If the platform has no published engine asset, build only the engine with:

```bash
cargo install kmp-mcp --locked
```

Keep the native plugin as the MCP owner and rerun its setup workflow. Detailed
plugin ownership and packaging live in [`plugins/kmp`](../../plugins/kmp/README.md).

## Verify

```bash
kmp-mcp info
kmp-mcp doctor
```

`info` identifies the binary, selected backend, data directory, engine,
durability state and viewer. `doctor` diagnoses the process-local setup. The
plugin's `kmp-doctor` workflow also checks host wiring and duplicate ownership.

`KMP_MCP_BACKEND=fixture` is only for deterministic protocol tests. It stores
nothing and must never be mistaken for a real memory.

## Where memory lives

The data directory is selected in this order:

1. `KMP_MCP_DATA_DIR`, when explicitly set;
2. the selection saved with `kmp-mcp config memory-store`;
3. `.kernel/` at the nearest git root;
4. the per-user local data directory: `$XDG_DATA_HOME/kmp/default` or
   `~/.local/share/kmp/default` on Unix, and `%LOCALAPPDATA%\kmp\default`
   on Windows (with `APPDATA` and `USERPROFILE` fallbacks).

The environment variable stays first because it is the most explicit and most
local thing anyone can say, and tests, baselines and reproduction scripts
depend on it meaning exactly one process. A saved selection then beats
automatic discovery, which is the point of saving one: a workspace with no git
root reaches the memory its operator chose rather than whatever the per-user
default happens to hold.
A valid environment override does not depend on the saved selection being
readable. Config and doctor report an invalid unused selection with a repair;
without the override, an invalid selection still stops startup. A `#` inside
a quoted selection is part of its path, not a comment.

KMP creates SQLite format-4 stores and upgrades format-3 stores on open. Unsupported store formats
are detected and rejected before their bytes are opened, so an upgrade never
substitutes empty SQLite memory for an older store.

To change the store used by a project, point `KMP_MCP_DATA_DIR` at the intended
directory and verify the selection with `kmp-mcp info` before writing. To
change the memory this machine opens when nothing more local applies:

```bash
kmp-mcp config memory-store /absolute/path/to/memory   # save it
kmp-mcp config                                         # saved, effective, and which rule won
kmp-mcp config memory-store --clear                    # back to automatic selection
```

The selection is one line in the user config file, so it survives restarts of
the desktop application without any environment. It starts no host and creates
no store. A directory holding memory this engine cannot open is refused with
the reason and the repair: KMP never migrates, moves, converts or overwrites an
existing store, and the refused directory keeps every byte it had.

SQLite permits multiple local agent hosts to share one store. To recover an
unsupported store, stop its writers and preserve the directory. Use an explicitly
archived compatible exporter to create a portable bundle, then import it into
an empty current store. The recovery runbook defines that external contract.

### The lexical index beside the store

`lexical-index.sqlite3` sits beside `store/` (not inside it: the format gate
refuses files it does not know there). It is a derived index of what `kmp_ask`
reads (DESIGN L6): for each about, every candidate's terms with their counts
(`LexFwd`), postings of each term in blocks of 128 (`LexPost`), document
frequencies (`LexKeyDf`), the about's totals and language signals
(`LexStats`), the clocks of its relations and the words of its texts, under
both readings an ask can take (with the alias terms the anchored gate reads,
and plain for `best_effort`). It records the event of the store's log it has
followed and the derivation that wrote it.

An about is indexed the first time it is asked about. From then on every write
this binary makes is followed at once, and every ask first follows whatever
other writers (older binaries included) appended to the log: the nodes and
edges the new events touched are read again and the difference applied, so
following an event twice changes nothing. A sidecar of another derivation, or
behind a log that was replaced, is emptied and built again. Older binaries
never open it.

An ask of one about at the frontier, with no dimensions and no remote channel
that reads the whole pool (semantic retrieval, re-ranking, the doubt band), is
answered from it: only the memories that carry a word the question weighs (its
own words, their associations in this memory, the words the lexical bridge
finds for them) are read, with their neighbourhood, and ranked against the
whole about as the index describes it; the response is the one reading the
whole about gives, byte for byte. When those memories are more than a third of
the about, the about has fewer than 2,250 entries (never indexed), or the ask is
any other kind, it reads the about as before; `lexical-index.json` beside the
store tunes both limits. It is on by default; `KMP_LEXICAL_INDEX=off` closes it, `shadow` only compares it with
every ask, and `verify` answers both ways and logs whether they agree. It can be
deleted at any time: the next ask builds what it needs again. It is not part of
a bundle.

## How Ask decides

`kmp_ask` reads with the anchored ask gate unless the store opts out. Under
`evidence_or_unknown` and `show_conflicts`, a question that names an
identifier (`C6.4`, `#188`, `v0.7.0`, `corte 10`) is answered only from
memories that name it and state what was asked beside it. Every response says
how it settled in `answer_status` (`answered`, `partial` or `unknown`) and,
when UNKNOWN, why in `unknown_reason`. `partial` is an enumerative question
answered in part: `proof.missing` names the rest in the question's own words,
and confidence is at most medium.

The gate is the default since 26 September 2026. The previous rule, where
UNKNOWN meant only that the best evidence shared too few of the question's
words, is one file away. Put this `ask-gate.json` in the data directory and
restart the host:

```json
{"mode":"off"}
```

That store then answers byte for byte as v0.23.0 did, without
`answer_status` or `unknown_reason`. `{"mode":"anchored","partial":false}`
keeps the gate and answers UNKNOWN wherever it would have answered PARTIAL.
The engine reports the file in its `kmp_store_config` log line; one it cannot
read or does not recognise is reported and the default applies.

`{"mode":"anchored","confidence_calibration":"shipped"}` states
`proof.confidence` through a versioned calibration table
(`crates/kmp-proto-mapping/language/confidence_calibration.json`): its rules
only move `high` down to `medium`, and nothing else in the answer changes. It
is off by default because `high` has not been certified yet: on 170 labeled
questions the table leaves 20 `high` answers, 19 of them right, and the
one-sided Clopper-Pearson bound (δ = 0.1) is 0.82, short of the 0.95 it must
reach before it is turned on.

### The doubt band (opt-in, sends text to TypeSafe)

A store that already opted into TypeSafe Jev (`typesafe.json`) can ask it
about the asks the gate settled in doubt. Put `ask-judge.json` beside the
store:

```json
{}
```

The defaults are `veto_at` 0.9, `margin_tenths` 20 and `promote` false
(`promote_at` 0.9 when promotion is turned on). They were fixed on the
development corpora before validation and are not tuned per store.

An ask enters the band only when no anchor it names is absent and it is
UNKNOWN without an anchor while a `best_effort` reading would cite something,
`attribute_not_found` or PARTIAL under an anchor, or answered with a first
citation leading the second by less than `margin_tenths` tenths of a BM25
point. One batch of at most eight admitted passages goes to Jev through the
verdict book (`judgements.sqlite3`), with a 1.5 s deadline on the first page;
past it the deterministic answer stands, with a warning, and the verdict lands
in the book for the next ask. A continuation never asks.

- **Veto (B1).** A cited memory Jev finds at least `veto_at` likely not to
  answer leaves the core. It stays in `proof.evidence` with `judged_out` (the
  model), `judged_permille` and `judged_template`. A veto only takes answers
  away.
- **Promotion (B2)**, off unless `"promote": true`: an admitted memory that
  passed the anchored gate, judged at least `promote_at` likely to answer,
  joins the core or answers an UNKNOWN whose anchor was found, marked
  `judged_by`. Confidence stays the words' own and is never `high` for it. It
  never admits an absent anchor or anything outside the selection, and never
  writes text.

`"question": "score"` asks a four-level grade instead of yes/no (an
experiment). With the band on, an ask is a function of the store and the
verdict book, not of the store alone.

### Search expansions at write (opt-in, sends text to TypeSafe)

A writer may propose, per memory, up to six `search_expansions` in
`kmp_write_memory` (`memories[]`, or `search_summaries[]` for a memory that
already exists): questions the memory answers, paraphrases and keys in the
other language (Spanish or English), at most 120 characters each. They are
kept only on a store that opted in with `write-expansions.json` beside
`typesafe.json`:

```json
{}
```

Each expansion is read first by a deterministic lint (it may not name an
identifier the memory does not state, repeat the memory's own words or repeat
another expansion), then by Jev with one yes/no question through the verdict
book. Those Jev reads as belonging at `accept_at` (0.5, fixed on the
development corpora) are stored as metadata bound to the text they were
judged against; the rest are listed in the result as refused. Without the
file, a working Jev or an answer, nothing is stored and the result says why
(`search_expansions.not_stored`). A memories write commits the memories
first and attaches the kept expansions as a second, metadata-only write.

The gRPC write carries the same proposal: `MemoryEntry.search_expansions`
(field 6), optional and additive, so an older client that never sets it
writes exactly as before. Ingest reads it with the same lint and reports what
became of it in `IngestResponse.search_expansions` (field 5,
`SearchExpansionsReport`: `stored`, `refused`, `not_stored`, `judged_by`).
The kernel serves no judge, so over gRPC nothing is stored and `not_stored`
says why, the same outcome `kmp_write_memory` reports on a backend that
cannot judge. More than six expansions for one entry is `INVALID_ARGUMENT`.

Ask searches the expansions as a field of their own and never as the
memory's words. A memory the question reaches only through them comes back
after every memory it reached in its own words, outside the answer core,
marked `reached_by: expansion` with `expansion_terms`, and cites its own
text. An expansion never names an anchor, never answers and never raises
confidence. Readers other than inspect are shown the memory without its
expansions.

Measured (P15, three Jev samples, 27 Sept 2026): on synth 10^3 the gold
memory of 7 of the 24 paraphrase and cross-language questions enters the
first ten proofs (0 without), all of them cross-language; none of the 12
zero-overlap paraphrases. The retrieval case `paraphrase-gap` is reached.
No answer, UNKNOWN or false answer changes on B-real, the hard negatives
(seeds 7, 11, 13) or synth. Jev reads 500 to 900 input tokens per memory
written ($0.00002–0.00004); the writer adds about 60 output tokens per
memory.

## How a review without focus pairs orphans

`kmp_curate` in `review` mode without `focus` asks Jev, for up to 30 facts
of an about that no pair or declared relation touches, which other fact of
the about relates to each. It does so in abouts of at most 120 current facts;
past that the review says so and names the focused review. A store raises
the cap, to at most 512, with a `curate.json` in the data directory:

```json
{"partner_facts": 512}
```

An about of more than 255 facts is then read in windows of at most 240
options and a final choice between their winners. On the judged atlas case
(316 facts) that review costs about 160k Jev tokens instead of 48k and
proposes more pairs of notes written from one template
(`docs/development/jev-evaluation.md`). `"partner_filter"` (`off`,
`rare_term`, `confirm`) is measured but left off. A `curate.json` that
cannot apply is reported and the defaults stand.

## Durability and recovery

- writes are committed durably before success is returned;
- embedded projection is synchronous, so successful writes are immediately
  readable;
- project stores maintain `.kmp/memory.jsonl` as a portable event bundle;
- import only restores into an empty store;
- named snapshots are immutable recovery points;
- format and engine mismatches fail instead of opening an empty neighboring
  store.

The store itself is machine state and is ignored by git. The portable bundle
can be reviewed and committed deliberately; it may contain the evidence that
was written, so treat it with the same confidentiality as the store.

## Viewer and local telemetry

An embedded session attempts to serve a read-only viewer rooted at
`http://127.0.0.1:7317/`. It binds loopback only and prints a random,
per-session capability link. `kmp_view_open` and `kmp_view_get_state` return
the same link, so an agent can hand it over even when its host hides server
output. Opening the link exchanges the capability for an HttpOnly, SameSite
cookie and redirects to the clean URL; requests from other local processes
receive `401`. Set `KMP_VIEWER_ADDR` to a different loopback address or `off`
to disable it.

Logs and the bounded quality journal live inside the data directory. Quality
observations use `telemetry/quality.sqlite3` in WAL mode, so every local host
keeps its Observability Pulse. These are local diagnostics, not remote
telemetry; KMP does not upload memory to Underpass.

The prompt-quality journal covers reads that render a model prompt. Temporal
reads (`goto`, `near`, `forward`, `rewind`) and ChronoLoom projections read the
structured bundle directly; they do not render and tokenize a discarded prompt
to produce journal metrics. Over MCP, `kmp_ask`, `kmp_relate` and `kmp_curate`
project the bundle and never return the prompt, and `kmp_wake` reads only the
rendered sections it projects, so none of them measures a prompt for the
journal. The journal is a passive observer (`QualityMetricsObserver::is_active`
is false): it records the renders that are measured anyway, such as
`kmp_trace` and the `kmp-embedded` API's recall, which returns the rendered
content. A composition that installs an active observer makes those reads
measure again. Their MCP response quality remains computed from
the selected entries and proof. An absent prompt-quality observation is not a
zero-quality read.

Each tool call leaves one `kmp_mcp_tool` line in those logs: counts, durations
and labels, never stored text, a question or an answer. A line also names the
host as it introduced itself (`clientInfo` name and version only; over HTTP,
the host of that session, never another's) and whether the call is a page of
an earlier one. A `kmp_ask` or `kmp_wake` line adds how it came out (answer
status, UNKNOWN reason, stated confidence, whether the anchored gate decided,
and cited passages per `reached_by`) and a keyed fingerprint of its question or
intent; any call with a guidance `context_id` adds one of that id. A
fingerprint is the whole HMAC-SHA256 under `telemetry-salt`, 32 random bytes
created with mode `0600` beside the store on the first wake, ask or write that
succeeds, over the text in Unicode NFC, lower case and single spaces. A refused
call lists the validation codes and field paths its `feedback` named
(`LABELS_REQUIRED@labels`), never their reasons or values.

The salt never enters a log, a bundle or a request, so a fingerprint compares
only within its store. There is no rotate command: delete `telemetry-salt` and
the next successful call creates a new one; fingerprints from before stop being
comparable with those after. `kmp-mcp doctor` says whether the salt exists
(never its bytes) and `kmp-mcp uninstall` removes it with its store. The fields
are listed in `scripts/performance/memory_bench/SCHEMAS.md` (`kmp_mcp_tool`).

## Maintenance commands

Run `kmp-mcp --help` for the live command contract.

| Command | Purpose |
|:--|:--|
| `info`, `doctor`, `config` | Identify the installation, diagnose it and configure memory routing. |
| `export [file] [--about <about>]...`, `import` | Checkpoint all events or exact opaque abouts, then restore a bundle. |
| `snapshot create|list|verify|read|merge` | Create and inspect immutable recovery points. |
| `document <about>` | Render one about as deterministic Markdown. |
| `viewer [addr]` | Serve the viewer without an MCP host session. |
| `uninstall [--store <path> \| --engine <path>] [--apply]` | Preview the whole installation, one exact store or one exact engine; the preview marks pieces a live host still holds, apply refuses live owners, and it runs only when explicitly requested. |

The MCP tools are a separate surface advertised by the live `tools/list`
catalogue; CLI verbs and model tools are not interchangeable inventories.

`--about` is repeatable and matches `root_node_id` byte-for-byte. A requested
about with no events fails before the destination is created. Filtered bundles
are complete format-3 bundles (`bundle_format: 3`, `event_format: 2` for memory-only histories or `3` with authored card events): their header names only the included abouts and
its count, digest and range cover only the filtered payload. `event_range` is
bundle-local, so filtered payload positions are renumbered from one; aggregate
revisions and refs are preserved.

## Limits

Embedded KMP shares memory between local processes that can access the same
directory. It is not a network service and does not provide remote identity,
authorization, high availability or centralized operations. Use
[Enterprise KMP](../enterprise/README.md) only when those are actual
requirements.

## Implementation authority

- [`crates/kmp-mcp`](../../crates/kmp-mcp/)
- [`crates/kmp-embedded`](../../crates/kmp-embedded/)
- [`crates/kmp-adapter-embedded`](../../crates/kmp-adapter-embedded/)
- [`crates/kmp-viewer`](../../crates/kmp-viewer/)
- [`plugins/kmp/capabilities.json`](../../plugins/kmp/capabilities.json)


### Authored card history

`kmp_condense` appends a complete card revision to the `node_cards` event
stream for its about. The source version checks, compare-and-set, event append,
and card projections share one SQLite transaction. A rejected write appends
nothing. Canonical bodies, their revisions, relations and proof stay unchanged.

Rebuild, full export/import, about-filtered export and named snapshots retain
card text, language, author, authorship time, source identity and card revision.
A historical compact read selects the latest authored card at or before its
cutoff using an indexed lookup, then applies the usual source-digest policy.
A card that predates the cutoff can still be stale for the selected body.
The event history records authorship; it does not certify the prose as evidence.

The first open with card-event support adopts surviving pre-event cards as
`BASELINE` events in one transaction. A migration receipt makes later opens
constant-cost. No earlier overwritten card versions are manufactured. On a
project store, export this extended history to its maintained `.kmp/memory.jsonl`
before resuming guarded writes if the committed bundle is now behind; the
bundle guard still refuses a stale checkout. New successful MCP Condense writes
publish the maintained bundle before reporting success.

The SQLite layout is format 4; upgrading format 3 retains the existing SQLite
file. Portable event format 3 identifies bundles
containing card history; memory-only exports retain event format 2. The current
reader accepts both. Upgrade every writer and recovery tool before using card
history; older engines reject the upgraded format stamp. Stop already-running
older processes before upgrading a shared store. See
[the card history design](../architecture/authored-card-history.md).
