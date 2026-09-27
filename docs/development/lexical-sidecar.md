# Lexical sidecar in shadow (P12, DESIGN L6)

`kmp_ask` has no inverted index: every ask reads the about's neighbourhood,
prepares every candidate's terms and measures BM25 over all of them. P12 adds
the index beside the store, maintained from the event log, and compares it with
the ranker on every ask. It answers nothing yet; P13 generates candidates from
its postings through `LexicalCandidates`
(`crates/kmp-mcp/src/serving/ports/lexical_candidates.rs`).

## Off by default

The sidecar is closed unless `KMP_LEXICAL_INDEX=shadow`: `off`, no value or any
other value keep it closed (an unknown value is logged). In shadow it answers
nothing and costs about +5 % per ask at 10^3 and 10^4 and a build on each
about's first ask (31 s at 10^5), so it stays off until P13 answers from it.
BT18 `sidecar_catch_up` opens it explicitly; so do the tests that exercise it
(`EmbeddedKernelMcpBackend::with_lexical_index`).

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
- Closed unless `KMP_LEXICAL_INDEX=shadow`. It can be deleted any time.

## Shadow comparison

Each first-page ask carries a `LexicalShadowWitness`; its ranker records, from
the first collection it builds, N, Σlen per field, every candidate's
fingerprint (tf and length in both fields), df of every weighted term and which
candidates carry one. The sidecar then answers the same through
`LexicalCandidates` and the digests, and one `kmp_lexical_shadow` line reports
`differences` (SCHEMAS.md, Telemetry). An about that differs is forgotten and
built again on its next ask.

## Measured (27 Sept 2026, GX10, release build)

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
at 10^4 (about +5 %), about 0.5 s of comparison at 10^5 (+2 %). P13 removes it.

## What P13 inherits

- `LexicalCandidates::{totals, frequencies, candidates}` answer from the
  sidecar alone, under either reading.
- An ask narrowed by dimensions reads a subset of the about: its rows match the
  about's, but N, Σlen, df and possibly the language are the subset's own.
- Time-scoped asks, several abouts, and asks shallower than depth 2 are not
  indexed.
- PMI expansion (P6) and the lexical bridge need the direct counts and texts
  of the candidates that carry the question's terms: the rows and the store.
- A change to what a row reads fails `derivation_golden_tests` until
  `INDEX_VERSION` is bumped, which rebuilds every sidecar.
