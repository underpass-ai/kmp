# Optional Ask re-ranking with TypeSafe Jev

The embedded backend can reorder Ask proof by asking TypeSafe Jev whether
each admitted passage answers the question. It is off by default. Enabling it
sends passage text to TypeSafe on every Ask, which is why it has its own
opt-in on top of the store's TypeSafe one.

Beside the selected store, place:

- `typesafe.json`, the store's TypeSafe opt-in, shared with `kmp_curate`:

  ```json
  {"endpoint": "https://api.typesafe.ai/v1/systemone", "model": "jev-1.13.0", "timeout_ms": 20000}
  ```

- `rerank.json`, Ask re-ranking itself. `pool_size` is from 1 to 400 and
  `excerpt_chars` from 200 to 2000 (default 2000). On a large store, a wide
  pool of short excerpts reaches answers the lexical ranker never lists
  (`jev-evaluation.md`):

  ```json
  {"pool_size": 400, "excerpt_chars": 300}
  ```

and set `TYPESAFE_API_KEY` in the environment of the MCP process. Then
restart that process. The key is never written to a file by KMP and never
appears in logs, warnings or errors.

## What it changes

- **The pool.** Jev reads at most `pool_size` passages (1 to 400). First come
  those in the lexical ranker's own order, then admitted live entries the
  ranker left out, so a paraphrase that shares no word with the question can
  still be read. Nothing the selection does not admit is ever sent: scope,
  interval, lifecycle and content version apply first. Each passage is cut
  to `excerpt_chars`.
- **The question.** Jev answers one yes/no question per passage: does it
  answer the question? Only passages with a probability of at least 0.5
  join the ranking, ordered by that probability, with ties broken by ref and
  then by text. A channel that listed the whole pool would grow proof with
  passages the judge itself finds irrelevant.
- **What the order may move.** It joins the RRF fusion as one more channel,
  like the local semantic channel. The five deterministic citations,
  `because` and confidence stay lexical. An entry that only re-ranking
  reached carries `reached_by: rerank` and `rerank_model`. It extends proof
  and never establishes an answer or removes UNKNOWN.
- **Reproducibility.** The order is frozen per question, pool and model (up
  to 64 selections), so continuation pages never call TypeSafe again. A
  changed snapshot or an evicted selection requires a fresh Ask.
- **Reporting.** The response always names the model and how many judged
  entries it resolved.

## Measured so far

On 2026-09-25, on a copy of a real store (`project:made`, 307 facts), four
paraphrased questions were asked with and without re-ranking:

- Every Ask judged 40 passages in one request.
- With the 0.5 cut, proof kept its size (9, 6, 22 and 8 items). Listing the
  whole pool had grown it to 26 or 30 items.
- Three answer-bearing memories were already first or second without
  re-ranking, and stayed there.
- One (a logo decision stored in Spanish, asked in English) never entered the
  pool. The lexical ranker did not reach it, and the admitted remainder is
  taken in bundle order until the pool is full.

On that sample, re-ranking did not move an answer into the page. Rescuing
distant paraphrases needs a better way to choose the remainder, for example
the semantic channel, before the judge reads it. Keep it off unless a
measurement on your own questions shows a gain.

## Failure

Ordinary retrieval continues with a warning in each of these cases:

- `rerank.json` is present without a working TypeSafe opt-in;
- the key is missing;
- TypeSafe fails, times out or rate-limits beyond two retries;
- TypeSafe answers something that does not match the questions sent.

A failure is frozen like a success, so a later page cannot silently read a
different order.

Design: `docs/development/jev-rerank-design.md`. The TypeSafe client is shared
with `kmp_curate` (`docs/development/jev-curate-design.md`).
