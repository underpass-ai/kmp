#!/usr/bin/env python3
"""Build a KMP lexical-bridge table: whole words, one int8 unit vector each.

The table is what lets `kmp_ask` bridge a question written in one language to
a memory written in another, inside BM25 and with the word pairs it used
declared on every hit. It is built once, offline, from a static embedding
model — a lookup table of token vectors with no transformer behind it — and
read at runtime by ~150 lines of Rust with no new crates.

Format (`.kmpb`, little-endian, version 1):

    magic     8 bytes   b"KMPBRIDG"
    version   u16       1
    dims      u16       vector width
    count     u32       number of words
    model     u16 len + UTF-8 bytes   provenance of the vectors
    offsets   (count + 1) x u32       byte offsets into the words blob
    words     UTF-8 blob, words sorted by byte order, folded (NFKD, no marks,
              lowercase) exactly as `search_terms::fold_search_term` folds them
    vectors   count x dims x i8       round(unit_vector * 127)

Cosine similarity at runtime is an integer dot product over an integer norm,
so the same table gives the same number on every platform.

Teacher: sentence-transformers/static-similarity-mrl-multilingual-v1
(Apache-2.0, static EmbeddingBag over a bert-base-multilingual-uncased
tokenizer, Matryoshka-trained so its first 64 dimensions are a valid model).
Measured on the MUSE es-en test dictionary (1,499 pairs): at 64 dims int8 and
a 0.45 threshold, 87% of true translation pairs bridge and 0.1% of random
pairs do; int8 costs nothing against f32.

Usage:

    build.py --vocabulary words.txt --output lexical-bridge.kmpb [--dims 64]
    build.py --fixture-from crates/kmp-testkit/judged/retrieval_cases.json \
             --output crates/kmp-testkit/judged/lexical-bridge.kmpb

`--vocabulary` is one word per line, any language the teacher covers. Which
list ships is a licensing decision recorded beside the artifact, not here.
"""

from __future__ import annotations

import argparse
import json
import re
import struct
import sys
import unicodedata
from pathlib import Path

MAGIC = b"KMPBRIDG"
VERSION = 1
TEACHER = "sentence-transformers/static-similarity-mrl-multilingual-v1"
STOP_WORDS = set(
    "a against an and are as at be because by came did do does earlier for from he how i "
    "if in is it me more my of on one or plus same should than the this to us use used uses "
    "was we were what when where which who why will with el la los las de al del donde en es "
    "lo no por para que se su un ya como cual cuando".split()
)


def fold(word: str) -> str:
    folded = []
    for character in unicodedata.normalize("NFKD", word):
        if unicodedata.combining(character):
            continue
        folded.append("ss" if character in "ßẞ" else character.lower())
    return "".join(folded)


def informative_tokens(text: str) -> list[str]:
    tokens = []
    for raw in re.split(r"[^0-9A-Za-zÀ-ɏ]+", text):
        token = fold(raw)
        if token and token not in STOP_WORDS and (token.isdigit() or len(token) >= 2):
            tokens.append(token)
    return tokens


def fixture_vocabulary(cases_path: Path) -> list[str]:
    """Every informative word the judged cases use, questions included."""
    document = json.loads(cases_path.read_text(encoding="utf-8"))
    words: set[str] = set()
    for case in document["cases"]:
        words.update(informative_tokens(case["question"]))
        for entry in case["memory"]["entries"]:
            words.update(informative_tokens(entry["text"]))
        for relation in case["memory"].get("relations", []):
            for field in ("why", "evidence"):
                words.update(informative_tokens(relation.get(field, "")))
    return sorted(words)


def load_teacher():
    from huggingface_hub import snapshot_download
    from safetensors.numpy import load_file
    from tokenizers import Tokenizer

    root = snapshot_download(TEACHER)
    table = load_file(f"{root}/0_StaticEmbedding/model.safetensors")["embedding.weight"]
    tokenizer = Tokenizer.from_file(f"{root}/0_StaticEmbedding/tokenizer.json")
    return table, tokenizer


def embed(words: list[str], dims: int):
    import numpy as np

    table, tokenizer = load_teacher()
    vectors = np.zeros((len(words), dims), dtype=np.float64)
    kept = []
    for index, word in enumerate(words):
        ids = tokenizer.encode(word, add_special_tokens=False).ids
        if not ids:
            continue
        vector = table[ids].mean(axis=0)[:dims].astype(np.float64)
        norm = float(np.linalg.norm(vector))
        if norm == 0.0:
            continue
        vectors[index] = vector / norm
        kept.append(index)
    return vectors[kept], [words[index] for index in kept]


def write_table(path: Path, words: list[str], vectors, dims: int) -> None:
    import numpy as np

    order = sorted(range(len(words)), key=lambda index: words[index].encode("utf-8"))
    words = [words[index] for index in order]
    vectors = vectors[order]
    quantized = np.clip(np.round(vectors * 127.0), -127, 127).astype(np.int8)

    blob = bytearray()
    offsets = [0]
    for word in words:
        blob.extend(word.encode("utf-8"))
        offsets.append(len(blob))
    model = TEACHER.encode("utf-8")
    with path.open("wb") as output:
        output.write(MAGIC)
        output.write(struct.pack("<HHI", VERSION, dims, len(words)))
        output.write(struct.pack("<H", len(model)))
        output.write(model)
        output.write(struct.pack(f"<{len(offsets)}I", *offsets))
        output.write(bytes(blob))
        output.write(quantized.tobytes())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--vocabulary", type=Path, help="one word per line")
    source.add_argument("--fixture-from", type=Path, help="judged cases whose words form the vocabulary")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--dims", type=int, default=64, choices=(32, 64, 128, 256))
    arguments = parser.parse_args()

    if arguments.vocabulary is not None:
        raw = arguments.vocabulary.read_text(encoding="utf-8").split()
        # Keyed by the folded form the kernel searches with. Two words that
        # fold onto one key keep the first listed, so a frequency-ordered list
        # keeps its commoner sense.
        seen: dict[str, str] = {}
        for word in raw:
            key = fold(word)
            if key and key not in STOP_WORDS and key not in seen and (key.isdigit() or len(key) >= 2):
                seen[key] = word
        keys = list(seen)
        surface = [seen[key] for key in keys]
    else:
        keys = fixture_vocabulary(arguments.fixture_from)
        surface = keys

    vectors, kept_surface = embed(surface, arguments.dims)
    kept_keys = [keys[surface.index(word)] for word in kept_surface]
    write_table(arguments.output, kept_keys, vectors, arguments.dims)
    size = arguments.output.stat().st_size
    print(f"{arguments.output}: {len(kept_keys)} words x {arguments.dims} dims, {size / 1e6:.2f} MB", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
