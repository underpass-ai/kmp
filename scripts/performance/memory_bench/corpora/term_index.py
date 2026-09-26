"""Per-about index of what the kernel's tokenizer reads in each entry.

Built once per about from the store copy and the search probe: for each entry
its identifiers and search keys, the inverted maps, the anchors it mentions
and, for the store's own words, the search key each word becomes. Every
question the generators build is decided from this index, and verified
against it again after rendering.
"""
from dataclasses import dataclass
import re
import unicodedata

from .anchor_forms import anchors_in
from .search_probe import ProbeScope

WORD = re.compile(r'[^\W\d_]+', re.UNICODE)
# A word after a lowercase letter or clause punctuation and a space: not sentence-initial.
MID_SENTENCE_WORD = re.compile(r'(?<=[^\W\d_,;:(] )([^\W\d_]+)|(?<=[,;:(] )([^\W\d_]+)')


def fold(word):
    """ASCII facet name: NFKD, diacritics dropped, lowercase."""
    text = unicodedata.normalize('NFKD', word)
    return ''.join(c for c in text if not unicodedata.combining(c)).lower()


@dataclass(frozen=True)
class DocTerms:
    ref: str
    active: bool
    raw: str
    identifiers: frozenset
    keys: frozenset
    own_raw: str = ''  # the entry's own words (no relation explanations)
    own_keys: frozenset = frozenset()


class AboutIndex:
    """Read-only index of one about. `docs` are sorted by ref."""

    def __init__(self, about, language, docs, neighbors, words):
        self.about, self.language = about, language
        self.docs = tuple(sorted(docs, key=lambda d: d.ref))
        self.by_ref = {doc.ref: doc for doc in self.docs}
        self.neighbors = {ref: frozenset(items) for ref, items in neighbors.items()}
        self.key_docs = {}
        for doc in self.docs:
            for key in doc.keys:
                self.key_docs.setdefault(key, set()).add(doc.ref)
        self.key_docs = {key: frozenset(refs) for key, refs in self.key_docs.items()}
        self.word_key = dict(words)  # surface word -> its single search key
        self.key_word = {}
        counts = {}
        for doc in self.docs:
            for word in set(WORD.findall(doc.raw.lower())):
                counts[word] = counts.get(word, 0) + 1
        for word, key in sorted(self.word_key.items()):
            best = self.key_word.get(key)
            if best is None or (counts.get(word, 0), word) > (counts.get(best, 0), best):
                self.key_word[key] = word
        self.capitalized = {}  # word -> (capitalized mid-sentence occurrences, mid-sentence occurrences)
        for doc in self.docs:
            for match in MID_SENTENCE_WORD.finditer(doc.raw):
                word = match.group(1) or match.group(2)
                upper, seen = self.capitalized.get(word.lower(), (0, 0))
                self.capitalized[word.lower()] = (upper + word[0].isupper(), seen + 1)
        self._memo = {}
        self.anchor_docs = {}
        for doc in self.docs:
            for anchor in anchors_in(doc.identifiers):
                self.anchor_docs.setdefault(anchor, set()).add(doc.ref)

    def docs_with_key(self, key):
        return self.key_docs.get(key, frozenset())

    def df(self, key):
        return len(self.docs_with_key(key))

    def _cached(self, kind, anchor, compute):
        key = (kind, anchor)
        if key not in self._memo:
            self._memo[key] = compute(anchor)
        return self._memo[key]

    def mentions(self, anchor):
        """Wide, lexical: refs holding every key of one of the anchor's key sets."""
        return self._cached('mentions', anchor, self._mentions)

    def _mentions(self, anchor):
        refs = set()
        for keyset in anchor.keysets():
            sets = [self.docs_with_key(key) for key in keyset]
            refs |= frozenset.intersection(*sets) if sets else frozenset()
        return frozenset(refs)

    def strict(self, anchor):
        """Written occurrences of the anchor (see anchor_forms)."""
        return self._cached('strict', anchor, self._strict)

    def _strict(self, anchor):
        pattern = anchor.strict_pattern()
        return frozenset(doc.ref for doc in self.docs if pattern.search(doc.raw))

    def strict_own(self, anchor):
        """Entries whose own words write the anchor: gold uses this, absence checks use `strict`."""
        return self._cached('strict_own', anchor, self._strict_own)

    def _strict_own(self, anchor):
        pattern = anchor.strict_pattern()
        return frozenset(doc.ref for doc in self.docs if pattern.search(doc.own_raw))

    def own_docs_with_key(self, key):
        return frozenset(doc.ref for doc in self.docs if key in doc.own_keys)

    def anchors(self):
        """Anchors the kernel reads as identifiers here and that occur written, sorted."""
        return tuple(sorted(a for a in self.anchor_docs if self.strict(a)))

    def around(self, refs, hops):
        """`refs` plus everything within `hops` entry-to-entry relations."""
        seen, frontier = set(refs), set(refs)
        for _ in range(hops):
            frontier = {n for ref in frontier for n in self.neighbors.get(ref, ())} - seen
            seen |= frontier
        return frozenset(seen)

    def keys_of(self, refs):
        return self._cached('keys_of', frozenset(refs), self._keys_of)

    def _keys_of(self, refs):
        keys = set()
        for ref in refs:
            keys |= self.by_ref[ref].keys
        return frozenset(keys)

    def surface(self, key):
        return self.key_word.get(key)

    def proper_ratio(self, word):
        upper, seen = self.capitalized.get(word, (0, 0))
        return upper / seen if seen else 0.0


def build_index(about_docs, probe, candidate_word=None):
    """AboutIndex of one AboutDocs, reading every text through `probe`."""
    joined = [doc.joined() for doc in about_docs.entries]
    owned = [doc.own() for doc in about_docs.entries]
    scope = ProbeScope.of(about_docs)
    terms = probe.probe(scope, joined)
    own_terms = probe.probe(scope, owned)
    languages = {t.language for t in terms}
    language = terms[0].language if len(languages) == 1 and terms else None
    docs = [DocTerms(doc.ref, doc.active, raw, t.identifiers, t.search_keys, own, o.search_keys)
            for doc, raw, t, own, o in zip(about_docs.entries, joined, terms, owned, own_terms)]
    words = sorted({w for raw in joined for w in WORD.findall(raw.lower())
                    if candidate_word is None or candidate_word(w)})
    word_terms = probe.probe(scope, words)
    pairs = [(w, next(iter(t.search_keys))) for w, t in zip(words, word_terms)
             if len(t.search_keys) == 1 and not t.identifiers]
    return AboutIndex(about_docs.about, language, docs, about_docs.neighbor_map(), pairs)
