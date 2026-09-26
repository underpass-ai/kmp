# memory_bench contracts (v1)

The data contracts every part of the KMP memory bench reads and writes. The
bench specification (BENCH_SPEC) says what the bench decides; this file says
what its files look like. Code in `domain/` implements these contracts, and
`tests/test_schemas_doc.py` validates every example below against that code,
so an example here that stops being valid fails the build.

| Contract | Schema id | Implemented by | Written by | Read by |
|---|---|---|---|---|
| synth world | `kmp.bench.world.v1` | this file (BT13 implements) | BT13 | BT14, BT13 questions |
| question | `kmp.bench.question.v1` | `domain/question.py`, `domain/gold.py`, `domain/question_rules.py` | BT07, BT08, BT13, BT16, BT17 | BT09, BT10, BT15 |
| run manifest | `kmp.bench.run.v1` | `domain/run_manifest.py` | BT15 | BT09, BT10 |
| call record | `kmp.bench.call.v1` | `domain/run_record.py` | BT15 | BT09, BT10, BT11 |
| journey record | `kmp.bench.journey.v1` | `domain/run_record.py` | BT15 | BT10 |
| variant | `kmp.bench.variant.v1` | `domain/variant.py` | humans, BT12 | every runner |
| report | `kmp.bench.report.v1` | this file (BT10 implements) | BT10 | BT10 Markdown, BT20 |
| built store | `kmp.bench.store.v1` | `runtime/store_cache.py` | BT14 (`build`) | every runner (`run_questions.StoreRef`), BT18 |
| write cost | `kmp.bench.write_cost.v1` | `application/write_cost.py` | BT05 | BT10 (`latency_resources`) |
| answer reading, write receipts | — | `domain/refs.py`, `domain/receipt.py` | — | every runner and scorer |
| cache keys | — | `domain/cachekey.py` | all | all |
| cache layout | — | `runtime/layout.py` | all | all |

`python3 -m scripts.performance.memory_bench validate --questions F --variant F --calls F --journeys F`
checks files against these contracts.

## 0. Conventions

- **Versioning.** Every record names its schema (`"schema": "kmp.bench.question.v1"`). A
  change that makes an old record invalid or changes what a field means is a new
  version (`v2`) and a bump of `BENCH_VERSION` in `__init__.py`, which invalidates every
  cache entry. Adding a value to a closed list (a corpus, a type, a reason) leaves
  existing records and their digests untouched: it is a reviewed edit of the tuple in
  code plus a row here, with no version bump. Changing a scoring rule or the driver is a
  `BENCH_VERSION` bump even when no schema changes.
- **Strictness.** Readers refuse unknown fields, duplicate JSON keys, booleans where
  integers are expected and non-finite numbers. Absent and `null` mean the same for an
  optional field; writers always emit every field (with `null`), so records have one
  canonical form.
- **Canonical JSON.** `json.dumps(value, sort_keys=True, ensure_ascii=False,
  separators=(',', ':'), allow_nan=False)`, UTF-8 (`cachekey.canonical_json`). JSONL
  writers emit exactly one canonical object per line; a record's line is the text its
  digest is computed over.
- **Digests.** Bench digests are lowercase SHA-256 hex without a prefix
  (`cachekey.digest(value)` = SHA-256 of the canonical JSON). KMP's own bundle
  `content_digest` keeps its `sha256:` prefix verbatim.
- **Refs.** Gold and records hold canonical refs: what `refs.normalize` returns, a
  port of `strip_prefix` in `crates/kmp-testkit/src/bin/retrieval_kmp_scorecard.rs`
  (lines 334-340 at v0.23.0): strip one leading `entry:`, else one leading `detail:`,
  else keep. A reader judges the memory, not the envelope it arrived in.
- **Reading an answer** (ports of the same scorecard, `domain/refs.py`):
  - retrieved = `refs.evidence_refs(structured)`: `proof.evidence[].id`, normalized, in
    response order (the pool, R@k and nDCG read this);
  - core = `refs.cited_refs(structured)`: `because[].ref`, normalized, sorted unique (the
    "núcleo": citations and core precision read this);
  - UNKNOWN = `answer == "UNKNOWN"`; `proof.missing` lists what was sought and not found.
  A v0.23.0 binary never says PARTIAL; the field that carries PARTIAL arrives with the
  P4 gate, and until then the decision is ANSWER or UNKNOWN.
- **Absent values.** A measured value that could not be measured is `null` with a reason
  in the record's `absent` map — never an invented zero (token_harness' Measurement rule).
- **Privacy.** A record with `"private": true` (and every record of a private corpus)
  never lands inside the repository, `tmp/` included: writers call
  `jsonl.guard_private_path`, and private caches live under
  `$MEMORY_BENCH_PRIVATE_ROOT` (section 7). Reports carry aggregates only.
- **Times.** RFC 3339 in UTC with `Z` (`2026-03-01T10:00:00Z`).

## 1. synth-v1 world (`kmp.bench.world.v1`)

A world is one seed and one topology generated up to its highest level. It lives in
`tmp/memory-bench/worlds/<world_key>/` (section 6 for the key):

```text
manifest.json      kmp.bench.world.v1: parameters, counts, file digests, level digests
abouts.jsonl       one about per line
entries.jsonl      one entry per line, ordinal order
relations.jsonl    same-about relations, ingested with kmp_ingest
writes.jsonl       cross-about links, written with kmp_write_memory
questions.jsonl    kmp.bench.question.v1, corpus "synth-v1", each with min_level
```

### 1.1 Ladder, blocks and stable refs

- Entries are generated in **blocks** of `block_size` (1000) entries. Block `k` is a
  function of `(seed, topology, k)` only — a counted SplitMix64 stream per block — so
  the world generated up to 10^4 is exactly the first ten blocks of the world generated
  up to 10^5 (the nested ladder).
- Level `L` holds the entries with `block < L / block_size`, the relations and writes
  whose `block` (the later endpoint's block) is below that bound, and the questions
  with `min_level <= L`.
- **Abouts:** `synth:mono-a000` for `mono`; `synth:multi-a000` … `synth:multi-aNNN` for
  `multi` (Zipf s≈1.1 over abouts). The about list is fixed per topology and level-free.
- **Entry refs:** `<about>:e<ordinal, 7 digits>`, e.g. `synth:mono-a000:e0000412`. The
  ordinal is global (`block * block_size + index`) and never changes as the ladder
  grows. Refs are opaque on purpose: since #875 `kmp_ask` searches a ref's slug, and a
  slug like `e0000412` carries no word a question could match.
- **Relation ids:** `r<7 digits>`; **write ids:** `w<7 digits>`; both in generation order.

### 1.2 Records

`abouts.jsonl`:

```json world-about
{"about": "synth:mono-a000", "title": "Synthetic journal a000",
 "dimensions": [{"id": "work:main", "kind": "work"}]}
```

`entries.jsonl` — `truth` is generator-internal ground truth that drives questions,
invariants and calibration; it is never sent to KMP:

```json world-entry
{"ref": "synth:mono-a000:e0000412", "about": "synth:mono-a000", "ordinal": 412, "block": 0,
 "kind": "decision",
 "text": "C6.4 moves the shared cache of svc-017 to Valkey after the latency review.",
 "coordinates": [{"dimension": "work", "scope_id": "work:main",
                  "occurred_at": "2026-01-01T06:52:00Z", "valid_from": "2026-01-01T06:52:00Z",
                  "sequence": 413}],
 "metadata": {},
 "truth": {"subject": "svc-017", "attribute": "cache-engine", "value": "valkey",
           "anchors": ["C6.4"], "chain": null, "lang": "en", "paraphrase_of": null,
           "supersedes": null}}
```

Required entry fields: `ref`, `about`, `ordinal`, `block`, `kind`, `text`,
`coordinates` (at least one; each names a dimension its about declares; `sequence`
is `ordinal + 1`), `metadata` (string values only; `summary_en` is KMP's reserved
search summary), `truth` (`subject`, `anchors`, `lang` required).

`truth` is written with every key below (null or `[]` when it does not apply), so an
entry's truth has one canonical form:

| Key | Meaning |
|---|---|
| `subject` | who the entry is about: `svc-017`, `db-004`, a chain host, a paraphrase row id, the shared identifier of an echo |
| `attribute` | the facet it states (`cache-engine`), `event` (chain nodes), a changing attribute (supersessions), `hub`, `note`… |
| `value` | what it states about the subject: the words a reader would quote |
| `anchors` | anchors the entry carries (family entries only) |
| `chain` | chain nodes only: `{id, position, length, declared_prev, mentions_prev}`; `id` is `b<block>-c<n>`, `position` counts from 0 (the root cause), `declared_prev` says whether a relation to the previous node was ingested, `mentions_prev` whether the text names the previous node's identifier |
| `lang` | `en` or `es` |
| `paraphrase_of` | paraphrase entries: the combo (`s03+p07`) of `paraphrase.tsv` rows it renders |
| `supersedes` | the ref this version replaces, whatever the supersession mode |
| `episode` | what generated it: `family`, `chain`, `supersession-declared`, `supersession-valid_until`, `supersession-date_only`, `hub`, `hub_note`, `distractor`, `rare`, `paraphrase`, `crosslang`, `echo`, `filler` |
| `group` | family entries: `g<group>-f<family>` |

The three supersession modes are how a replacement is told to KMP: a `supersedes`
relation, only a `valid_until` on the old version, or only the dates.

`relations.jsonl` — both endpoints in the same about; `block` is the later endpoint's:

```json world-relation
{"id": "r0000031", "about": "synth:mono-a000", "block": 0,
 "from": "synth:mono-a000:e0000731", "to": "synth:mono-a000:e0000412",
 "rel": "supersedes", "class": "evidential",
 "why": "The cache engine of svc-017 was replaced after the second review.",
 "evidence": "Second latency review of svc-017.", "confidence": "high", "clocks": null}
```

`writes.jsonl` — `arguments` is a `kmp_write_memory` call, verbatim (the only way a
link crosses abouts). Both endpoints are already stored, so the link travels in the
tool's `relations` (links between existing memories), never as a `memories` record:

```json world-write
{"id": "w0000002", "block": 3, "about": "synth:multi-a004",
 "arguments": {"about": "synth:multi-a004", "actor": "synth-writer",
               "observed_at": "2026-01-03T10:00:00Z",
               "read_context": {"inspected_refs": ["synth:multi-a004:e0003120"],
                                "relate_proposals": [{"from": "synth:multi-a004:e0003120",
                                                      "to": "synth:multi-a001:e0003007",
                                                      "proposed_by": ["identifier"]}]},
               "options": {"strict": true},
               "relations": [{"from": "synth:multi-a004:e0003120",
                              "to": "synth:multi-a001:e0003007",
                              "rel": "same_event_as", "class": "evidential",
                              "why": "Both record INC-3120.",
                              "evidence": "Identifier INC-3120 in both.",
                              "confidence": "high"}]}}
```

Generator 1.0.0 sent the echo again under `memories` with `connect_to`. Against v0.23.0
that shape is refused without `labels` (`INVALID_LABELS`), and with labels it writes a
second revision of the stored echo (writer metadata, a label coordinate, a new content
hash), so 1.1.0 moved the link to `relations`: the receipt then says both sources are
unchanged (`attachment.unchanged_sources`) and `kmp_inspect` shows revision 1 and the
same content hash before and after. Only `multi` has writes, so only its digests moved.

`manifest.json`:

```json world-manifest
{"schema": "kmp.bench.world.v1", "generator": "synth-v1", "generator_version": "1.0.0",
 "bench_version": "kmp.memory_bench.v1",
 "world_key": "5b0c6f1e3d2a4b8c9e7f6a5d4c3b2a1908f7e6d5c4b3a29180f7e6d5c4b3a291",
 "seed": 7, "topology": "mono", "block_size": 1000, "levels": [1000, 10000, 100000],
 "zipf_s": null, "time_origin": "2026-01-01T00:00:00Z", "abouts": 1,
 "counts": {"entries": 100000, "relations": 33600, "writes": 0, "questions": 912},
 "level_counts": {"1000": {"entries": 1000, "relations": 336, "writes": 0, "questions": 304}},
 "files": {"entries.jsonl": {"bytes": 41230117, "sha256": "…"}},
 "level_digests": {"1000": "…", "10000": "…", "100000": "…"},
 "world_digest": "…", "invariants": {"ok": true, "checked": ["paraphrase_zero_overlap"]},
 "calibration": null}
```

Digests:

- `level_digests[L]` = SHA-256 over, for each section in the order `abouts`, `entries`,
  `relations`, `writes`, `questions`: the section name, `\n`, then the canonical JSON
  line of every record of that section at level `L`, in file order.
- `world_digest` = `level_digests[max(levels)]`. **Nesting invariant:** the level digest
  of `L` read from a larger world equals the `world_digest` of the world generated up to
  `L` with the same seed and topology. `config/world-digests.json` pins them for seed 7.

### 1.3 From a world to KMP calls

For each about, in `abouts.jsonl` order, take its entries at the level in ordinal order,
split them into batches of `B` (the build's batch size, recorded in the store key), and
send one `kmp_ingest` per batch:

```json world-ingest
{"about": "synth:mono-a000",
 "idempotency_key": "synth-v1:5b0c6f1e3d2a4b8c:synth:mono-a000:B1000:0",
 "memory": {"dimensions": [{"id": "work:main", "kind": "work"}],
            "entries": [{"id": "synth:mono-a000:e0000412", "kind": "decision",
                         "text": "C6.4 moves the shared cache of svc-017 to Valkey after the latency review.",
                         "coordinates": [{"dimension": "work", "scope_id": "work:main",
                                          "occurred_at": "2026-01-01T06:52:00Z",
                                          "valid_from": "2026-01-01T06:52:00Z", "sequence": 413}]}],
            "relations": []}}
```

- `idempotency_key` = `synth-v1:<first 16 hex of world_key>:<about>:B<B>:<batch index>`.
- `memory.dimensions` is the about's declaration in its first batch and `[]` afterwards.
- An entry maps to `{id: ref, kind, text, coordinates}` plus `metadata` when non-empty;
  `ordinal`, `block` and `truth` are never sent.
- A relation maps to `{from, to, rel, class, why, evidence, confidence}` plus `clocks`
  when non-null, and travels in the batch holding its later endpoint.
- After every ingest of the level, each write at the level is sent through
  `kmp_write_memory` in `id` order (every v0.23.0 write of this shape asks for review:
  the cross-about `same_event_as` is a rich link). A `needs_review` answer is resolved by executing
  `receipt.review_action(...)` (`next_actions[0]`) verbatim and saying so on stderr — the
  fixture resolving its own review, as `commit_judged_write` in retrieval_kmp_scorecard.rs
  does.
- Every receipt must pass `receipt.is_accepted` (a port of kmp-testkit's
  `WriteReceipt::is_accepted`: never a dry run; `accepted` only as `committed` or
  `replayed`; a canonical ingest answers `memory.read_after_write_ready` instead). A
  refused write stops the build (token_harness' fixture rule: a scenario without its
  world is not run).
- Checked against the v0.23.0 release binary on a disposable store: this call shape is
  accepted, a second batch with `dimensions: []` is accepted, and `kmp_ask` cites
  `entry:synth:mono-a000:e0000731`, which `refs.normalize` turns back into the entry id.
  BT14 loaded 10^3, 10^4 and 10^5 through it (section 7, built stores): every ingest
  and every write receipt accepted, each write after one review round.

### 1.4 Question provenance (synth-v1 `source`)

Every synth-v1 question has `source.kind = "generator"` and `source.rule =
"<type>.v1"`, plus the keys its type needs to be audited (strings only: `source` is a
flat object):

| Key | Types | Meaning |
|---|---|---|
| `neighbour`, `keyword` | `anchor_neighbor_existing` | the sibling anchor (same subject) whose entries do state the asked facet, and that facet's keyword |
| `perturbed_from` | `anchor_absent` | the real anchor the absent one was perturbed from |
| `attribute` | `near_miss_attribute` | the value asked about: stated by another family of the about, never in the anchor's entries |
| `keyword` | `singular_anchored_twin` | keyword of the asked facet, which no family of the subject states |
| `anchor_about` | `cross_about_anchor` | the about where the anchor does live |
| `combo` | `paraphrase_zero_overlap`, `crosslang_*` | the `paraphrase.tsv` row combo of the gold entry |
| `bridged_terms` | `crosslang_*` | space-separated terms the lexical bridge table covers |
| `chain` | `multihop_why_k`, `path_between`, `path_open` | `truth.chain.id` of the chain asked about |
| `undeclared_edges` | `path_between`, `path_open` | comma-separated `from>to` hops with no ingested relation (the reader must infer them); `""` when every hop is declared |
| `future_refs` | `as_of_historical`, `interval_scoped` (known) | comma-separated later versions of the asked value: citing one is a future leak |

Judged repo corpora keep their own seeding (`retrieval_cases.json`: one `kmp_ingest` of
`memory` with `idempotency_key` `judged:<case id>`, then each `memories[]` about, then
`writes[]`), with entry ids controlled by `entries[].id`.

## 2. Question (`kmp.bench.question.v1`)

One JSON object per line of a `questions.jsonl`. Ids are unique per file and match
`[A-Za-z0-9][A-Za-z0-9_.-]{0,127}` (they name trace files).

| Field | Type | Meaning |
|---|---|---|
| `schema` | `"kmp.bench.question.v1"` | |
| `id` | string | stable id |
| `corpus` | enum | `synth-v1`, `b-real`, `hard-negatives`, `identifier-guards`, `retrieval-judged`, `jev-judged`, `relate-judged`, `token-harness`, `longmemeval-s`, `locomo`, `factconsolidation-sh`, `factconsolidation-mh`, `musique`, `2wiki` |
| `type` | enum | section 2.2 |
| `about` | string | the about the call reads from |
| `tool` | enum | `kmp_ask`, `kmp_wake`, `kmp_trace`, `kmp_curate` |
| `question` | string or null | required for `kmp_ask`; for other tools a description, never sent |
| `asked_as` | string or null | `kmp_ask` only: the user's own words when `question` renders them in English |
| `answer_policy` | enum or null | required for `kmp_ask` (`evidence_or_unknown`, `show_conflicts`, `best_effort`); null otherwise |
| `as_of` | `{time}` or `{ref}` or null | exclusive with `interval` |
| `interval` | `{start?, end?}` or null | half-open span |
| `axis` | enum or null | `occurred`, `observed`, `ingested`, `validity`; only with `as_of`/`interval` |
| `dimensions` | object or null | passed verbatim, as `kmp_ask` takes it |
| `arguments` | object | extra arguments merged into the first call (budget, depth, intent, from/to, mode…) |
| `anchors` | list of strings | identifiers the question is about (`#188`, `C6.4`); required for anchored types |
| `gold` | object | section 2.3 |
| `tags` | list of strings | stored sorted; e.g. `retrieval35` marks the 35 original judged cases |
| `private` | bool | true for every question of `b-real`, `hard-negatives`, `identifier-guards`, `locomo` |
| `min_level` | int or null | synth-v1 only: smallest ladder level holding all its gold |
| `source` | flat object or null | provenance: `kind` (`transcript`, `author`, `generator`, `dataset`, `judged_case`), `ref`, `rule`, `rules_sha256`… |

### 2.1 The first call

`Question.first_call(max_bytes=None, default_budget=None)` builds it; runners never
assemble it by hand.

| Tool | Sent from dedicated fields | Accepts | Must be in `arguments` |
|---|---|---|---|
| `kmp_ask` | `about`, `question`, `answer_policy`, `asked_as` | `as_of`, `interval`, `axis`, `dimensions` | — |
| `kmp_wake` | `about` | `as_of`, `interval`, `axis`, `dimensions` | — (`intent`, `role` go here) |
| `kmp_trace` | `about` | `as_of`, `interval`, `axis` | `from`, and `to` or `search` |
| `kmp_curate` | `about` | `interval`, `axis`, `dimensions` | `mode` |

- `arguments` may not repeat a dedicated field, nor `continuation` or `context_id`
  (owned by the runner).
- `max_bytes` sets `budget.max_bytes`, overriding a pinned value, exactly as
  token_harness' `first_arguments` does; that is how the budget sweep (2048, 4096,
  10000) runs. Any other budget key a question pins (`tokens`, `detail`,
  `max_entries`, `depth`) is kept; a mode's default budget applies only to questions
  that pin none.
- After the first call the runner follows `projection.next_action` verbatim until the
  server returns none or `max_calls` (≤256) is reached (token_harness' driver).

### 2.2 Types

BENCH_SPEC section 5, plus `evidence_recall` for section 4.6 (R@k without a reader).
Structural rules enforced by `domain/question_rules.py`:

| Type | Tool | Gold must carry |
|---|---|---|
| `enumerative_anchored` | ask | anchors; shape `enumerative` (≥2 facets) |
| `singular_anchored` | ask | anchors; shape `singular` (1 facet) |
| `anchor_neighbor_existing`, `anchor_absent`, `near_miss_attribute`, `cross_about_anchor` | ask | anchors; `UNKNOWN`, `unknown_reason`, `absent_terms` |
| `singular_anchored_twin` | ask | as above, and shape `singular` |
| `negated_anchor` | ask | anchors; `KNOWN` with `excluded_refs` |
| `identifier_guard`, `rare_identifier` | ask | anchors |
| `lookup_exact`, `paraphrase_zero_overlap`, `crosslang_es_en`, `crosslang_en_es`, `hard_distractor`, `hub_adjacent`, `multi_about`, `evidence_recall` | ask | shape and facets |
| `multihop_why_k` | ask | `chain` (k = number of refs, ≥2) |
| `current_after_supersession` | ask | `current_ref` (one of the answers) and `stale_refs` |
| `as_of_historical` | ask | question `as_of` |
| `interval_scoped` | ask | question `interval` (`nearest_outside` allowed) |
| `path_between` | trace or curate | `path` with `to` |
| `path_open` | trace or curate | `path` without `to` |
| `wake_resume` | wake | `wake_required` |
| `repeat_and_determinism` | any | — (a control: the runner repeats it) |

Every `kmp_ask` question has a `shape` and at least one facet; a `KNOWN` one has at
least one answering entry. Strata a metric is split by live in `tags` as `key:value`:
`bridge:covered`/`bridge:uncovered` (crosslang), `degree:<n>` (hub_adjacent),
`abouts:<n>` (multi_about), `form:short`/`form:long` (hard negatives), `retrieval35`.

### 2.3 Gold

| Field | Type | Meaning |
|---|---|---|
| `answerable` | `KNOWN` or `UNKNOWN` | question-level |
| `shape` | `singular`, `enumerative` or null | labeled by a human for B-real; required for `kmp_ask` |
| `facets[]` | `{name, words, answers, related, answerable_facet}` | per facet asked: entries that answer it, entries of the same subject that do not, whether the store answers it; `words` are the reader's own |
| `wrong_subject` | refs | entries of another subject that may appear in the pool; citing one voids ANSWER |
| `chain` | `{refs, ordered}` or null | multi-hop evidence chain, question side first |
| `path` | object or null | `{from, to, steps, required, accept_edges, accept_alternatives, directed}` |
| `current_ref` / `stale_refs` | ref / refs | temporal: the successor and what it superseded |
| `nearest_outside` | ref or null | interval UNKNOWN: what the proof must name outside the span |
| `unknown_reason` | enum or null | `anchor_not_found`, `attribute_not_found`, `anchor_in_other_about`, `not_in_selection`, `no_evidence` |
| `absent_terms` | strings | what a PARTIAL's `missing` must name to count as abstention (section 6 of the spec) |
| `excluded_refs` | refs | `negated_anchor`: never citable |
| `wake_required` | refs | `wake_resume` obligations |

Invariants: facet names unique; a facet with answers is `answerable_facet: true`; a ref
never both answers and relates to a facet; `wrong_subject`, `excluded_refs` and
`stale_refs` never overlap the answers; an `UNKNOWN` question has no answering facet and
a `KNOWN` one has no `unknown_reason`/`nearest_outside`; `singular` ⇒ one facet,
`enumerative` ⇒ at least two. Set-like ref lists are stored sorted; `chain.refs` and
`path.steps` keep their order. `path.steps`, when given, runs from `from` to `to`, and
`from`/`to` must equal `arguments.from`/`arguments.to` when those are set.

The expected `unknown_reason` is bench vocabulary: BT10 maps whatever a binary reports
onto it (`domain/unknown_reasons.py`: a stated `reason`/`unknown_reason` first; for
v0.23.0 `proof.nearest_outside` -> `not_in_selection`, `missing` "any stored memory
for:" -> `no_evidence`, "stored memory that bears on:" -> unmapped), and a binary that
reports no mappable reason leaves `reason_accuracy` null with a reason
(`reason_mapped_rate` says how many UNKNOWNs could be judged).

B-real labeling (BT07) may keep its own working files (`labeling/gold_schema.json`,
one gold per labeler, the κ computation), but each labeler's gold is exactly the gold
object above, and what reaches a runner is always a `kmp.bench.question.v1` record with
the resolved gold. Scoring never reads a labeling file.

### 2.4 Examples

A synthetic enumerative question over an anchor:

```json question
{"schema": "kmp.bench.question.v1", "id": "synth7-mono-enum-0001", "corpus": "synth-v1",
 "type": "enumerative_anchored", "about": "synth:mono-a000", "tool": "kmp_ask",
 "question": "What do we know about C6.4: the decision, its constraint and its status?",
 "asked_as": null, "answer_policy": "evidence_or_unknown",
 "as_of": null, "interval": null, "axis": null, "dimensions": null, "arguments": {},
 "anchors": ["C6.4"],
 "gold": {"answerable": "KNOWN", "shape": "enumerative",
          "facets": [{"name": "decision", "words": "the decision",
                      "answers": ["synth:mono-a000:e0000412"],
                      "related": ["synth:mono-a000:e0000413"], "answerable_facet": true},
                     {"name": "constraint", "words": "its constraint",
                      "answers": ["synth:mono-a000:e0000415"], "related": [],
                      "answerable_facet": true},
                     {"name": "status", "words": "its status", "answers": [], "related": [],
                      "answerable_facet": false}],
          "wrong_subject": ["synth:mono-a000:e0000388"], "chain": null, "path": null,
          "current_ref": null, "stale_refs": [], "nearest_outside": null,
          "unknown_reason": null, "absent_terms": [], "excluded_refs": [], "wake_required": []},
 "tags": ["ladder"], "private": false, "min_level": 1000,
 "source": {"kind": "generator", "rule": "enumerative_anchored.v1"}}
```

A hard negative from the real store (private; the refs are illustrative):

```json question
{"schema": "kmp.bench.question.v1", "id": "hn-near-miss-0007", "corpus": "hard-negatives",
 "type": "near_miss_attribute", "about": "project:example", "tool": "kmp_ask",
 "question": "Which Kubernetes namespace does #188 deploy to?",
 "answer_policy": "evidence_or_unknown", "anchors": ["#188"],
 "gold": {"answerable": "UNKNOWN", "shape": "singular",
          "facets": [{"name": "namespace", "words": "Kubernetes namespace", "answers": [],
                      "related": ["project:example:entry:decision:issue-188-gates-the-release"],
                      "answerable_facet": false}],
          "unknown_reason": "attribute_not_found", "absent_terms": ["Kubernetes"]},
 "private": true,
 "source": {"kind": "generator", "rule": "near_miss_attribute",
            "rules_sha256": "0000000000000000000000000000000000000000000000000000000000000000"}}
```

A judged repo case (`retrieval_cases.json` `interval-unknown-with-nearest`), keeping the
Rust scorecard's call shape in `arguments`:

```json question
{"schema": "kmp.bench.question.v1", "id": "retrieval-interval-unknown-with-nearest",
 "corpus": "retrieval-judged", "type": "interval_scoped", "about": "project:nearest",
 "tool": "kmp_ask", "question": "Which valve failed during the night shift?",
 "answer_policy": "evidence_or_unknown",
 "interval": {"start": "2026-03-01T00:00:00Z", "end": "2026-04-01T00:00:00Z"},
 "arguments": {"depth": 3, "budget": {"tokens": 2048, "detail": "balanced", "max_entries": 10}},
 "gold": {"answerable": "UNKNOWN", "shape": "singular",
          "facets": [{"name": "judged", "answers": [],
                      "related": ["project:nearest:incident:valve-february"],
                      "answerable_facet": false}],
          "nearest_outside": "project:nearest:incident:valve-february",
          "unknown_reason": "not_in_selection"},
 "tags": ["retrieval35"], "private": false,
 "source": {"kind": "judged_case", "ref": "interval-unknown-with-nearest"}}
```

A path and a resume from `jev_cases.json`:

```json question
{"schema": "kmp.bench.question.v1", "id": "jev-path-b02-b03", "corpus": "jev-judged",
 "type": "path_between", "about": "service:billing", "tool": "kmp_curate",
 "question": "How does the 502 during failover lead to the three-replica decision?",
 "arguments": {"mode": "paths", "from": "service:billing:b02", "to": "service:billing:b03"},
 "gold": {"answerable": "KNOWN",
          "path": {"from": "service:billing:b02", "to": "service:billing:b03",
                   "steps": ["service:billing:b02", "service:billing:b01", "service:billing:b03"],
                   "required": ["service:billing:b01"],
                   "accept_edges": [["service:billing:b02", "service:billing:b01"],
                                    ["service:billing:b01", "service:billing:b03"],
                                    ["service:billing:b02", "service:billing:b16"],
                                    ["service:billing:b16", "service:billing:b03"]],
                   "accept_alternatives": true, "directed": false}},
 "private": false}
```

```json question
{"schema": "kmp.bench.question.v1", "id": "jev-wake-inc-4711", "corpus": "jev-judged",
 "type": "wake_resume", "about": "service:billing", "tool": "kmp_wake",
 "question": "Resume the INC-4711 follow-ups.",
 "arguments": {"intent": "Finish the INC-4711 card outage follow-ups: refunds and the cache redundancy fix."},
 "gold": {"answerable": "KNOWN",
          "wake_required": ["service:billing:b01", "service:billing:b03",
                            "service:billing:b06", "service:pagos:p11"]},
 "private": false}
```

Mapping `retrieval_cases.json` (BT16): `id` → `retrieval-<id>`; `question`,
`answer_policy`, `as_of`, `interval`, `axis`, `dimensions` → the same fields;
`arguments` = `{"depth": 3, "budget": {"tokens": 2048, "detail": "balanced",
"max_entries": 10}}`; `judged` → one facet `judged` with those answers (or, with
`nearest_outside`, an `UNKNOWN` gold naming it); `probes` → `source.probes`; the 35
original cases carry the tag `retrieval35`.

## 3. Run records

### 3.1 Run directory

A run is one variant against one store and one question set in one mode, all samples
and repeats included. Its directory `runs/<run_id>/` (section 7) is **also a
token_harness capture**, flat and sealed by token_harness' `_seal`, so
`python3 -m scripts.performance.token_harness verify|measure --run DIR` works on it
unchanged:

```text
manifest.json                      token_harness manifest: uncompressed_files table,
                                   journeys [{lesson, case_id, budget, exit_code, driver_status}],
                                   binary_sha256, evidence_scope "native_replay"
run.json.gz                        kmp.bench.run.v1
calls.jsonl.gz                     kmp.bench.call.v1, one line per measured tool call
journeys.jsonl.gz                  kmp.bench.journey.v1, one line per journey
<question_id>~s<sample>~r<repeat>.jsonl.gz    token_harness trace of that journey
process-<n>.jsonl.gz               handshake frames of MCP process n (hashed, never a journey)
process-<n>.stderr                 that process's server stderr, store root redacted as <store-root>
```

- The journey name is `run_record.journey_name(question_id, sample, repeat)`; it is the
  manifest's `lesson` and `case_id` is the question id. `~` never appears in a question
  id, so a journey name never collides with the run's own files.
- One MCP process may serve many journeys (warm latency, `process_call_index`); its
  `initialize`/`tools/list` frames go to `process-<n>.jsonl`, and each journey's trace
  holds only that journey's `tools/call` pairs, so token_harness counts a journey's
  tokens without the startup catalogue (amortized startup is reported separately).
- Trace lines are token_harness `TraceRecorder` lines (exact wire lexemes of request and
  response, plus markers such as `session_start`, `session_end`, `timing`, `resources`
  added by BT05), so token counts come from token_harness `measure`.
- `response_path` = `<journey>.jsonl#<n>`: line `n` (0-based, token_harness'
  `event_index`) of that trace, resolved through the manifest (the file is stored gzip).

### 3.2 `run.json` (`kmp.bench.run.v1`)

```json run
{"schema": "kmp.bench.run.v1",
 "run_id": "d69566426316216abb930f49452b8c6d3fdb12cd4a70d82fcc8b3753d6c978e0",
 "key": {"binary_sha256": "1111111111111111111111111111111111111111111111111111111111111111",
         "config_digest": "2222222222222222222222222222222222222222222222222222222222222222",
         "store_key": "3333333333333333333333333333333333333333333333333333333333333333",
         "questions_digest": "4444444444444444444444444444444444444444444444444444444444444444",
         "mode": "quick-a", "bench_version": "kmp.memory_bench.v1", "nonce": null},
 "variant": {"name": "baseline", "path": "scripts/performance/memory_bench/variants/baseline.toml",
             "preregistration_digest": "5555555555555555555555555555555555555555555555555555555555555555",
             "config_digest": "2222222222222222222222222222222222222222222222222222222222222222"},
 "binary": {"sha256": "1111111111111111111111111111111111111111111111111111111111111111",
            "version": "0.23.0", "provenance": {"git_commit": "9ed08b68", "build": "cargo build --release --locked -p kmp-mcp"}},
 "store": {"key": "3333333333333333333333333333333333333333333333333333333333333333",
           "content_digest": "sha256:6666666666666666666666666666666666666666666666666666666666666666",
           "label": "synth-v1 seed 7 mono 1000"},
 "questions": {"digest": "4444444444444444444444444444444444444444444444444444444444444444",
               "count": 304, "by_corpus": {"synth-v1": 304}},
 "driver_version": "kmp.native_driver.v1",
 "encoders": [{"encoding": "o200k_base", "asset_sha256": "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d"},
              {"encoding": "cl100k_base", "asset_sha256": "223921b76ee99bde995b7ff738513eef100fb51d18c93597a113bcffe865b2a7"}],
 "samples": 1, "repeats": 2, "max_calls": 256, "timeout_s": 60.0, "private": false,
 "max_bytes": [4096],
 "machine": {"arch": "aarch64", "cpus": "5-9", "governor": "performance",
             "loadavg": [0.31, 0.22, 0.18], "kernel": "7.0.0", "python": "3.12.3"},
 "order": {"scheme": "ABAB", "block": 10, "position": "A"},
 "started_at": "2026-09-26T08:00:00Z", "ended_at": "2026-09-26T08:04:10Z", "failures": []}
```

Required: `schema`, `run_id`, `key`, `variant`, `binary`, `store`, `questions`,
`driver_version`, `encoders`, `samples`, `repeats`, `max_calls`, `timeout_s`, `private`.
Optional: `machine`, `order`, `isolation`, `started_at`, `ended_at`, `failures`,
`max_bytes`, `notes`. `run.json` is the bench's counterpart of token_harness'
`capture.json` (`kmp.token.capture.v1`) and keeps its names where they mean the same
thing: `binary_sha256` → `binary.sha256`, `build_provenance` → `binary.provenance`,
`driver_version`, `isolation` (the allowlisted child environment, store root redacted
as `<store-root>`), `started_at`, `ended_at`, `failures` (`[{question_id, code,
detail}]`, token_harness' `{case_id, code, detail}`). It has its own name because
token_harness' `oracle` reads `capture.json` as synthetic scenarios.

What `application/run_questions.py` (BT15) writes in the optional objects:

| Field | Content |
|---|---|
| `order.scheme` | `ABAB` when two arms are interleaved, `single` for one arm |
| `order.block` | questions per block: each arm runs a whole block before the other takes its turn |
| `order.arms` | variant names, in position order (`A` first) |
| `order.warmup_journeys` | journeys each process runs first, kept out of the records |
| `order.interleaved` | false when one arm came from the cache and only the other ran |
| `order.position` | `A` or `B`: this arm's place in the order |
| `isolation.env` | token_harness' `allowlisted_env()` of the first process: the child environment without secrets, store root as `<store-root>` |
| `isolation.variant_applied` | false iff a store file was not acknowledged (`VARIANT_NOT_APPLIED` in `failures`) |
| `isolation.store_files` | name → SHA-256 of each store file the runner copied |
| `isolation.processes[]` | one per MCP process: `sample`, `process`, `exit_code`, `startup_ns`, `cpus_allowed`, `effective_store` (`{data_dir, rule, engine}` from the binary's own log), `tool_lines`, `unattributed_judgements`, `malformed_log_lines`, `store_files` (`{name, sha256, status: applied|not_applied, reason}`), `warmup_journeys` |

`run_id` must equal `cachekey.result_key(**key)`; `binary.sha256`,
`variant.config_digest`, `store.key` and `questions.digest` must equal their `key`
fields. `RunManifest.comparability()` returns the anti-drift view (questions digest,
driver version, encoders, bench version); `cachekey.drift(a, b)` names what differs, and
any difference makes a comparison `no_comparable`.

### 3.3 Call record (`kmp.bench.call.v1`)

| Field | Type | Meaning |
|---|---|---|
| `run_id`, `question_id`, `variant`, `sample`, `repeat`, `journey` | | identity |
| `call_index` | 0..255 | position in the journey |
| `process_call_index` | int | position among `tools/call` of the MCP process (warm-up state) |
| `phase` | `first`, `warm`, `repeat` | first tool call of a fresh process; a later one; the same arguments again |
| `binary_sha256` | hex | the binary that answered |
| `tool`, `arguments` | | the exact call sent (a continuation is `{"continuation": ...}`) |
| `status` | `ok`, `tool_error`, `rpc_error`, `transport_error` | a timeout is a `transport_error` (token_harness `TRANSPORT_FAILED`) |
| `rpc_id` | int or null | JSON-RPC id |
| `response_path`, `response_sha256`, `response_bytes` | | where the raw response line lives, its SHA-256 and UTF-8 length |
| `wall_ns` | int | harness wall time from request write to response read |
| `server_ms` / `server_us` | int | `duration_ms` of the `kmp_mcp_tool` log event; µs once BT03 lands |
| `cpu_ns` | int | CPU of every task of the process over the call (`/proc/<pid>/task/*/schedstat`) |
| `rss_peak_kb` | int | `VmHWM` after `clear_refs=5` before the call |
| `io` | object | `/proc/<pid>/io` deltas: `rchar`, `wchar`, `syscr`, `syscw`, `read_bytes`, `write_bytes`, `cancelled_write_bytes` |
| `jev` | `{evaluations: [...]}` or null | `kmp_mcp::judgement` lines of this call (BT03): `{site, questions, requests, input_tokens, source, us}`, `source` ∈ `remote`, `cassette_hit`, `cassette_miss`, `book_hit`; `[]` = telemetry present, nothing judged |
| `censored`, `censor_reason` | bool, `timeout` or null | a timed-out call: `wall_ns` is then a lower bound |
| `absent` | `{field: reason}` | every measured field that is null, and only those |

Measured fields: `response_path`, `response_sha256`, `response_bytes`, `wall_ns`,
`server_ms`, `server_us`, `cpu_ns`, `rss_peak_kb`, `io`, `jev`. An `ok` call always
points at its response.

```json call
{"schema": "kmp.bench.call.v1",
 "run_id": "d69566426316216abb930f49452b8c6d3fdb12cd4a70d82fcc8b3753d6c978e0",
 "question_id": "synth7-mono-enum-0001", "variant": "baseline", "sample": 0, "repeat": 0,
 "journey": "synth7-mono-enum-0001~s0~r0", "call_index": 0, "process_call_index": 0,
 "phase": "first",
 "binary_sha256": "1111111111111111111111111111111111111111111111111111111111111111",
 "tool": "kmp_ask",
 "arguments": {"about": "synth:mono-a000",
               "question": "What do we know about C6.4: the decision, its constraint and its status?",
               "answer_policy": "evidence_or_unknown", "budget": {"max_bytes": 4096}},
 "status": "ok", "rpc_id": 3, "response_path": "synth7-mono-enum-0001~s0~r0.jsonl#1",
 "response_sha256": "7777777777777777777777777777777777777777777777777777777777777777",
 "response_bytes": 4011, "wall_ns": 48210033, "server_ms": 46, "server_us": null,
 "cpu_ns": 45110000, "rss_peak_kb": 58312,
 "io": {"rchar": 912345, "wchar": 4200, "syscr": 310, "syscw": 12, "read_bytes": 0,
        "write_bytes": 0, "cancelled_write_bytes": 0},
 "jev": null, "censored": false, "censor_reason": null,
 "absent": {"server_us": "binary logs duration_ms only (before BT03)",
            "jev": "binary emits no kmp_mcp::judgement telemetry (before BT03)"}}
```

### 3.4 Journey record (`kmp.bench.journey.v1`)

`status` keeps token_harness' `JourneyOutcome` names (`completed`, `call_cap_reached`,
`tool_error`, `rpc_error`, `transport_error`). A journey stopped by `max_calls` is
`call_cap_reached` and censored with `max_calls`; one whose call timed out is
censored with `timeout`. `startup_ns` is process start to `initialize` answered, for
the journey that started the process (null with a reason otherwise).

```json journey
{"schema": "kmp.bench.journey.v1",
 "run_id": "d69566426316216abb930f49452b8c6d3fdb12cd4a70d82fcc8b3753d6c978e0",
 "question_id": "synth7-mono-enum-0001", "variant": "baseline", "sample": 0, "repeat": 0,
 "journey": "synth7-mono-enum-0001~s0~r0", "tool": "kmp_ask", "status": "completed",
 "calls": 2, "max_calls": 256, "censored": false, "censor_reason": null,
 "startup_ns": 81200311, "error": null, "absent": {}}
```

## 4. Variant (`kmp.bench.variant.v1`)

One TOML per arm in `variants/` (BENCH_SPEC section 8). Relative paths resolve against
the repository root.

| Key | Type | Meaning |
|---|---|---|
| `schema` | optional, `"kmp.bench.variant.v1"` | |
| `name` | `[a-z0-9][a-z0-9_-]*` | |
| `description` | string, optional | |
| `claim` | `parity`, `quality`, `cost` | |
| `targets` | list | pre-registered target metrics. `quality`/`cost`: tables `{metric, direction = "up"/"down", effect > 0}` (the smallest effect worth claiming; MDE above it is `indecidible`). `parity`: bare metric names allowed |
| `guards` | list | metric names (margin 0) or `{metric, margin}` |
| `n` | int ≥ 1 | pre-registered paired questions |
| `store` | `shared`, `own` | `shared`: import the base's bundle (same `content_digest`) with this binary; `own`: build through this binary's write path and also measure build and write ms |
| `jev` | `off`, `replay`, `record` | |
| `[binary]` | exactly one of `path`, `git_ref` | `git_ref` is built in a worktree with `cargo build --release --locked -p kmp-mcp` and provenance |
| `[store_files]` | name → path, `{path = ...}` or `{json = {...}}` | files copied into the store data directory; names are bare `*.json`/`*.kmpb`; inline JSON is written canonically |
| `[env]` | allowlist | `KMP_LEXICAL_BRIDGE`, `KMP_MCP_ENGINE`, `KMP_TYPESAFE_CASSETTE`, `KMP_TYPESAFE_CASSETTE_MODE` (`replay`/`record`), `RUST_LOG` |

Rules:

- Any other environment name is refused; a name containing `KEY`, `TOKEN`, `SECRET`,
  `PASSWORD` or `CREDENTIAL` is refused as a secret (`TYPESAFE_API_KEY` is handed to
  the runner as a secret in record mode, BT19, and never appears in a manifest).
- The runner owns the rest of the child environment: token_harness isolation (`HOME`,
  `XDG_*`, `CODEX_HOME`, `KMP_MCP_DATA_DIR` inside a disposable root,
  `KMP_MCP_BACKEND=embedded`, `KMP_VIEWER_ADDR=off`); a variant's `RUST_LOG` is appended
  to the directives the harness needs (the data-dir event and `kmp_mcp_tool`).
- `jev = "off"`: no cassette variable and no `typesafe.json`. `replay`: a cassette, if
  named, must exist and the cassette mode may not be `record`. `record`: the cassette
  mode may not be `replay`. `KMP_LEXICAL_BRIDGE` must exist.
- Every store file must be acknowledged by `kmp_mcp::store_config` (BT03), otherwise the
  variant is `not_applied`.
- `Variant.preregistration_digest()` covers `name`, `claim`, `targets`, `guards`, `n`
  and is recorded before the candidate's first run. `Variant.config_digest()` covers
  what changes the binary's behaviour: `jev`, each store file's SHA-256, and the
  environment (path-valued keys by their file's SHA-256, so moving a checkout changes
  nothing). The binary and the store are separate key parts.

```toml variant
# variants/baseline.toml
name = "baseline"
description = "v0.23.0 release binary, as shipped."
claim = "parity"
targets = ["parity_rate"]
guards = []
n = 60
store = "shared"
jev = "off"

[binary]
path = "target/release/kmp-mcp"
```

```toml variant
name = "rerank-narrow"
claim = "quality"
targets = [{ metric = "useful_rate", direction = "up", effect = 0.10 }]
guards = [
  "false_answer_rate",
  "stale_serve_rate",
  "future_leak_rate",
  { metric = "core_precision", margin = 0.0 },
]
n = 100
store = "shared"
jev = "replay"

[binary]
git_ref = "main"

[store_files]
"typesafe.json" = { json = { endpoint = "https://api.typesafe.ai/v1/systemone", model = "jev-1.13.0", timeout_ms = 20000 } }
"rerank.json" = { json = { pool_size = 40 } }

[env]
KMP_TYPESAFE_CASSETTE = "crates/kmp-testkit/judged/retrieval.jev.cassette.json"
RUST_LOG = "kmp_mcp::judgement=debug"
```

## 5. Report (`kmp.bench.report.v1`)

`report.json` is the only source of numbers; `report.md` is rendered from it alone,
deterministically. Top level: `schema`, `bench_version`, `report_key`, `mode`,
`generated_by`, then the eleven sections of BENCH_SPEC section 12, in this order:
`provenance`, `headlines`, `by_type`, `scale`, `latency_resources`, `tokens`,
`jev_by_site`, `controls`, `power`, `limitations`, `verdict`.

Shared shapes:

- **Metric**: `{"value": number|null, "n": int, "ci95": [lo, hi]|null, "method":
  "wilson"|"clopper_pearson"|"bootstrap"|"exact"|null, "absent_reason": string|null}`.
- **Delta**: `{"metric", "baseline": Metric, "candidate": Metric, "delta", "ci95",
  "method": "mcnemar_exact"|"paired_bootstrap", "p_value", "discordant": {"improved",
  "worsened", "rate"}, "mde", "effect", "decidable"}`.
- **Verdict values** (ASCII slugs of section 12): `mejora`, `mejora_con_coste`,
  `solo_coste`, `neutral`, `regresion`, `indecidible`, `no_comparable`,
  `captura_fallida`.
- Private corpora appear only as aggregates; `per_question` rows exist only for public
  corpora. ANSWER and PARTIAL are always separate metrics.
- Metric names and directions: `domain/metric_catalog.py` (what variant targets and
  guards may name, besides `parity_rate`, `tokens_journey`, `tokens_first_page`,
  `pages` and `jev_usd`).

What BT10 fills beyond the example (`application/report.py`; the file on disk keeps
this key order, compact, no NaN):

- `provenance.baseline|candidate`: also `store`, `samples`, `repeats`, `max_calls`,
  `max_bytes` (one per run of the arm: primary run first, then sweep/level runs).
- `headlines[]`: one row per run (`arm`, `run_id`, `level`, `topology`, `max_bytes`),
  plus `false_unknown` (`scorecard` and `given_known`), `high_precision`,
  `high_certified`, `tokens_per_useful`, `errors`.
- `by_type[type]`: `metrics`, `deltas` (McNemar, no bootstrap), `per_question` keyed by
  arm; also `outcomes`, `reasons` (native and bench), `confidence` (per level,
  Clopper-Pearson, AURC), `scorecard` (the `retrieval_scorecard.rs` port) and `strata`
  (tags `bridge:`, `form:`, `degree:`, `abouts:`, `supersession:`, `interval:`, `k:`,
  `hops:`, `undeclared:`, `lang:`).
- `tokens`: `encoders`, `representation` (`json_compact_lexical_v1`, token_harness'
  primary), `absent_reason`, `by_type[type][arm][encoding]` with `journey`,
  `first_page`, `to_first_evidence`, `to_task_ready`; `arms[arm]` with
  `startup_amortized`, `tokens_per_useful`, `agent_load[tool]` and `wake_load` (pages,
  response bytes and tokens until the journey completes); `curve[arm]` (quality-tokens
  points per `max_bytes` and level).
- `jev_by_site`: `price_usd_per_mtok`, `arms[arm]` (`sites` or null with
  `absent_reason`), `delta_usd`, `marginal_cost_per_useful`.
- `controls`: `aa`, `determinism_rate` and `determinism` (a `determinism.py` report),
  `parity` (parity oracle between the two runs), `repeat[arm]` (repeat 0 against later
  repeats, JSON-RPC id set aside), `unpaired_questions`.
- `power[]`: targets and guards, then headline rates; a single-arm report gives
  `mde_at_reference_d` for d = 0.08 and 0.30 instead.
- `verdict`: also `deltas` (`tokens_journey`, `jev_usd`, `useful_rate` when measured).

```json report
{"schema": "kmp.bench.report.v1", "bench_version": "kmp.memory_bench.v1",
 "report_key": "8888888888888888888888888888888888888888888888888888888888888888",
 "mode": "quick-a", "generated_by": {"code_sha256": "9999999999999999999999999999999999999999999999999999999999999999", "python": "3.12.3"},
 "provenance": {
   "baseline": {"variant": "baseline", "run_ids": ["d69566426316216abb930f49452b8c6d3fdb12cd4a70d82fcc8b3753d6c978e0"],
                "binary_sha256": "1111111111111111111111111111111111111111111111111111111111111111",
                "binary_version": "0.23.0", "config_digest": "2222222222222222222222222222222222222222222222222222222222222222",
                "preregistration_digest": "5555555555555555555555555555555555555555555555555555555555555555"},
   "candidate": null,
   "questions": {"digest": "4444444444444444444444444444444444444444444444444444444444444444",
                 "by_corpus": {"synth-v1": 304}},
   "driver_version": "kmp.native_driver.v1", "encoders": ["o200k_base", "cl100k_base"],
   "rules": {"scoring_rules_sha256": null, "negatives_sha256": null, "modes_sha256": null},
   "comparable": true, "drift": []},
 "headlines": [{"level": 1000, "topology": "mono", "arm": "baseline",
                "metrics": {"useful_rate": {"value": 0.71, "n": 304, "ci95": [0.66, 0.76],
                                            "method": "wilson", "absent_reason": null}}}],
 "by_type": {"enumerative_anchored": {"n": 40, "metrics": {}, "deltas": {}, "per_question": []}},
 "scale": {"exponents": [], "calibration": null},
 "latency_resources": {"by_tool_phase": []},
 "tokens": {"encoders": ["o200k_base", "cl100k_base"], "by_type": {}},
 "jev_by_site": null,
 "controls": {"aa": null, "determinism_rate": null, "parity": null},
 "power": [{"metric": "useful_rate", "n": 304, "discordant_rate": null, "mde": null,
            "effect": null, "decidable": false}],
 "limitations": ["n real below 60: B-real not run in this mode"],
 "verdict": {"value": "no_comparable", "claim": null, "targets": [], "guards": [],
             "cost": null, "reasons": ["single arm: nothing to compare"]}}
```

## 6. Cache keys (`domain/cachekey.py`)

Every key is `digest({"kind": K, "bench_version": BENCH_VERSION, "material": M})`, so
kinds never collide and a bench version bump invalidates every cache entry.

| Kind | Function | Material |
|---|---|---|
| `result` | `result_key(binary_sha256, config_digest, store_key, questions_digest, mode, nonce=None)` | section 8 of the spec; `nonce` only for `jev = "record"` runs, which are never reused |
| `world` | `world_key(generator, generator_version, seed, topology, max_level, block_size)` | |
| `store` | `store_key(source, n, bundle_format, reader_sha256, build, writer_sha256=None, batch_size=None, store_mode="shared")` | `source` is `synth_source(generator, generator_version, seed, topology, world_digest)` or `bundle_source(content_digest, label)`; `build` ∈ `ingest`, `import`, `copy` |
| `report` | `report_key(result_keys, options)` | result keys sorted |

- `questions_digest(questions)` (`question.py`): SHA-256 over `id \0 digest \0` for each
  question sorted by id, where a question's digest is the digest of its canonical record;
  duplicate ids are refused. Order-free on purpose: the set is the identity.
- `Variant.config_digest()` and `Variant.preregistration_digest()`: section 4.
- Binary identity is the SHA-256 of the executable file.

## 7. Cache layout (`runtime/layout.py`)

```text
<repo>/tmp/memory-bench/               public cache (git-ignored)
  worlds/<world_key>/                  section 1
  stores/<store_key>/                  store.json (kmp.bench.store.v1), bundle.jsonl, store/ (template data dir)
  runs/<run_id>/                       section 3
  reports/<report_key>/                report.json, report.md
  bin/<binary_sha256>/                 kmp-mcp built from a git_ref, provenance.json
  jev/<key>/                           Jev books per variant and sample (BT11)
  work/                                disposable stores of running sessions; deletable any time

$MEMORY_BENCH_PRIVATE_ROOT/            outside the repository; required for private corpora
  <YYYY-MM-DD>/                        B-real freeze: store/, questions.jsonl, gold.jsonl, labels/
  cache/                               same tree as above for private stores, runs and reports
```

- Every keyed entry is a directory named by its full 64-hex key; nothing else is written
  outside `work/` and keyed entries (`CacheLayout.require_inside`).
- A store template is never opened by a run: each run copies it (`cp -a`) into `work/`
  first, so every run starts from the same `content_digest`.
- `store.json` (`kmp.bench.store.v1`, BT14): section 7.1.
- tiktoken assets stay where token_harness keeps them (`tmp/tiktoken-cache`).
- The private root has no default. The recommended value on the GX10 is
  `~/Documents/ai/artifacts/kmp-bench-private`; `private_layout` refuses any root inside
  the repository.


### 7.1 Built store (`kmp.bench.store.v1`)

`python3 -m scripts.performance.memory_bench build --seed 7 --levels 1e3,1e4 --topology
mono,multi [--batch-size 5000] [--binary W] [--reader R]` makes two entries per
(topology, level, batch size), both keyed by `cachekey.store_key`:

- `build = "ingest"`: the writer loads the world over MCP stdio (section 1.3), then
  `kmp-mcp export` writes `bundle.jsonl`. `reader_sha256 = writer_sha256`, and
  `bundle_format = "stdio-ingest"`: nothing was read from a bundle.
- `build = "import"`: the reader replays that bundle into an empty store with
  `kmp-mcp import`, then re-exports it; the build stops unless the re-export has the
  same `content_digest` and `event_count`. `bundle_format` is the bundle header's
  `{bundle_format, event_format}`, so a reader of another format gets another key.

`source` is `synth_source(...)` with `world_digest` = the level digest of `n` (equal to
the world digest of a world generated up to `n`, by the nesting invariant), and
`batch_size` is in the key because the bundle has one event per batch: B changes the
`content_digest`. Runs read the `import` entry; the `ingest` entry keeps the write-path
profile. `CachedStore` → `run_questions.StoreRef(key=record.store_key,
content_digest=record.content_digest, label=record.label, template=entry.template)`.

| Field | Meaning |
|---|---|
| `store_key`, `material` | the key and exactly the keyword arguments of `cachekey.store_key` that give it (a reader recomputes and refuses a mismatch) |
| `label` | human label: `synth-v1 seed 7 mono 1000 B1000` (+ ` imported`) |
| `content_digest`, `event_count` | KMP's own bundle digest (with `sha256:`) and events |
| `bundle` | `{sha256, bytes, header}` of `bundle.jsonl`; `header` keeps `bundle_format`, `event_format`, `kernel_version`, `snapshot_id`, `event_count`, `content_digest` |
| `template` | `{tree_sha256, bytes}` of `store/` (token_harness `tree_digest`, which every run's copy is checked against) |
| `timings_ms` | wall ms: `ingest` (every `kmp_ingest` and write over MCP), `export` (the entry's `kmp-mcp export`; the re-export for an `import` entry), `import` (`kmp-mcp import`), `copy` (one copy of the template into `work/`, what every run pays) |
| `absent` | timing → why it is null (an `ingest` entry was not imported; an `import` entry was not ingested) |
| `ingest` | `ingest` entries: the profile below; null for `import` |
| `checks` | what was verified before the entry was committed: `receipts_accepted`, `export_data_dir` (the export named the disposable store), `reexport_content_digest`, `source_store_key`, `events_imported`, `mutations_applied`, `template_excludes` (`["logs"]`: the build's server journals never enter a template) |
| `created_at` | RFC 3339 UTC |

`ingest` profile: `batch_size`, `abouts` (in load order), `columns` = `["about",
"held_before", "entries", "relations", "wall_ms", "server_ms"]` and `batches` (one row
per `kmp_ingest`: about index, entries the about held before the call, entries and
relations sent, harness wall ms, `kmp_mcp_tool` `duration_ms`), `entries`, `relations`,
`writes`, `reviews`, `write_ms` (wall ms per write, review included), `ingest_ms`,
`writes_ms`, `server` (`serverInfo`), `cpus_allowed`, `pinning`, `loadavg`. The build
pins the writer to cores 15-19 (the Cortex-X925 cores latency runs do not use) unless
`--cpus` says otherwise.

`domain/ingest_growth.py` fits the batch rows as `wall_ms ≈ c_B·B + c_H·H + c_BH·B·H +
c_BB·B²` (H = `held_before`) and predicts a larger build by summing its batches;
`build --batch-size 1000,5000 --estimate 1e5` prints the fit pooled over both sizes.
Measured on v0.23.0 at 10^4 mono (BT14): c_H ≈ 0.51 ms per held entry per batch and
c_B ≈ 0.08 ms per entry sent, with B·H and B² under 1 % of the largest batch. A batch
re-reads the about it lands in, so a build is O(N²/B), not O(B·N); hence
`DEFAULT_BATCH = 5000`.

```json store
{"schema": "kmp.bench.store.v1",
 "store_key": "c9948f65469bc6463f94d367bc687352804dc7df1a4c00a5e2a9e12658839180",
 "material": {"source": {"kind": "synth", "generator": "synth-v1", "generator_version": "1.1.0",
                         "seed": 7, "topology": "mono",
                         "world_digest": "c2c0c20d5d0469133fb0d0c169201f63c3751500273b7b13697502de47211f0e"},
              "n": 1000, "bundle_format": {"bundle_format": 3, "event_format": 2},
              "reader_sha256": "0358908298685e66fc4f0d37d8345fac058be6d401d2fb00ff97296634518f6f",
              "build": "import",
              "writer_sha256": "0358908298685e66fc4f0d37d8345fac058be6d401d2fb00ff97296634518f6f",
              "batch_size": 1000, "store_mode": "shared"},
 "label": "synth-v1 seed 7 mono 1000 B1000 imported",
 "content_digest": "sha256:d9c936abe9238ad00aba22b50de5c0c69a23beaacdc74704f54703f04d53821e",
 "event_count": 1,
 "bundle": {"sha256": "57407b85f783cff461ed1853ec856f0e5d9b00df8c5242d121383a07be8de81f", "bytes": 1225509,
            "header": {"bundle_format": 3, "event_format": 2, "kernel_version": "0.23.0",
                       "snapshot_id": "content-d9c936abe9238ad0", "event_count": 1,
                       "content_digest": "sha256:d9c936abe9238ad00aba22b50de5c0c69a23beaacdc74704f54703f04d53821e"}},
 "template": {"tree_sha256": "87bc9f060b93430fee9a92c60731daa0a18247d8c2affc2ba16358abf9d66aef", "bytes": 8978436},
 "timings_ms": {"ingest": null, "export": 29.757, "import": 118.551, "copy": 2.588},
 "absent": {"ingest": "imported from the bundle of store 105972f1472dec75210fcf929e81567f5cb572c138111ce2eb2722b9f19c2bc6"},
 "ingest": null,
 "checks": {"source_store_key": "105972f1472dec75210fcf929e81567f5cb572c138111ce2eb2722b9f19c2bc6",
            "reexport_content_digest": "equal", "events_imported": 1, "mutations_applied": 4313,
            "template_excludes": ["logs"]},
 "created_at": "2026-09-25T23:49:58Z"}
```

`memory_bench materialize --store-key K [--dest DIR]` copies a template into the cache
root (`work/` by default); `memory_bench cache ls` lists the entries, `cache gc` removes
abandoned `work/staging-*` directories. An entry is staged in `work/` and renamed into
`stores/<key>/` in one step and is never overwritten.

## Write cost (`kmp.bench.write_cost.v1`)

`python3 -m scripts.performance.memory_bench.application.write_cost --binary B [--out DIR]`
(BT05): milliseconds to write one entry. One pinned process over a store (empty or a
template copy) runs `warmup` single-entry `kmp_ingest` calls unmeasured, then `writes`
measured ones, each its own journey; every write must be accepted or the command fails.
With `--out`, `write-cost.json` (canonical JSON) sits beside the process and per-write
traces (`write-cost~w<NNN>.jsonl`).

| Field | Meaning |
|---|---|
| `schema`, `binary_sha256`, `tool` (`kmp_ingest`), `about`, `entries_per_write` (1) | identity |
| `writes`, `warmup` | the plan (at least 2 measured writes) |
| `accepted`, `censored` | measured writes accepted by `receipt.is_accepted`, and timed out |
| `wall_ns`, `server_ms`, `server_us`, `cpu_ns`, `rss_peak_kb` | `{n, p50, p95, min, max}` over accepted, uncensored writes (the same probes as a call record); null when nothing was measured |
| `startup_ns`, `cpus_allowed`, `machine` | the process and the machine (`probes.machine`) |
| `samples[]` | one per measured write: `write`, `status`, `accepted`, `calls`, `wall_ns`, `censored`, `server_ms`, `server_us`, `cpu_ns`, `rss_peak_kb`, `server_absent` (why the server line is missing, else null) |

```json
{"schema": "kmp.bench.write_cost.v1", "binary_sha256": "…", "tool": "kmp_ingest",
 "about": "bench:write-cost", "entries_per_write": 1, "writes": 20, "warmup": 1,
 "accepted": 20, "censored": 0,
 "wall_ns": {"n": 20, "p50": 2100000, "p95": 2900000, "min": 1800000, "max": 3100000},
 "server_ms": {"n": 20, "p50": 2, "p95": 3, "min": 1, "max": 3},
 "server_us": null, "cpu_ns": null, "rss_peak_kb": null,
 "startup_ns": 81200311, "cpus_allowed": "5-9", "machine": {"arch": "aarch64"},
 "samples": [{"write": 0, "status": "completed", "accepted": true, "calls": 1,
              "wall_ns": 2100000, "censored": false, "server_ms": 2, "server_us": null,
              "cpu_ns": null, "rss_peak_kb": null, "server_absent": null}]}
```

## Telemetry (kmp-mcp log lines, BT03)

`kmp-mcp` logs JSON Lines through `tracing_subscriber::fmt().json()` to stderr (a bounded,
lossy queue: lines can be dropped if stderr is not drained) and, with the embedded
backend, to `<data dir>/logs/kmp-mcp.log.<YYYY-MM-DD>` (UTC date, not lossy). The bench should
parse the file journal, or drain stderr continuously. Every line has the shape

```json
{"timestamp": "2026-09-25T22:48:54.599332Z", "level": "DEBUG", "target": "kmp_mcp::judgement",
 "fields": {"message": "...", "event": "kmp_judgement", "...": "..."}}
```

Select lines by `target` and `fields.event`; ignore `message` (human text, may change)
and unknown fields (fields may be added, never renamed). Integers are JSON numbers.
No line carries stored text, questions, answers, error messages or keys.

Filter the bench passes (it is also what the variant `[env] RUST_LOG` is appended to):

```text
RUST_LOG=kmp_mcp=info,kmp_mcp::judgement=debug,kmp_mcp::store_config=debug
```

`RUST_LOG=kmp_mcp::judgement=debug` alone silences every other `kmp_mcp` line, including
`kmp_mcp_tool`; always keep `kmp_mcp=info`.

### `kmp_mcp_tool` (every tool call)

Target `kmp_mcp::serving::telemetry::recorders`, level `INFO` on success and `WARN` on
error, `fields.event = "kmp_mcp_tool"`. Pre-existing fields (`kmp_move`, `backend`,
`status`, `duration_ms`, counts) are unchanged; BT03 adds:

| Field | Type | Meaning |
|---|---|---|
| `duration_us` | int | server-side duration of the tool call in µs (same clock as `duration_ms`) |

Call record mapping: `server_us = duration_us`, `server_ms = duration_ms`. A binary
older than BT03 has no `duration_us`: `server_us = null` with an `absent` reason.

### `kmp_judgement` (every Jev evaluation)

Target `kmp_mcp::judgement`, level `DEBUG` (off unless asked for; computing the line's
`request_key` costs one SHA-256 of the request body, so it is skipped when disabled).
One line per `JudgementModel::evaluate` a site makes, after it returns; a site may
evaluate several times in one tool call (curate paths, focus reviews), and a call
with a frozen selection (continuation page) evaluates nothing.

```json
{"timestamp":"2026-09-25T22:48:54.599332Z","level":"DEBUG","target":"kmp_mcp::judgement",
 "fields":{"message":"judgement evaluated","event":"kmp_judgement","site":"rerank",
           "model":"jev-1.13.0","source":"cassette_hit","status":"ok","questions":3,
           "answers":3,"requests":1,"http_requests":0,"input_tokens":395,"elapsed_us":127,
           "request_key":"a0b4a155271abd4b5cbb4c446fba3ccb7a96299de1b60a27e222b270aded6b2b"}}
```

| Field | Type | Meaning |
|---|---|---|
| `site` | enum | who spent it, below |
| `model` | string | pinned model (`jev-1.13.0`) |
| `source` | `remote`, `cassette_hit`, `cassette_miss` | provider over the network; answered from `KMP_TYPESAFE_CASSETTE`; not in the cassette (replay: the evaluation fails; record: the provider was asked and the answer kept) |
| `status` | `ok`, `error` | |
| `questions` | int | typed questions in the request (one per passage for rerank/wake focus) |
| `answers` | int | `ok` only: answers returned |
| `requests` | int | `ok` only: provider requests this judgement costs (budget batching, `typesafe_batches.rs`); for a cassette hit, what the recording cost |
| `http_requests` | int | `ok` only: HTTP requests this process actually sent: `0` for `cassette_hit`, else `requests` |
| `input_tokens` | int | `ok` only: provider-reported input tokens (summed over batches; from the cassette entry on a hit) |
| `elapsed_us` | int | wall time of the evaluation in µs, retries included |
| `request_key` | hex | SHA-256 of the whole request's provider body = the cassette entry key (`cassette_judgement.rs`), so a line joins its cassette/book entry |
| `error_hash` | hex16 | `error` only: first 16 hex of SHA-256 of the error message |

On `status = "error"` the token fields are absent, never `0`. Errors on the network
path (`source = "remote"`) include timeouts and 429 after retries.

Sites (stable words; new ones may be added):

| `site` | Caller | Opt-in | BENCH_SPEC §7 letter |
|---|---|---|---|
| `rerank` | `kmp_ask` re-ranking | `rerank.json` | p |
| `wake_focus` | `kmp_wake` with an intent | `wake-focus.json` | p |
| `curate_review` | `kmp_curate` `mode: review` without `focus` | `typesafe.json` | r |
| `curate_focus` | `kmp_curate` `mode: review` with `focus` | `typesafe.json` | r |
| `write_relations` | relations proposed after a write | `write-relations.json` | r |
| `precheck` | pre-check of accepted relations (`mode: apply`) | `typesafe.json` | t |
| `paths` | `kmp_curate` `mode: paths` (suspects, facts on the way, pair typing) | `typesafe.json` | w/n/c |
| `labels` | `kmp_curate` `mode: labels` | `typesafe.json` | t |
| `summaries` | `kmp_summaries_audit` meaning check | `typesafe.json` | s/b |

Call record mapping (`domain/run_record.py` `JevEvaluation`): `site`, `questions`,
`requests`, `input_tokens`, `source` copied; `us = elapsed_us`. Lines are attributed to
the call during which they were logged (timestamps between request write and response
read of the same process; tool calls are serial). A binary older than BT03 emits no
`kmp_judgement` lines: `jev = null` with reason, never `[]`. With BT03, a call on a store
with `typesafe.json` loaded that logs no line judged nothing: `jev = {evaluations: []}`.

Checked on 2026-09-26 against `crates/kmp-testkit/judged/retrieval_cases.json` with rerank
narrow (`{"pool_size":40}`) in cassette replay through the stdio binary: 35 lines, all
`cassette_hit`, each line's `input_tokens` and `requests` equal to the cassette entry its
`request_key` names; the 34 distinct keys are the whole cassette (14 197 input tokens;
one request repeats across two cases, so the lines sum to 14 595).

### `kmp_store_config` and `kmp_store_config_summary` (store open)

Target `kmp_mcp::store_config`, emitted once each time the embedded backend opens a
store (the retrying backend may reopen it). Files consulted:

| `file` | Where | Applied when |
|---|---|---|
| `typesafe.json` | beside the store | the judgement model loaded (remote, or cassette replay/record) |
| `typesafe-cassette` | `KMP_TYPESAFE_CASSETTE` | same as `typesafe.json` |
| `rerank.json` | beside the store | the reranker loaded (needs a working `typesafe.json`) |
| `wake-focus.json` | beside the store | wake focus loaded (needs a working `typesafe.json`) |
| `write-relations.json` | beside the store | a working `typesafe.json` |
| `semantic-retrieval.json` | beside the store | the loopback semantic retriever loaded |
| `lexical-bridge.kmpb` | `KMP_LEXICAL_BRIDGE`, else beside the store, else the machine table | the table loaded and is not empty; absent from the report when `KMP_LEXICAL_BRIDGE=none` |
| any other `*.json` / `*.kmpb` beside the store | | never: `reason = "not read by this kmp-mcp version"` |

Absent files produce no line.

```json
{"level":"DEBUG","target":"kmp_mcp::store_config",
 "fields":{"message":"store configuration loaded","event":"kmp_store_config",
           "data_dir":"/abs/store","file":"rerank.json","path":"/abs/store/rerank.json",
           "sha256":"72d3e914f1d1df2b95cfa58c360c34bac7a7dfa28c017bb0406412490febaee4",
           "bytes":16,"status":"loaded"}}
{"level":"WARN","target":"kmp_mcp::store_config",
 "fields":{"message":"store configuration present but ignored","event":"kmp_store_config",
           "data_dir":"/abs/store","file":"rerank.json","path":"/abs/store/rerank.json",
           "sha256":"...","bytes":16,"status":"ignored",
           "reason":"rerank.json needs typesafe.json beside the store"}}
{"level":"DEBUG","target":"kmp_mcp::store_config",
 "fields":{"message":"store configuration read","event":"kmp_store_config_summary",
           "data_dir":"/abs/store","loaded":"typesafe.json,typesafe-cassette,rerank.json,lexical-bridge.kmpb",
           "ignored":""}}
```

Levels: `loaded` lines and the summary are `DEBUG` (the file is read and hashed only
then, so an ordinary start does not hash a 13 MB bridge); `ignored` lines are `WARN` and
appear under the default filter. `sha256` is of the file bytes as read at open time
(`""` with `bytes = 0` if the read failed between the check and the hash). `loaded` and
`ignored` in the summary are comma-joined `file` values in report order.

Variant acknowledgement (§4): a store file named `N` is applied iff a
`kmp_store_config` line has `file = N`, `status = "loaded"` and `sha256` equal to the
SHA-256 the runner copied; `status = "ignored"` or no line makes the variant
`not_applied`, reporting `reason`. A binary older than BT03 emits no such lines: every
store file is then `not_applied` with reason "binary predates store_config telemetry".

## Search probe (`kmp.bench.search_probe.v1`)

`kmp_search_probe` shows what the kernel's tokenizer makes of a text, so the bench can
explain a lexical miss without reimplementing KMP's tokenizer in Python. It is a thin
binary over the public facade `kmp_proto_mapping::v1beta1::SearchProbe`
(`crates/kmp-proto-mapping/src/v1beta1/memory_mapping/search_probe.rs`), which calls the
same functions the ranker does. It opens no store, calls no model and reads no `KMP_*`
variable: its output is a pure function of the input and the kernel build, so a probe
result is cacheable by (binary SHA-256, input digest).

```text
cargo build --release -p kmp-testkit --bin kmp_search_probe
target/release/kmp_search_probe probes.jsonl > terms.jsonl   # or: ... < probes.jsonl, or "-"
```

Input: JSON Lines, one request per line; blank lines are skipped. Unknown fields are
refused. Exactly one of `text` or `texts` is required.

```json
{"id": "q-017", "about_texts": ["The deployment of the gateway was frozen during the audit."], "carries_search_summary": false, "texts": ["Which valves moved after PR #83?"]}
```

- `about_texts`: the about's own memory texts (entry texts, details, relation
  rationale/motivation/evidence). The morphology is read once from them, as the ranker
  reads it from the bundle (`search_morphology` in `answer_recall_context.rs`): Snowball
  `spanish` or `english` when `LanguageVocabulary` reads one language, none for an empty,
  too-short or evenly mixed about.
- `carries_search_summary` (default `false`): whether the about holds a linted English
  `search_summary`. When the language cannot be read and this is `true`, the probe stems
  in the kernel's search language (`english`), exactly like the ranker's fallback.
- `id` (optional, any JSON value): echoed on every output line.

Output: one canonical-order JSON line per text, keys sorted, every field present.

```json
{"compound_identifiers":[],"concept_keys":["83","after","concept:movement","pr","valves"],"id":"q-017","identifiers":["#83","pr"],"index":0,"informative_terms":["83","after","moved","pr","valves"],"language":"english","line":1,"schema":"kmp.bench.search_probe.v1","search_keys":["83","after","concept:movement","pr","valv"],"text":"Which valves moved after PR #83?"}
```

| Field | Meaning |
|---|---|
| `line` | 1-based input line (blank lines counted) |
| `index` | position of the text inside `texts` (0 for `text`) |
| `language` | the Snowball language the probe stems in, or `null` (exact matching) |
| `informative_terms` | `kmp_domain::language::informative_tokens`: folded (NFKD, no diacritics, lowercase, `ß`→`ss`) tokens split on non-alphanumerics, minus stop words and one-letter non-digits |
| `concept_keys` | each informative term through the hand-kept concept table only (`concept:movement`, ...); unknown words unchanged |
| `search_keys` | what the ranker compares: concept key when the table knows the word, else the Snowball stem under `language`. Equal to `AnswerCandidateTerms.text` for the same text (unit-tested) |
| `identifiers` | `kmp_domain::language::identifiers`, folded |
| `compound_identifiers` | always `[]` until P3 lands multi-token identifiers |

All term lists are sorted and unique because the ranker compares sets. Errors (bad
JSON, both or neither of `text`/`texts`, unreadable file, extra arguments) go to stderr as
`kmp_search_probe: line N: ...` with exit status 2 and no partial guarantee for later
lines; lines before the error have already been written.
