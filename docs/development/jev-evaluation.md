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

## Result (2026-09-25, `jev-1.13.0`)

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
