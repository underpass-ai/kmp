import unittest

from lexical_ranking import POLICY, bm25, fuse, revision
from test_retriever import source


class LexicalRankingTests(unittest.TestCase):
    def test_exact_identifier_beats_repeated_generic_terms_and_duplicates(self):
        sources = [source('generic', 'valve ' * 50), source('specific', 'valve ZX91 is closed')]
        expected = bm25('valve ZX91', sources)
        self.assertEqual(expected[0][0], 'specific')
        self.assertEqual(expected, bm25('valve ZX91', list(reversed(sources)) + sources))
        self.assertEqual(bm25('missing', sources), [])
        self.assertEqual(bm25('query', [source('blank', '')]), [])

    def test_each_channel_votes_once_and_single_channel_order_survives(self):
        a, b, c = ('a', '1'), ('b', '2'), ('c', '3')
        self.assertEqual(fuse([[a, a, b], [b, c]], 3), [b, a, c])
        self.assertEqual(fuse([[c, b, a], []], 2), [c, b])
        self.assertEqual(fuse([[(str(i), str(i)) for i in range(110)]], 110).__len__(), 100)

    def test_policy_is_explicit_in_retrieval_revision(self):
        self.assertEqual(revision('m@r', 'dense'), 'm@r')
        self.assertEqual(revision('m@r', POLICY), 'm@r/bm25-rrf-v1')
        with self.assertRaises(ValueError):
            revision('m@r', 'unknown')
