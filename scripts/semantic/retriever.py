"""Rank only the explicit source snapshot submitted by KMP."""
import hashlib
import numpy as np


class Retriever:
    def __init__(self, encoder, cache, revision):
        self.encoder, self.cache, self.revision = encoder, cache, revision

    def rank(self, request):
        if set(request) != {'question', 'sources', 'model_revision', 'top_k'}:
            raise ValueError('invalid request fields')
        if request['model_revision'] != self.revision:
            raise ValueError('encoder revision mismatch')
        question, sources, top_k = request['question'], request['sources'], request['top_k']
        if not isinstance(question, str) or not question.strip() or not isinstance(sources, list) or not 1 <= top_k <= 100:
            raise ValueError('invalid query or limits')
        unique = {}
        for source in sources:
            if set(source) != {'entry_ref', 'text', 'text_sha256'} or not isinstance(source['entry_ref'], str) or not source['entry_ref']:
                raise ValueError('invalid source identity')
            if not isinstance(source['text'], str) or hashlib.sha256(source['text'].encode()).hexdigest() != source['text_sha256']:
                raise ValueError('source fingerprint mismatch')
            unique[source['text_sha256']] = source['text']
        vectors, missing = {}, []
        for fingerprint, text in unique.items():
            vector = self.cache.get(fingerprint)
            if vector is None:
                missing.append((fingerprint, text))
            else:
                vectors[fingerprint] = vector
        if missing:
            encoded = self.encoder.encode([text for _, text in missing])
            pairs = list(zip([fp for fp, _ in missing], encoded))
            self.cache.put_many(pairs)
            vectors.update(pairs)
        candidates = []
        if sources:
            query = self.encoder.query(question)
            for source in sources:
                score = float(vectors[source['text_sha256']] @ query)
                if not np.isfinite(score):
                    raise ValueError('non-finite similarity')
                candidates.append((score, source['entry_ref'], source['text_sha256']))
        candidates.sort(key=lambda row: (-row[0], row[1], row[2]))
        ranked, seen = [], set()
        for _, ref, fingerprint in candidates:
            if ref not in seen:
                seen.add(ref)
                ranked.append((ref, fingerprint))
            if len(ranked) == top_k:
                break
        return {'model_revision': self.revision,
                'question_sha256': hashlib.sha256(question.encode()).hexdigest(), 'candidates': ranked}, {
                'sources': len(sources), 'encoded_sources': len(missing), 'cache_hits': len(unique)-len(missing)}
