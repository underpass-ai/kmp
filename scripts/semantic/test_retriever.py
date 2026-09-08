import hashlib
from pathlib import Path
import tempfile
import unittest
import numpy as np

from retriever import Retriever
from vector_cache import VectorCache
from lexical_ranking import POLICY, SEPARATE_POLICY


class FakeEncoder:
    def __init__(self):
        self.calls = 0

    def encode(self, texts):
        self.calls += len(texts)
        return np.array([[1., 0.] if 'car' in t else [0., 1.] for t in texts])

    def query(self, question):
        return np.array([1., 0.])


def source(ref, text):
    return {'entry_ref': ref, 'text': text, 'text_sha256': hashlib.sha256(text.encode()).hexdigest()}


class RetrieverTests(unittest.TestCase):
    def test_separate_channels_keep_original_ranks_and_reject_cached_scope_leaks(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = VectorCache(Path(directory)/'vectors.sqlite', 'm@r')
            retriever = Retriever(FakeEncoder(), cache, 'm@r', SEPARATE_POLICY)
            query = {'question': 'plane', 'model_revision': retriever.revision, 'top_k': 2,
                     'sources': [source('a', 'car'), source('b', 'plane'), source('b', 'plane')]}
            result, _ = retriever.rank(query)
            self.assertEqual([ref for ref, _ in result['candidates']], ['a', 'b'])
            self.assertEqual([ref for ref, _ in result['lexical_candidates']], ['b'])
            scoped, metrics = retriever.rank({**query, 'sources': query['sources'][:1]})
            self.assertEqual([ref for ref, _ in scoped['candidates']], ['a'])
            self.assertEqual(scoped['lexical_candidates'], [])
            self.assertEqual(metrics['encoded_sources'], 0)
            cache.db.close()

    def test_hybrid_uses_same_encoder_cache_and_cannot_restore_an_absent_source(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = VectorCache(Path(directory)/'vectors.sqlite', 'm@r')
            encoder = FakeEncoder()
            dense = Retriever(encoder, cache, 'm@r')
            query = {'question': 'plane', 'model_revision': 'm@r', 'top_k': 2,
                     'sources': [source('a', 'car'), source('b', 'plane')]}
            self.assertEqual(dense.rank(query)[0]['candidates'][0][0], 'a')
            hybrid = Retriever(encoder, cache, 'm@r', POLICY)
            with self.assertRaises(ValueError):
                hybrid.rank(query)
            result, metrics = hybrid.rank({**query, 'model_revision': hybrid.revision})
            self.assertEqual(result['candidates'][0][0], 'b')
            self.assertEqual(metrics['encoded_sources'], 0)
            scoped, _ = hybrid.rank({**query, 'model_revision': hybrid.revision, 'sources': query['sources'][:1]})
            self.assertEqual([ref for ref, _ in scoped['candidates']], ['a'])
            cache.db.close()

    def test_cached_vectors_never_expand_the_requested_scope_and_survive_restart(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'vectors.sqlite'
            encoder = FakeEncoder()
            cache = VectorCache(path, 'model@one')
            retriever = Retriever(encoder, cache, 'model@one')
            query = {'question': 'vehicle', 'model_revision': 'model@one', 'top_k': 100,
                     'sources': [source('private:a', 'car'), source('public:b', 'plane')]}
            first, metrics = retriever.rank(query)
            self.assertEqual(first['candidates'][0][0], 'private:a')
            self.assertEqual(metrics['encoded_sources'], 2)
            cache.db.close()
            cache = VectorCache(path, 'model@one')
            retriever = Retriever(encoder, cache, 'model@one')
            second, metrics = retriever.rank({**query, 'sources': [source('public:b', 'plane')]})
            self.assertEqual([ref for ref, _ in second['candidates']], ['public:b'])
            self.assertEqual(metrics['encoded_sources'], 0)
            self.assertEqual(encoder.calls, 2)
            cache.db.close()

    def test_text_or_revision_mismatch_cannot_reuse_an_embedding(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = VectorCache(Path(directory)/'vectors.sqlite', 'm@r')
            retriever = Retriever(FakeEncoder(), cache, 'm@r')
            src = source('a', 'car')
            query = {'question': 'q', 'sources': [src], 'model_revision': 'm@r', 'top_k': 20}
            with self.assertRaises(ValueError):
                retriever.rank({**query, 'model_revision': 'other'})
            with self.assertRaises(ValueError):
                retriever.rank({**query, 'sources': [{**src, 'text': 'not car'}]})
            self.assertEqual(retriever.rank({**query, 'sources': []})[0]['candidates'], [])
            cache.db.close()


if __name__ == '__main__':
    unittest.main()
