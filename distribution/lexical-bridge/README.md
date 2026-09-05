# The shipped lexical-bridge table

`kmp-lexical-bridge.kmpb` is the table every release publishes as an asset and
`kmp-mcp setup` installs, once for the machine, at
`<user data home>/kmp/lexical-bridge.kmpb`. It is what lets `kmp_ask` answer a
question written in one language from a memory written in another, with the
word pairs it used declared on every citation. The mechanism is documented in
[`scripts/lexical-bridge/`](../../scripts/lexical-bridge/README.md); this
file records what these exact bytes are made of, under what licence they
travel, and what was measured on them.

The release candidate copies this file and its checksum byte for byte
(`kmp-release candidate assemble --lexical-bridge distribution/lexical-bridge`),
and the directory is a release input: a rebuilt table is a candidate that no
longer matches until it is rebuilt too. `crates/kmp-mcp/tests/shipped_lexical_bridge.rs`
proves on every run that the kernel reads these bytes, that the checksum is
theirs, and that the documented pairs bridge.

## Provenance

| input | source | revision | licence |
|---|---|---|---|
| vectors | [`sentence-transformers/static-similarity-mrl-multilingual-v1`](https://huggingface.co/sentence-transformers/static-similarity-mrl-multilingual-v1), first 64 Matryoshka dimensions, one unit vector per word quantized to signed bytes | `b68f4122911bcffcd6e1f695f2d99cd6788972d8` | Apache-2.0 |
| word list | every whole Latin-script word in that model's `bert-base-multilingual-uncased` tokenizer vocabulary, joined with every whole Latin-script word in the [`sentence-transformers/LaBSE`](https://huggingface.co/sentence-transformers/LaBSE) tokenizer vocabulary | teacher as above; LaBSE `836121a0533e5664b21c7aacc5d22951f2b8b25b` | Apache-2.0 (both, Google) |

Built on 2026-09-05 with
`scripts/lexical-bridge/build.py --shipped-vocabulary --dims 64`:
177,388 words × 64 dimensions, 13.3 MB, `sha256` in
[`kmp-lexical-bridge.kmpb.sha256`](kmp-lexical-bridge.kmpb.sha256). The
header's provenance field names the teacher model.

No other source enters the table. The teacher's model card lists training
sets under other terms (MUSE under CC BY-NC 4.0, StackExchange duplicates
under CC BY-SA, OpenSubtitles); this artifact relies on the licence the
weights are published under, as every downstream use of a released model
does, and records that reliance here rather than leaving it implied. The
licence of this artifact is therefore the product's own, Apache-2.0, with
attribution to the two model releases above; `THIRD_PARTY_NOTICES.md` carries
the same line.

## Measured

Vectors, on the MUSE Spanish–English test dictionary (2,416 pairs, CC BY-NC
4.0, used to measure and never as a source). Of those pairs, 1,681 (70 %)
have both words in this table; the rest name a form the two vocabularies do
not carry, mostly inflected verbs. True pairs at or above the bar, over the
pairs the table holds / over every pair counting an absent word as a miss /
random Spanish–English pairs at or above it:

| bar | true, in table | true, all pairs | random |
|---|---|---|---|
| 0.40 | 0.90 | 0.63 | 0.003 |
| 0.45 (the kernel's) | 0.87 | 0.61 | 0.001 |
| 0.50 | 0.85 | 0.59 | 0.001 |

The in-table rate is the prototype's (0.87 at 0.45): same teacher, same
arithmetic. What this table lacks against the 86k-word prototype is
coverage, and coverage is exactly what the licence decision bought — the
prototype's list came from `wordfreq` (CC BY-SA) and could not ship.

Retrieval, on the judged collection (`scripts/ci/retrieval-baseline.sh` with
`KMP_LEXICAL_BRIDGE` pointed at this table instead of the 157-word fixture):
35 cases, `recall_at_1` 0.8714, `recall_at_5` 0.9714, `mean_reciprocal_rank`
0.9357, `ndcg_at_10` 0.9446, `answer_core_precision` 0.9143 — every recorded
floor held, and no case lost rank against the fixture run. Of the 694 word
types the judged cases use, 570 (82 %) are in this table.

What is not in it, by name, so nobody is surprised: inflected verb forms
(`congelaron`, `adelantó`, `froze`), some plurals (`auditores`, `auditors`)
and some domain nouns (`despliegue`, `canteen`, `backlog`). A question that
hinges on one of those matches within one language for that word. Widening
the list means a second source and, with every permissive one checked so far
(Google Books Ngrams under CC BY 3.0, RLA-ES under MPL), a second licence on
the artifact; that is a product decision recorded in #517, not a build flag.

## Rebuilding

```bash
uv venv .venv && uv pip install --python .venv/bin/python tokenizers safetensors huggingface_hub numpy
.venv/bin/python scripts/lexical-bridge/build.py --shipped-vocabulary \
  --output distribution/lexical-bridge/kmp-lexical-bridge.kmpb
(cd distribution/lexical-bridge && sha256sum kmp-lexical-bridge.kmpb > kmp-lexical-bridge.kmpb.sha256)
```

Both revisions are pinned in `build.py`, so a rebuild from the same inputs is
the same bytes. Re-measure and update this file before claiming anything
about a table built from different ones.
