import hashlib
from pathlib import Path
import tempfile
import unittest
import numpy as np

from retriever import Retriever
from vector_cache import VectorCache


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
