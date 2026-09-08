"""Fixed BM25 and RRF over the caller's admitted snapshot, without gold labels."""
from collections import Counter
import math
import re

POLICY = 'bm25-rrf-v1'
SEPARATE_POLICY = 'separate-bm25-v1'
K1, B, RRF_CONSTANT, WINDOW = 1.5, 0.75, 60, 100


def bm25(question, sources):
    # Repeated transport rows must not change corpus statistics or add votes.
    unique = {(s['entry_ref'], s['text_sha256']): s['text'] for s in sources}
    terms = {key: Counter(re.findall(r'\w+', text.lower())) for key, text in unique.items()}
    if not terms:
        return []
    lengths = {key: sum(t.values()) for key, t in terms.items()}
    average = sum(lengths.values()) / len(terms)
    if not average:
        return []
    df = Counter(word for row in terms.values() for word in row)
    query = sorted(set(re.findall(r'\w+', question.lower())))
    scored = []
    for key, row in terms.items():
        score = 0.0
        for word in query:
            tf = row[word]
            if tf:
                idf = math.log1p((len(terms) - df[word] + 0.5) / (df[word] + 0.5))
                score += idf * tf * (K1 + 1) / (tf + K1 * (1 - B + B * lengths[key] / average))
        if score > 0:
            scored.append((score, key))
    return [key for _, key in sorted(scored, key=lambda x: (-x[0], x[1]))]


def fuse(rankings, top_k):
    scores, fingerprints = {}, {}
    for ranking in rankings:
        seen = set()
        for ref, fingerprint in ranking:
            if ref in seen:
                continue
            if len(seen) == WINDOW:
                break
            seen.add(ref)
            scores[ref] = scores.get(ref, 0.0) + 1 / (RRF_CONSTANT + len(seen))
            # All tuples came from this snapshot. Keep the first channel's
            # fingerprint if several stored text variants share an entry ref.
            fingerprints.setdefault(ref, fingerprint)
    return [(ref, fingerprints[ref]) for ref in sorted(scores, key=lambda ref: (-scores[ref], ref))[:top_k]]


def revision(encoder_revision, policy):
    if policy not in ('dense', POLICY, SEPARATE_POLICY):
        raise ValueError('unknown ranking policy')
    return encoder_revision if policy == 'dense' else encoder_revision + '/' + policy


def unique_refs(ranking, top_k):
    result, seen = [], set()
    for ref, fingerprint in ranking:
        if ref not in seen:
            seen.add(ref)
            result.append((ref, fingerprint))
        if len(result) == top_k:
            break
    return result
