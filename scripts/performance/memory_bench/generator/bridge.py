"""Which crosslang questions the shipped lexical bridge can reach (tag `bridge:covered`).

A reader of `distribution/lexical-bridge/kmp-lexical-bridge.kmpb` (format v1, see
`lexical_bridge_table.rs`), with the kernel's pairing rule from `lexical_bridge.rs`: a
question word bridges to a candidate word when both are in the table, they differ, the
integer cosine is at least `MINIMUM_SIMILARITY`, and the candidate is among the word's
`MAX_BRIDGES_PER_WORD` best. The cosine is the kernel's: an integer dot product over
integer norms, one square root and one division.

Words are folded the kernel's way (NFKD, no diacritics, lowercase, split on anything not
alphanumeric, one-letter non-digits dropped) minus a short English/Spanish stop list kept
here, so the tag is a pure function of the question and the table, not of a probe build.

Approximation, stated: the kernel ranks a word's neighbours over the whole candidate
vocabulary of the about; here the vocabulary is the gold entry's own terms, so a word can
look bridged here and lose its slot to closer words of other entries in the store.
"""
from dataclasses import dataclass
import math
from pathlib import Path
import re
import struct
import unicodedata

from ..domain.errors import BenchError
from ..domain.jsonl import REPO_ROOT

SHIPPED = REPO_ROOT / 'distribution' / 'lexical-bridge' / 'kmp-lexical-bridge.kmpb'
MAGIC = b'KMPBRIDG'
MINIMUM_SIMILARITY = 0.45
MAX_BRIDGES_PER_WORD = 3
# A question counts as bridge-covered when at least this many of its informative terms
# bridge to a term of the gold entry (or all of them, for a shorter question).
COVERED_PAIRS = 2


STOP_WORDS = frozenset((
    'the a an of to in on at by for from with and or is are was were be been do does did '
    'what which who whom whose where when why how this that these those it its their his '
    'her our your my me we you they he she them us there here not no yes into onto about '
    'el la los las un una unos unas de del al en con por para y o que qué quien quién '
    'quienes donde dónde cuando cuándo como cómo cual cuál sus su se lo le les es son fue '
    'era a adónde').split())


def fold(text):
    """Kernel-style informative words of `text` (see the module docstring)."""
    text = unicodedata.normalize('NFKD', text)
    text = ''.join(char for char in text if not unicodedata.combining(char)).lower()
    words = [word for word in re.split(r'[^0-9a-z]+', text) if word]
    return [word for word in words
            if (len(word) > 1 or word.isdigit()) and word not in STOP_WORDS_FOLDED]


STOP_WORDS_FOLDED = frozenset(
    ''.join(char for char in unicodedata.normalize('NFKD', word) if not unicodedata.combining(char))
    for word in STOP_WORDS)


class BridgeTableInvalid(BenchError):
    code = 'BRIDGE_TABLE_INVALID'


@dataclass(frozen=True)
class BridgedPair:
    question: str
    candidate: str
    similarity: float


class BridgeTable:
    def __init__(self, data):
        if data[:8] != MAGIC:
            raise BridgeTableInvalid('not a lexical bridge table: bad magic')
        version, dims, count, provenance_len = struct.unpack_from('<HHIH', data, 8)
        if version != 1 or dims == 0:
            raise BridgeTableInvalid(f'unsupported bridge table v{version} with {dims} dims')
        cursor = 18 + provenance_len
        offsets = struct.unpack_from(f'<{count + 1}I', data, cursor)
        cursor += 4 * (count + 1)
        words = data[cursor:cursor + offsets[-1]]
        cursor += offsets[-1]
        if len(data) - cursor != count * dims:
            raise BridgeTableInvalid('vector section does not account for every byte')
        self.dims, self.count = dims, count
        self.provenance = data[18:18 + provenance_len].decode('utf-8')
        self._index = {words[offsets[i]:offsets[i + 1]].decode('utf-8'): i for i in range(count)}
        self._vectors = memoryview(data)[cursor:]
        self._norms = {}

    @classmethod
    def load(cls, path=None):
        return cls(Path(path or SHIPPED).read_bytes())

    def __contains__(self, word):
        return word in self._index

    def _vector(self, index):
        return struct.unpack_from(f'<{self.dims}b', self._vectors, index * self.dims)

    def _norm(self, index):
        if index not in self._norms:
            self._norms[index] = sum(value * value for value in self._vector(index))
        return self._norms[index]

    def similarity(self, left, right):
        """The kernel's cosine, or None when a word is unknown."""
        if left not in self._index or right not in self._index:
            return None
        a, b = self._index[left], self._index[right]
        dot = sum(x * y for x, y in zip(self._vector(a), self._vector(b)))
        norms = self._norm(a) * self._norm(b)
        return 0.0 if norms == 0 else dot / math.sqrt(norms)

    def bridge(self, question_terms, vocabulary):
        """The kernel's `bridge`: each question word's best candidates above the bar."""
        pairs = []
        vocabulary = sorted(set(word for word in vocabulary if word in self))
        for word in dict.fromkeys(question_terms):
            if word not in self:
                continue
            found = [(self.similarity(word, candidate), candidate)
                     for candidate in vocabulary if candidate != word]
            found = [(score, candidate) for score, candidate in found
                     if score >= MINIMUM_SIMILARITY]
            found.sort(key=lambda item: (-item[0], item[1]))
            pairs.extend(BridgedPair(word, candidate, score)
                         for score, candidate in found[:MAX_BRIDGES_PER_WORD])
        return pairs


def coverage(table, question, entry):
    """('covered' | 'uncovered', bridged question words) for one crosslang question."""
    question_terms, entry_terms = fold(question), fold(entry)
    pairs = table.bridge(question_terms, entry_terms)
    bridged = sorted({pair.question for pair in pairs})
    needed = min(COVERED_PAIRS, len(set(question_terms)))
    return ('covered' if needed and len(bridged) >= needed else 'uncovered'), bridged
