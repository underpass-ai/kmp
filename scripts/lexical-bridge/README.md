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

Signed bytes cost nothing against f32 at any width. A 64-dimension table
over ~86k Spanish and English word forms is 6.5 MB.

## Installing one

Place it beside the store as `<data dir>/lexical-bridge.kmpb`, or point
`KMP_LEXICAL_BRIDGE` at it. Absent, `ask` behaves exactly as before. A
malformed table is logged and ignored; it is an aid to retrieval, not a
condition of it.

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

## Licensing the shipped vocabulary

The vectors derive from an Apache-2.0 model. The word list decides what the
table may be distributed under: a list drawn from `wordfreq` is CC-BY-SA and
would carry that licence into the artifact, so the prototype table built from
it is for local use. A shipped table should draw its list from a permissive
source — SCOWL for English, an MPL-licensed Spanish spelling dictionary — or
from the teacher's own tokenizer vocabulary. That decision is recorded beside
the released artifact, not here.
