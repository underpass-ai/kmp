# Lexical bridge

The table that lets `kmp_ask` answer a question written in one language from
a memory written in another, inside the kernel, without a model at runtime,
and with the word pairs it used declared on every citation.

## What it is

One vector per whole word, quantized to signed bytes, keyed by the folded form
the ranker already searches with (`válvula` → `valvula`). At runtime the
kernel looks up the question's words and the candidates' words, multiplies
integers, and offers a pair when the cosine clears 0.45. A candidate that
bridges most of the question is an answer and carries `bridged_terms`
(`valvula≈valve 0.51; noche≈night 0.91`); one that bridges a word or two
arrives as proof with `reached_by: bridge`. Confidence is `medium` at most
for an answer that crossed a language: the table's opinion is not the
reader's words.

What it does not do: a single vector per word carries no sense, so
`slipped` does not reach `postponed`. Paraphrase inside one language is not
what the table knows. The judged case `paraphrase-gap` stays open on purpose
and says so.

## Format

`.kmpb`, little-endian, version 1 — read by
`crates/kmp-proto-mapping/src/v1beta1/memory_mapping/lexical_bridge.rs`,
which refuses any file that does not account for every byte.

| field    | size                | meaning                                        |
|----------|---------------------|------------------------------------------------|
| magic    | 8                   | `KMPBRIDG`                                     |
| version  | u16                 | `1`                                            |
| dims     | u16                 | vector width                                   |
| count    | u32                 | words                                          |
| model    | u16 + bytes         | provenance of the vectors                      |
| offsets  | (count + 1) × u32   | byte offsets into the words blob               |
| words    | bytes               | folded words, sorted by byte order, concatenated |
| vectors  | count × dims × i8   | `round(unit_vector × 127)`                     |

Similarity is `dot / sqrt(‖a‖² · ‖b‖²)` over exact integers, so the same
table yields the same number on every platform.

## Building one

```bash
uv venv .venv && uv pip install --python .venv/bin/python model2vec safetensors tokenizers huggingface_hub numpy
.venv/bin/python scripts/lexical-bridge/build.py --vocabulary words.txt --output lexical-bridge.kmpb --dims 64
```

`words.txt` is one word per line. The teacher is
`sentence-transformers/static-similarity-mrl-multilingual-v1` (Apache-2.0):
a static embedding bag over a `bert-base-multilingual-uncased` tokenizer,
Matryoshka-trained so its first 64 dimensions stand on their own. It is
downloaded once; nothing runs it at `ask` time.

Measured on the MUSE es–en test dictionary (1,499 pairs), true pairs at or
above the bar / random pairs at or above it:

| table              | 0.40        | 0.45        | 0.50        |
|--------------------|-------------|-------------|-------------|
| mrl @64 int8       | 0.90 / 0.003 | 0.87 / 0.001 | 0.84 / 0.001 |
| mrl @128 int8      | 0.89 / 0.001 | 0.86 / 0.001 | 0.82 / 0.001 |
| potion-multilingual-128M @256 | 0.83 / 0.003 | 0.79 / 0.001 | 0.76 / 0.001 |
| shipped table, mrl @64 int8, pairs it holds | 0.90 / 0.003 | 0.87 / 0.001 | 0.85 / 0.001 |

Signed bytes cost nothing against f32 at any width. A 64-dimension table
over ~86k Spanish and English word forms is 6.5 MB; the shipped table, over
177k Latin-script words, is 13.3 MB and holds 70 % of the MUSE pairs — its
provenance and every number measured on it are recorded beside it in
`distribution/lexical-bridge/README.md`.

## Installing one

`kmp-mcp setup` installs the table the release publishes, once for the
machine, at `<user data home>/kmp/lexical-bridge.kmpb`. Decline it with
`--no-lexical-bridge`, or install one you built with `--lexical-bridge FILE`.
The table is checked against its published digest and proved to parse before
anything is written, and a table that cannot be fetched leaves setup
successful and says so in the receipt.

Three places are read, nearest first: whatever `KMP_LEXICAL_BRIDGE` names,
then `<data dir>/lexical-bridge.kmpb` for one store alone, then the machine's
table. The machine path exists because a store is selected per working
directory — a project `.kernel/` wins over the user default — and the shipped
table is too large to copy into every project that ever opens memory.

The machine's table converges to the release's: the next `setup` or `update`
without flags replaces whatever `--lexical-bridge FILE` installed there, and
the receipt names the digest it replaced (`replaced_sha256`). A table you
built and want to keep belongs beside one store, or wherever
`KMP_LEXICAL_BRIDGE` points.

Absent, `ask` behaves exactly as before. A malformed table is logged and
ignored; it is an aid to retrieval, not a condition of it.

## The judged fixture

`crates/kmp-testkit/judged/lexical-bridge.kmpb` holds real vectors for the
words the judged cases use — 157 of them — so the retrieval baseline
measures the mechanism rather than whichever table an operator installed.
Rebuild it whenever a case gains a word:

```bash
.venv/bin/python scripts/lexical-bridge/build.py \
  --fixture-from crates/kmp-testkit/judged/retrieval_cases.json \
  --output crates/kmp-testkit/judged/lexical-bridge.kmpb
```

## The shipped table

`distribution/lexical-bridge/kmp-lexical-bridge.kmpb` is the table every
release publishes, built with `--shipped-vocabulary` and committed with its
checksum; [its README](../../distribution/lexical-bridge/README.md) records
the exact inputs, the licence and the numbers measured on it. Rebuild it only
on purpose — a rebuilt table is a release candidate that no longer matches —
and re-measure before claiming anything about it:

```bash
.venv/bin/python scripts/lexical-bridge/build.py --shipped-vocabulary \
  --output distribution/lexical-bridge/kmp-lexical-bridge.kmpb
(cd distribution/lexical-bridge && sha256sum kmp-lexical-bridge.kmpb > kmp-lexical-bridge.kmpb.sha256)
```

## Licensing the vocabulary

The vectors derive from an Apache-2.0 model; the word list decides what the
table may be distributed under, and every input was checked at its source:

- `sentence-transformers/static-similarity-mrl-multilingual-v1` is Apache-2.0.
  Its card lists training sets under other terms (MUSE under CC BY-NC,
  StackExchange under CC BY-SA, OpenSubtitles); the table relies on the
  licence the weights are published under, as every downstream use of a
  released model does, and says so rather than leaving it implied.
- The teacher's `bert-base-multilingual-uncased` tokenizer vocabulary and the
  `sentence-transformers/LaBSE` tokenizer vocabulary are Apache-2.0 releases
  from Google. They are the shipped word list: no other source enters the
  table, so the artifact carries one licence, the product's own.
- `wordfreq`, which the prototype's 86k-word list came from, publishes its
  data under CC BY-SA 4.0 with further credits owed (SUBTLEX, OpenSubtitles,
  Google Books); ShareAlike would travel into the table, and its author states
  that a format with no room for attribution does not satisfy the licence. A
  table built from it stays local.
- The MUSE Spanish–English dictionary the numbers are measured on is
  CC BY-NC 4.0. It measures; it is not in the repository and no word from it
  seeds the vocabulary — which would also leak the test into the table.
- Candidates checked and not used: Google Books Ngrams (CC BY 3.0, attribution
  only) and RLA-ES, the Spanish spelling dictionary (GPL / LGPL / MPL 1.1+),
  either of which would add a second licence to the artifact for the
  inflected forms the tokenizer vocabularies lack. Whether an attribution-only
  list is acceptable is a product decision, not a build option.
