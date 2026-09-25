# TypeSafe Jev with KMP: judged evaluation

This measures what Jev adds to KMP on a corpus whose answers a reader wrote
before any run:

- `kmp_curate` review: the relations it finds and types, and the declared
  relations it doubts;
- the `apply` pre-write check;
- where `kmp_ask` puts the answer, with and without re-ranking.

## How to run it

```bash
bash scripts/ci/jev-baseline.sh
```

The scorecard (`crates/kmp-testkit/src/bin/jev_kmp_scorecard.rs`) seeds three
stores in process from `crates/kmp-testkit/judged/jev_cases.json`:

- one without Jev;
- one with `typesafe.json`;
- one with `typesafe.json` and `rerank.json`.

It answers every judgement from `crates/kmp-testkit/judged/jev.cassette.json`.
With no key and no network, a judgement that was never recorded fails the
run instead of being guessed. Floors are in `jev-baseline.tsv`. A number may
rise freely; lowering one is a reviewed change that says why.

To measure a new model, a new prompt or a corpus change, record again with a
real key:

```bash
set -a; . ~/.config/typesafe.env; set +a
KMP_TYPESAFE_CASSETTE_MODE=record JEV_BASELINE=write bash scripts/ci/jev-baseline.sh
```

Recording goes through `KMP_TYPESAFE_CASSETTE` and
`KMP_TYPESAFE_CASSETTE_MODE=record|replay`, which wrap the store's TypeSafe
judgement model. They are for evaluation only. The cassette holds requests'
digests and Jev's answers, never a key.

## The corpus

There is one case, `payments-platform`. It is a bilingual journal of a
payments platform across `service:billing` (English) and `service:pagos`
(Spanish): 33 facts and 9 declared relations.

- **Missing relations (8):** a status update, a replacement rule, a real
  contradiction, a support, a causal decision, a paraphrase that shares no
  word with its pair, and two English–Spanish equivalences. Each lists the
  types a reader accepts.
- **Distractors (4):** pairs that share a version number, a person, a cluster
  name or an incident id without being related.
- **Declared relations:** 4 sound ones. 5 planted bad ones:
  - a contradiction declared as `supports`;
  - an absurd `why`;
  - a causal link with no cause;
  - a `supersedes` in the wrong direction;
  - a Spanish contradiction declared as `supports`.
- **Pre-write checks (5):** three sound whys, and two that support nothing.
- **Asks (8):** lexical questions, paraphrases and cross-language questions,
  each with the memories that answer it.

## Decision: incorporate or drop, piece by piece

Each piece is judged on three things:

- **Quality:** does it get more right?
- **Agent load:** how many tokens the calling LLM reads and writes to reach
  the same outcome. This is what makes KMP cheaper for the people who use
  it.
- **Jev cost:** Jev's tokens, dollars and latency.

The rule is the one agreed on 2026-09-25: whatever does not improve is
considered for removal. All figures come from `jev-1.13.0`:

- the judged corpus: two cases, 33 and 315 facts, recorded three times;
- the 35 independent retrieval cases that already existed;
- a copy of a real 307-fact store, for Ask only. Numbers only: its content
  is private and is not committed.

Economics use Jev at $0.042 per million input tokens and, for the calling
LLM, $3 per million input tokens (a Sonnet-class price), at about 4 bytes
per token. At $15 per million the LLM side is five times larger, and every
saving below grows with it.

| Piece | Quality | Agent load | Jev cost | Verdict |
|---|---|---|---|---|
| `kmp_curate` review: Jev pairing and typing | Finds 11/11 missing relations against 7/11 with kernel pairs alone; 8/11 typed as a reader accepts; 4/4 distractors rejected. The same in 3 samples. | Curating the corpus by hand through `kmp_relate` means reading 132 KB (~33k tokens). `kmp_curate` without Jev: 25.6 KB. With Jev: 12.6 KB (~3.2k tokens). | 67k tokens for the whole corpus: $0.003. | **Keep.** About 10× less reading than by hand, half of curate without Jev, and more found. On the corpus: ~$0.099 of LLM reading by hand against ~$0.010 + $0.003. |
| Audit of stored relations (`suspect`) | 3/4 planted bad flagged, no false alarm, sound declarations left alone. The same in 3 samples. | The agent reads a handful of flagged items instead of every declaration. | Part of the review request. | **Keep.** Its miss is the reversed `supersedes`, below. |
| Direction question in the audit | Adds nothing: 0.75 with or without it, in all 3 samples. With the author's why, Jev follows the why (a reversed pair went from 0.01 to 0.86 "forward"). Without the why, the batch still read reversed replacements as forward. | None. | Extra questions per audit. | **Removed** from the audit. |
| Direction question in the pre-write check | Catches the reversed `updates_state` a writer was about to commit: 7/8 right against 6/8 without it, in all 3 samples. | Avoids writing, and later repairing, a relation the wrong way round. | Included in the pre-write request (about 2k tokens). | **Keep**, without the why, with dated texts. |
| Pre-write check | 7/8 right: absurd whys held back, sound ones written. | Prevents a bad write that would cost a review and a supersede later. | About 2k tokens per apply: < $0.0001. | **Keep.** |
| Dates in front of fact texts | Needed for the pre-write direction. Typing moved both ways when they were added. | None. | None. | **Keep.** Replacements and updates cannot be judged without time. |
| Ask re-ranking, narrow pool (40 whole passages) | Independent cases: top-1 0.843 → 0.900, top-5 0.943 → 1.000, MRR 0.907 → 0.964. Trap corpus: MRR 0.19 → 0.92–0.94 (3 samples). Real store: no change (0.625). | Responses grow about 9% (the rescue is appended). It saves a follow-up ask each time it rescues a miss: 2 of 35 independent cases, most trap cases. Roughly neutral on tokens where the lexical ranker is already right; a clear saving where it is not. | About 400 tokens per ask on small stores and 6k on a 315-fact store: $0.00002–$0.00025. About +0.5 s on small stores and +4.5 s on the real store. | **Keep as opt-in.** |
| Ask re-ranking, wide pool (400 excerpts of 300 characters) | Real store: reaches an answer the narrow pool never listed (MRR 0.625 → 0.688). Elsewhere equal to narrow, within noise. | +14% bytes on the real store. | About +5 s per ask on the real store. | **Keep as the configuration for large stores.** It is the only arm that helped where the lexical ranker fails. Latency is the price. |

| Focused `kmp_wake` (`wake-focus.json`, intent-driven) | Required memories delivered: 15/15 against 14/15 plain. Plain lost the one in the other about. The same in 3 samples. | First-page bytes over 6 intents fall from 49.9 KB to 36.4 KB (−27%); on the 318-fact store −30 to −45% per wake. | Part of the 30 passage calls of the run. About 7k tokens per wake: $0.0003. | **Keep as opt-in.** Less for the agent to read, and nothing required lost. Caveat: in this corpus the required memories are the ones wake already favours (decisions with relations), so plain wake competes on its best ground. |
| Whole paths (`kmp_curate mode: paths`) | Paths found: 3/5 against 0/5 over declared relations alone. Proposed steps right: 79–93% (3 samples). The misses: a valid direct step shorter than the reader's gold (left as a miss, not re-judged), and a chain that walked a planted bad declaration. With the walk audited (below), no returned path walks a planted bad declaration; the one avoided is bad (1/1). | An answer is about 2.2 KB per search, against reading the selection through `kmp_relate` (133 KB) to trace it by hand. | About 14k tokens per search: $0.0006. The audit adds 2 requests and 1.6k tokens over the corpus. | **Keep.** Paths the graph does not hold yet appear, and every proposed step can be declared through `apply`. The walk is audited. |

**Overall: incorporate Jev.** `kmp_curate` is where it pays most clearly: less
reading for the agent, more relations found, and a cost in fractions of a
cent. Ask re-ranking earns its place where the lexical ranker misses
(paraphrases, cross-language questions, large stores). It stays opt-in
because it adds latency and bytes to every Ask.

### How wake focus and paths work

- **Wake focus.** When the caller states an `intent` and the store has
  `wake-focus.json` (`{"pool_size": 400, "excerpt_chars": 300}`) beside
  `typesafe.json`, Jev reads every admitted evidence entry. For each it
  answers whether the entry matters for resuming that work. The packet keeps
  the entries at 0.5 or above, best first, and reports the rest as withheld.
  The focus is frozen per intent, pool and model for continuation pages.
  With no intent, the wake is the ordinary one.
- **Paths.** Jev reads the whole selection, one about or several, and keeps
  what lies on the way from `from` to `to` (up to 120 facts). Among those it
  picks each fact's direct consequence and direct cause, up to two of each at
  probability 0.3 or above. The steps are typed, joined to the declared
  relations, and searched: fewest hops, then fewest proposed steps, with up
  to three alternatives. With no `to`, the search returns the longest chains
  from `from`.

  The declared relations a result walks are then audited. Jev asks whether
  each one's why and evidence hold, the same support question as the review.
  Any below 0.3 is left out and the search runs again, up to three rounds.
  Only walked declarations are asked, so the audit costs one small request
  per round. The answer lists them under `avoided`. On the corpus, the
  goal-less chain from g30 stopped walking the planted `g05 chosen_because
  g08`. `proposed_hops_right` went from 9/11 to 17/21 because the returned
  alternatives are now proposed chains instead of that declaration. The new
  steps are 8/10 right, so the floor was lowered for that reason and not for
  a worse judge.

  Asking for the cause as well as the consequence, and keeping a second
  choice, took paths found from 1/5 to 3/5 with no loss in step precision.

### Tools behind these numbers

- `bash scripts/ci/jev-baseline.sh`: the judged corpus, replayed from its
  cassette, with the columns above and the agent-load comparison.
- `bash scripts/eval/jev-samples.sh N`: records N independent samples and
  prints the mean and range of every metric. Jev is not deterministic, so
  repeated identical requests move choice probabilities by a few hundredths.
- `RETRIEVAL_RERANK=narrow|wide RETRIEVAL_MAX_ENTRIES=n` on
  `retrieval_kmp_scorecard`: the independent retrieval cases with
  re-ranking. The arm reports without gating, and it is answered from
  `crates/kmp-testkit/judged/retrieval.jev.cassette.json`.

## First result (2026-09-25, `jev-1.13.0`, payments case only)

| Metric | Value |
|---|---|
| Missing relations found | 8/8 (1.00) |
| …typed as a reader accepts | 6/8 (0.75) |
| Distractors left untyped | 4/4 (1.00) |
| Planted bad declarations flagged | 4/5 (0.80) |
| Flags that were planted bad | 4/4 (1.00) |
| Sound declarations left alone | 4/4 (1.00) |
| Pre-write checks decided right | 5/5 (1.00) |
| Ask MRR, without → with re-ranking | 0.375 → 0.875 |
| Ask answer in top 5, without → with | 4/8 → 8/8 |
| Cost of the whole run | 8 requests, 16,257 input tokens |

What the misses say:

- **Direction is not audited.** The planted `supersedes` that runs from the
  old PDF-only rule to its April replacement was not flagged. The audit asks
  whether the why supports the type, and it does, but it never asks which
  way the relation runs. Asking the direction is the next improvement.
- **Replacement read as contradiction.** "Retries twice" against "from 2.5,
  retries once" was typed `contradicts`, not `supersedes`. The two readings
  are close, and the agent still decides.
- **A paraphrase typed `answers`.** The kernel could not pair it; Jev paired
  it correctly, then chose a weaker type.
- **Kernel versus Jev pairing.** Kernel signals found the status update, the
  causal decision and both equivalences. Jev's partner step found the
  replacement, the contradiction, the support and the paraphrase, which share
  no rare identifier.
- **Plain lexical Ask missed a Spanish question** whose answer repeats
  "10.000 euros". Re-ranking put it first.

## What this does not prove

- **The corpus is small.** One case and 33 facts. Every fact fits in the
  re-ranking pool, so re-ranking reads the whole store. On a real store with
  307 facts it gave no gain in four questions
  (`evidence-rerank.md`), because the answer never entered the pool. The
  large gain here is therefore the best case for re-ranking, not the
  typical one.
- **The same author** wrote the corpus and the prompts Jev is asked. A corpus
  written by someone else would test that bias.
- **One recording.** The cassette pins one set of answers. Jev is not
  guaranteed to give the same answers when recorded again, so a re-recording
  is a new measurement, not a check.
- **Thresholds.** The 0.3/0.7/0.5 thresholds were not tuned on this corpus.
  They were fixed before it existed.

Grow the corpus with new cases, especially large stores and relations whose
direction matters, before tuning anything on these numbers.
