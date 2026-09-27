# Lexical index beside the store (P12, P13, P14, DESIGN L6)

Without an index every `kmp_ask` reads the about's neighbourhood, prepares
every candidate's terms and measures BM25 over all of them. P12 added the
index beside the store, maintained from the event log, and compared it with
the ranker on every ask (shadow). P13 answers from it: an ask it can hold reads
only the candidates its postings reach.

## Modes

`KMP_LEXICAL_INDEX` chooses once per process:

| Value | What the sidecar does |
|---|---|
| `on` (**default**) | Answers every ask it can hold from its postings (below); every other ask reads the about as before. Followed after every write and before every ask. |
| `shadow` | Answers nothing; compares itself with the ranker on every ask (`kmp_lexical_shadow`), as P12 did. |
| `verify` | Answers as the about does and also from the postings, with no cost bound, and logs `kmp_lexical_verify` with whether the two responses are equal. For measuring. |
| `off` | Never opened. |

An unknown value is logged and the default stands. On by default because every
response it gave was byte-identical to the about's on every corpus measured
(below; decision of Tirso, 27 Sept 2026: on by default only with 100 %
parity). BT18 `sidecar_catch_up` and the sidecar's own tests open it
explicitly (`EmbeddedKernelMcpBackend::with_lexical_index`).

## Where and what

`<data dir>/lexical-index.sqlite3` (WAL), beside `store/`: the format gate
refuses unknown files inside `store/` (the same place the verdict book took).

| Table | Holds |
|---|---|
| `meta` | `index_version`, `profile`, `position` (last event followed), `tail` (a witness of that event) |
| `lex_fwd` (`LexFwd`) | per about and candidate (`entry:<node>`, `detail:<node>`): ordinal, aliased fingerprint, the row: per term `text`, `summary`, `extra`, `alias_content`, `alias_extra`, `expansion`, and the part lengths |
| `lex_post` (`LexPost`) | per about, term and block: up to 128 postings, varint deltas of the ordinal and the content/direct counts under both readings; a term only a candidate's judged expansions carry is posted with zero counts, so the postings reach it |
| `lex_key_df` (`LexKeyDf`) | per about and term: df in content and direct, plain and aliased |
| `lex_stats` (`LexStats`) | per about: N, Σ of every part length, how many candidates carry judged expansions and Σ their length, one digest of all rows per reading, the next ordinal, the function-word counts and linted summaries that decide the language, the language the rows were read in, and how many nodes lie one hop past the default depth |
| `lex_node`, `lex_relation`, `lex_far` | what follows the ask's selection one write at a time: each reached node's hop and selection counters and language contribution, each kept relation's language contribution, the nodes one hop past the default depth |
| `lex_clock` (P13) | per kept relation: its sequence, the latest instant it carries and its `valid_until`: the about's frontier and expiries, which an ask that reads only its candidates must still stand on |
| `lex_vocab` (P13) | per about and word: how many candidates' texts carry it (the rows carry each text's words, row layout 4): the vocabulary the lexical bridge reads |

Every part is a residual of the ranker's own counts (`surface_counts`, shared by
the ranker and the index), so content = text + summary (+ alias content) and
direct = content + extra (+ alias extra) hold exactly under both readings.

Judged search expansions (P15, `search_expansions` in the reserved metadata)
are field X: part of neither content nor direct, as the ranker reads them, and
counted apart per term (`expansion`) with their length. A row's fingerprint
covers them only when it has some, so rows without expansions fingerprint as
before; the index version is `lexical-index-2`, which rebuilds sidecars
written by `lexical-index-1`. The shadow also compares how many candidates
carry expansions and Σ their length, and counts a candidate its expansions
alone carry to a weighted term among those the postings must reach
(`expansion_shadow_tests.rs`).

## What is indexed

An about as an ask of it with no dimensions, at the frontier and the default
depth (2) reads it: every node two hops out, the `contains_entry` edges whose
coordinate is the about's own, the entries and labels they keep, the evidence
that supports a kept entry, and the relations among what is kept. Candidates
are the kept entries' texts and the kept evidence's details; the language is
decided from every kept text as `search_language` decides it. A deeper ask
reads the same while no node lies one hop past depth 2 (`lex_far` empty).

## Maintenance

- An about is built on its first ask (a breadth-first read of the store).
- After every write this binary makes, and before every ask, the sidecar
  follows the events appended since its position, whoever wrote them: the
  nodes and edges each event touched are read again over one snapshot
  (`EmbeddedKernelStore::read_points`), and the reach, the selection, the
  language and the rows around them are recomputed and the difference applied.
  Applying the same events twice moves nothing (tested by rewinding the
  position).
- A move of the about's language rebuilds the about; a sidecar of another
  `index_version`, or whose `tail` no longer matches the log, is emptied.
- Every write is one transaction that first checks the position it read; a
  writer that lost the race writes nothing and follows again.
- Opened unless `KMP_LEXICAL_INDEX=off`. It can be deleted any time: the next
  ask of an about builds it again. The index version is `lexical-index-4`
  (P13's clocks and vocabulary; `-4` since «quién» and «con» became stop
  words); a sidecar of another version is emptied.

## Shadow comparison

Each first-page ask carries a `LexicalShadowWitness`; its ranker records, from
the first collection it builds, N, Σlen per field, every candidate's
fingerprint (tf and length in both fields), df of every weighted term and which
candidates carry one. The sidecar then answers the same through
`LexicalCandidates` and the digests, and one `kmp_lexical_shadow` line reports
`differences` (SCHEMAS.md, Telemetry). An about that differs is forgotten and
built again on its next ask.

## Measured in shadow (P12, 27 Sept 2026, GX10, release build)

Shadow comparison, every ask of each corpus through MCP stdio on a fresh copy
of its store. "Compared" asks read what the sidecar indexes; the others were
time-scoped (`as_of`/`interval`), several abouts, or answered before ranking.

| Corpus | Asks | Compared | Differences |
|---|---:|---:|---:|
| Frozen real store: B-real 31, hard negatives and identifier guards (s7, s11, s13 fresh) | 1,047 | 1,042 (+5 narrowed by dimensions: 0 rows or language apart) | 0 |
| synth-v1 seed 7 mono 10^3 | 198 | 179 | 0 |
| synth-v1 seed 7 mono 10^4 | 297 | 276 | 0 |
| synth-v1 seed 7 mono 10^5 (every 8th question) | 50 | 46 | 0 |
| Judged retrieval, 53 cases (depth 3; plain) | 53 | 42 | 0 |
| Judged retrieval, narrow rerank arm | 35 | 24 | 0 |
| Judged Jev corpus, its asks | 48 | 24 | 0 |
| FactConsolidation 32k (`best_effort`) | 184 | 184 | 0 |
| LongMemEval-S 10 per type + 30 abstention (90 abouts) | 90 | 90 | 0 |

BT18 `sidecar_catch_up` (a v0.23.0 writer behind the sidecar, then the same
binary rebuilding from nothing) passes with 0 differences; `write_then_read`,
`concurrent_writers` and `reads_during_writes` pass with the sidecar on (63
asks compared, 6 not comparable because the store moved while they asked).
Answers are byte-identical to the base build (c338a30d) on 384 real and 198
synth asks, and the retrieval, Jev and relate baselines hold.

Build on the first ask of an about, and the file after it:

| Store | Candidates | Build | Sidecar |
|---|---:|---:|---:|
| Real store (6 abouts) | 1,939 | 1.0 s in all (≤ 0.29 s an about) | 8.7 MB |
| synth 10^3 | 1,000 | 0.36 s | 2.3 MB |
| synth 10^4 | 10,000 | 3.2 s | 20.9 MB |
| synth 10^5 | 100,000 | 31 s | 203 MB (store 954 MB) |
| FactConsolidation 32k | 2,310 | 0.26 s | 2.7 MB |
| LongMemEval-S (90 abouts) | 45,171 | 30 s in all (≤ 0.39 s an about) | 206 MB |

Peak RSS of the first ask at 10^5: 4.25 GB against 3.84 GB without the sidecar
(the ask itself holds the whole about).

Write cost: the time the sidecar takes to follow one write, per entry (median
of 3 repetitions, one at 10^5; p95 of the single-entry writes in brackets). The
wall-clock difference against the sidecar closed stays inside the noise of
the write itself.

| Store | 1 entry per write | 50 entries per write |
|---|---:|---:|
| synth 10^3 | 1.18 ms (4.0) | 0.30 ms |
| synth 10^4 | 1.71 ms (6.7) | 0.39 ms |
| synth 10^5 | 1.98 ms (2.1) | 0.45 ms |

In shadow each ask pays for the comparison: +15–20 ms at 10^3 and +170–185 ms
at 10^4 (about +5 %), about 0.5 s of comparison at 10^5 (+2 %). With the index
answering (`on`) there is no comparison.

## Which abouts are indexed, and the cost bound (`lexical-index.json`)

A store tunes both limits in `lexical-index.json` beside it (acknowledged on
`kmp_mcp::store_config` like every other store file; unreadable or out of range,
it is reported as ignored and the defaults apply):

```json
{"max_candidate_share_percent": 35, "min_about_entries": 2250,
 "head_window": 64, "continuation_chunk": 64}
```

- `min_about_entries` (default **2,250**): with the index `on`, an about with
  fewer entries (its `records` edges, counted without building anything) is never
  indexed; its asks read the about and log `the about is below the index's size
  threshold`. `shadow` and `verify` index every about, to measure them.
- `max_candidate_share_percent` (default **35**, 1 to 100): an ask whose
  candidates cover more of the about reads the about instead.

The threshold was chosen by a rule fixed before measuring (`eval-p13/PREREG.md`,
second addendum): the smallest measured about size above which every measured
about recovers its build within three asks (build ≤ 3 × mean saving per ask),
rounded down to a multiple of 250. Build and mean saving per ask against the
integration base, the index already built:

| About | Entries | Asks | Build | Mean saving per ask |
|---|---:|---:|---:|---:|
| Real store, six abouts | 17–309 | 2–432 | 30–356 ms | −26 to −0.3 ms |
| LongMemEval-S, 90 abouts | 396–616 | 1 each | 404 ms (median) | 12 ms (median) |
| synth 10^3 | 1,000 | 198 | 452 ms | 111 ms |
| FactConsolidation 32k | 2,310 | 184 | 306 ms | 116 ms |
| synth 10^4 | 10,000 | 297 | 3,590 ms | 1,200 ms |

Below 2,310 entries some about fails the rule (synth 10^3: 452 ms > 3 × 111 ms;
the real store's abouts save nothing); from 2,310 up every about passes, so the
threshold is 2,250. With it LongMemEval-S (one ask per about of ≤ 616 entries)
builds nothing and no longer regresses.

## Accepted costs

- Following the log after every write costs about 6 ms per single-entry write
  at 10^4 entries (accepted, Tirso, 27 Sept 2026).
- In a project store (commit-native), the process keeps the head bundle it last
  published in memory to extend it by the next write's events: about 121 MB at
  10^5 entries (accepted).

## Answering from the index (P13)

An ask is held when it reads one about at the frontier, with no dimensions, at
depth 2 (or deeper while nothing lies one hop past it), and no channel reads the
whole admitted pool: no semantic retrieval, no remote re-ranking, no doubt band
(`ask_tool.rs`). For a held ask (`indexed_plan.rs`, `indexed_parts.rs`):

1. **Candidates.** C = the postings of the question's words under every form the
   ranker reads (as asked; under the anchored gate without its negated
   stretches and with its alias terms), of their PMI associations (at most three
   per word, computed from the rows of the candidates the words reach and the
   about's df) and, when a table is installed, of the words the lexical bridge
   finds for them in the whole about's vocabulary (`lex_vocab`, at most three per
   word). A candidate outside C carries none of the words the question weighs,
   so it cannot clear the floor (`clears_floor` asks for score > 0).
2. **Cost bound.** When C covers more than 35 % of the about (configurable,
   above) the ask reads the about: reading a candidate point by point costs about twice what reading it
   with the whole about does (0.5 ms against 0.27 at 10^4, 0.65 against 0.28 at
   10^5), and on the frozen real store the index answered faster below 40 % and
   slower above it. P14 (top-k) bounds what an answer reads below that.
3. **Parts.** C, the entries C's evidence supports, and every node two kept hops
   from them (lifecycle chains followed whole: what `restated`, `reached` and a
   lifecycle rescue can bring in), with the labels that keep each, the anchor's
   edge, an entry's evidence and what evidence supports, are read point by point
   from one snapshot at the position the sidecar followed to, and assembled by the
   application exactly as the whole about is (`ask_from_parts`).
4. **Ranking.** The ranker ranks those candidates against the whole about as the
   sidecar holds it: N, Σ length and df under both readings (the collection), the
   frontier and expiries (`lex_clock`), the language and the vocabulary the bridge
   reads. Equal integers give equal scores to the bit.

Everything else reads the about as before and says why at debug
(`kmp_lexical_answer`, `answered = false`, `reason`): another clock, several
abouts, dimensions, a shallower depth, nodes past the indexed depth, a store
that moved during the read, an about with judged search expansions (field X is
measured over every expanded candidate), or the cost bound.

## MaxScore against the floor (P14)

**On by default** (`KMP_LEXICAL_MAXSCORE=off` reads every candidate the
postings reach, as P13 did). Implemented without a measurement campaign
(decision of Tirso, 28 Sept 2026): **not measured at scale**. Exactness is
tested (a randomized parity test, below) and observable in `verify`.

KMP's ask returns every candidate that clears the eligibility floor, ranked,
so the threshold MaxScore prunes against is the floor, not a k-th score: a
candidate that cannot clear it is ranked by nobody and rescued by nobody, and
is left unread (`floor_bound.rs`, `FloorBound`). The plan still reads every
reached row from the sidecar (cheap); what it saves is the point-by-point read
of the candidate and its two-hop neighbourhood (about 0.5 ms a candidate at
10^4), and the cost bound (35 %) now applies to what is left to read.

Why the bound is sound for the whole `RelevanceKey`:

- `clears_floor` asks `Σ w·idf·sat(tf, L) ≥ floor · sat(1, L)` on the direct
  field, with `sat(tf, L) = tf(k1+1)/(tf + k1·n(L))`. For every `tf ≥ 1` and
  every length, `sat(tf, L)/sat(1, L) ≤ tf`, so a candidate whose
  `Σ idf·tf` over the question's words stays below the floor cannot clear it
  whatever its length. The question's own words weigh one. A margin of 1e-9
  covers float drift.
- Content is part of direct, so the content score, the focus count and every
  lower key only order what cleared the floor.
- Both readings (plain; aliased, as the anchored gate reads) are bounded with
  their own idf and floor; a candidate is left unread only when both refuse.
- Every form the ranker may read the question in (as asked; under the gate
  without its negated stretches, with its alias terms) counts its words, and
  the lowest floor of any form is compared.
- Lexical bridge: the floor carries the bridged words exactly as the lexicon
  builds it, and a candidate carrying a bridged word is always read.
- Associations: a candidate carrying one is always read (it may be rescued
  through it). The associations are now counted over the rows of every
  candidate that carries a word of the question (`IndexedAsk::seed_rows`),
  never over the candidates read, so pruning changes no weight.
- A node at the edge of the walk also has its lifecycle edges read, so which
  conflicts on the proof path are live does not depend on what was pruned.

MaxScore's partition makes the common case cheap: words sorted by the most
they can add (`idf · max tf`); those whose running sum stays under the floor
are non-essential, and a candidate carrying only those is refused unsummed.

Where no sound bound exists, the ask is **not pruned** (it is still answered
from the postings as in P13):

- a question whose anchors the gate requires (the gate ranks without the
  focus filter and cites by anchor);
- a form of the question with no informative word (the ranker then returns
  every candidate).

Not held by the index at all, so they read the about as before: judged search
expansions in the about (field X is measured over every expanded candidate),
time-scoped asks (`as_of`, `interval`: idf is local to the selection),
several abouts, dimensions, a shallower depth, semantic retrieval, remote
re-ranking and the doubt band. The ⌈2/3⌉ focus of a strict policy is not used
to prune: it also counts words a candidate's relations and supported claims
carry, which its row does not hold.

`kmp_lexical_answer` and `kmp_lexical_verify` log `reached` (what the postings
reached) beside `candidates` (what was read).

## Top-k and lazy pages (P14)

**On by default**, not measured at scale (Tirso's decision). Breaking for the
answer's semantics, as decided.

**What a ranking is now.** An ask's ranking is a *head* and a *tail*
(`RankedEvidence`). The head is the best 64 eligible candidates (the
diversification window), diversified, with repeated claims at the end of the
window, followed by every rescue walked from them: restatements, lifecycle
heads, expansions, the reach graph, associations and the bridge. The tail is
every other eligible candidate in rank order. The head never depends on the
tail, so a reading carried to any depth of the tail is a prefix of the
exhaustive one.

**What a reading carries.** The head and `depth` tail items: none on a first
page, and on a `kmp2` continuation the resumed offset plus 64
(`ask_rank_depth`), in MCP and gRPC alike. Semantic retrieval, re-ranking, the
doubt band and `budget.max_entries` carry the whole ranking. A reading that
does not carry the whole ranking sets `AskResponse.more_ranked` (new additive
field) and its page offers a deeper continuation.

**Per store.** `lexical-index.json` also takes `head_window` and
`continuation_chunk` (64 each by default, 1 to 4096; decision of Tirso,
28 Sept 2026). They shape every ask of the store, indexed or not. The gRPC
server reads the same keys from `lexical-index.json` in its data directory
(`KMP_DATA_DIR`) with the same rule (`RankPages`), so API and MCP page an ask
alike. A file with an unknown key or any value out of range is discarded
whole on both surfaces, index limits and page sizes alike, with a warning
(`kmp_store_config`, `status = "ignored"`); decisions of Tirso, 28 Sept 2026.

**What changed in the answer (breaking):**

- repeated claims move to the end of the head window, not of the whole list;
  rescues follow the head, not the whole list;
- `proof.path`, its evidence normalization, `proof.superseded` and conflicts
  are the head's; the UNKNOWN summary counts the head;
- a lazy page pages its evidence only: paged supersessions, the balanced
  path and `missing` follow once a reading carries the ranking to its end;
- `page.total`, `sections.*.remaining` and `more_on_request` count what the
  reading carries; the page says so with `page.total_is_lower_bound` (and
  `AskResponse.total_is_lower_bound`, set with `more_ranked`), both additive;
- the core reserves room for the largest of its first 64 items, not of all.

**Reading fewer candidates.** Each candidate's rank prefix (content focus
matches, content BM25, direct BM25, in tenths) is exact from its row
(`RankPrefixes`, the same integers and the same lexicon the ranker builds).
The index reads what top-k must (candidates an association, a bridged word or
a declared lifecycle can rescue, `lifecycle_docs`) and the best
`64 + depth + 16` others by prefix, ties at the cut included, answers them,
and keeps the answer only when it is certified: the reading did not carry
the whole ranking, and every eligible candidate it carried ranks strictly
above the best prefix left unread. Otherwise it reads four times as many, up
to every candidate. The 35 % cost bound applies to each selection. A
question whose anchors the gate requires is ranked as the gate ranks it
(once an anchor decides: the question without its facets, the focus counted
but not required) and every candidate naming one of its anchors is always
read: the gate cites among them in rank order and its proof keeps them. Not
read lazily (every candidate after the floor is read): questions with no
informative word, and asks with `max_entries`. `kmp_lexical_answer`/`kmp_lexical_verify` log `planned` (left
by the floor) beside `candidates` (read by top-k).

**Exactness.** `maxscore_parity_tests.rs`: randomized stores (90 and 260
memories, with and without the shipped bridge, half the questions naming an
anchor the gate requires) answered from the index with
MaxScore and top-k at tail depths 0, 5 and 70 equal the whole about at the
same depth, field for field; and over MCP, every lazy page equals the whole
about's page byte for byte and the pages of one ask concatenate to the
exhaustive ranking. `verify` still compares every ask with the about.

## Continuations: the `kmp2` cursor (P14)

Every Wake/Ask continuation cursor is `kmp2:<offset>:<boundary>:<hash>`:
`boundary` is a 16-hex digest of the last item the page returned, and `hash`
binds the bound arguments, the core and every item the pages before it
delivered (not the items after it, so a deeper reading keeps it). A
continuation resumes only after that exact item; anything else is
`SELECTION_CHANGED` with a restart call, never a shifted page. A `kmp1`
cursor is refused with its own reason, `RECALL_CURSOR_ERROR_REASON_OUTDATED`
(gRPC `ABORTED`; MCP feedback code `READ_CURSOR_OUTDATED`), and a restart
call. Contract break accepted (Tirso, 27 Sept 2026).

## Write in O(delta) (P13)

- A write that reads no neighbourhood for review asks the store only what its
  translation asks (the about's dimensions and labels, whether each named ref
  stands one hop from the anchor, the sequence frontier of each coordinate it
  leaves to the kernel) instead of reading the about's depth-1 neighbourhood;
  frontiers this process's own writes advanced are kept.
- The commit-native guard of a project store (`commit_bundle.rs`) checks from
  the tail: when the committed file (length and modification time) and the log
  (position, and the aggregate, revision and content hash of its last event)
  stand where this process's last publish left them, they are the history that
  publish proved equal, and the bundle is published by extending the head it
  wrote with the events after that tail (`HeadStream`: every export encodes
  through it, so the bytes are a full export's). A second process, a write that
  did not publish, or anything else that moved sends the next write to the full
  check, which still refuses a history that differs.

## Measured with the index answering (P13, 27 Sept 2026)

Every ask of each corpus through MCP stdio, on a fresh copy of its store, with
the base build (d1f2cae7, no index) and with `KMP_LEXICAL_INDEX=on`; responses
compared byte for byte after the continuation handle. "Answered" counts the asks
the index answered; the rest were not held (another clock, several abouts,
re-ranking) or covered more than 35 % of the about.

| Corpus | Asks | Identical | Answered by the index | p50 / p95 ms, base → index |
|---|---:|---:|---:|---|
| Frozen real store (B-real 31, negatives and guards s7/s11/s13) | 1,047 | 1,047 | 486 | 197 / 296 → 177 / 301 |
| The same with the machine's lexical bridge (13 MB table) | 1,047 | 1,047 | 424 | 214 / 375 → 205 / 401 |
| synth-v1 seed 7 mono 10^3 | 198 | 198 | 118 | 307 / 388 → 215 / 435 |
| synth 10^4 | 297 | 297 | 185 | 2,755 / 3,492 → 1,633 / 3,602 |
| synth 10^5 (every 8th question) | 50 | 50 | 29 | 28,427 / 35,856 → 17,413 / 37,380 |
| Judged retrieval, 53 cases (depth 3, fixture bridge) | 53 | 53 | 19 | |
| Judged retrieval, narrow rerank arm | 35 | 35 | 0 (re-ranking) | |
| FactConsolidation 32k | 184 | 184 | 177 | 176 / 189 → 56 / 135 |
| LongMemEval-S (90 abouts) | 90 | 90 | 32 | 266 / 280 → 627 / 662 |

The retrieval rows score the same. LongMemEval asks each of its 90 abouts once,
so every ask pays its about's build; each p95 includes the asks the index does
not answer, which still read the about. Asks the index answered: p50 / p95 74 /
199 ms on the real store, 142 / 276 ms at 10^3, 1,445 / 2,503 ms at 10^4 and
14.2 / 22.8 s at 10^5, where a question's common words reach tens of thousands
of the 100,000 entries; a question whose words reach 1,000–1,500 is answered in
0.8–1.6 s with a peak RSS of 207 MB, against 27.7 s and 4.1 GB reading the about.
`verify` (no cost bound) found every response equal on the real store (1,042),
10^4 (28 asks, up to 5,959 candidates) and 10^5 (10 asks, up to 58,080).

Writes of one entry, p50 (base → P13 with the index on; responses and the asks
after them identical):

| Store | `kmp_ingest` | `kmp_write_memory` | commit-native `kmp_write_memory` |
|---|---:|---:|---:|
| synth 10^3 | 36 → 7.5 ms | 38 → 8.8 ms | |
| synth 10^4 | 219 → 30 ms | 218 → 13 ms | 756 → 36 ms (first write 599 ms) |
| synth 10^5 | 2,700 → 95 ms | 2,681 → 2.6 ms | 8,442 → 442 ms (first write 6.2 s) |

Following a write costs the index about 6 ms of that at 10^4 (23.5 and 7.5 ms
with it off).

## Not held yet

- An ask narrowed by dimensions reads a subset of the about: its rows match the
  about's, but N, Σlen, df and possibly the language are the subset's own.
- Time-scoped asks, several abouts, and asks shallower than depth 2.
- A question whose words leave more than 35 % of the about to read once
  MaxScore has left out what cannot clear the floor (P14).
- A change to what a row reads fails `derivation_golden_tests` until
  `INDEX_VERSION` is bumped, which rebuilds every sidecar.
