"""refs.normalize and the answer readers match retrieval_kmp_scorecard.rs exactly."""
from pathlib import Path
import re
import unittest

from ..domain import refs

SCORECARD = Path(__file__).resolve().parents[4] / 'crates/kmp-testkit/src/bin/retrieval_kmp_scorecard.rs'


class NormalizeTest(unittest.TestCase):
    CASES = (
        ('entry:project:x:e1', 'project:x:e1'),
        ('detail:project:x:e1', 'project:x:e1'),
        ('project:x:e1', 'project:x:e1'),
        # One prefix goes, never both, in the Rust order: entry first.
        ('entry:detail:project:x:e1', 'detail:project:x:e1'),
        ('detail:entry:project:x:e1', 'entry:project:x:e1'),
        ('entry:entry:x', 'entry:x'),
        ('entry:', ''),
        ('detail:', ''),
        ('', ''),
        ('Entry:x', 'Entry:x'),  # case-sensitive, like str::strip_prefix
        (' entry:x', ' entry:x'),
        ('detail:evidence:project:made:entry:decision:x', 'evidence:project:made:entry:decision:x'),
    )

    def test_table(self):
        for value, expected in self.CASES:
            with self.subTest(value=value):
                self.assertEqual(refs.normalize(value), expected)

    def test_rust_source_still_strips_the_same_prefixes_in_the_same_order(self):
        """Drift guard: if strip_prefix changes in Rust, this port must change with it."""
        source = SCORECARD.read_text(encoding='utf-8')
        body = re.search(r'fn strip_prefix\(value: &str\) -> String \{(.*?)\n\}', source, re.S)
        self.assertIsNotNone(body, 'strip_prefix moved or changed signature')
        prefixes = re.findall(r'strip_prefix\("([^"]+)"\)', body.group(1))
        self.assertEqual(prefixes, [refs.ENTRY_PREFIX, refs.DETAIL_PREFIX])
        self.assertIn('.or_else(', body.group(1))
        self.assertIn('.unwrap_or(value)', body.group(1))

    def test_canonical_refs(self):
        self.assertTrue(refs.is_canonical('project:x:e1'))
        for value in ('entry:project:x:e1', 'detail:x', '', 'a b', None, 3):
            self.assertFalse(refs.is_canonical(value), value)

    def test_ownership(self):
        self.assertTrue(refs.owned_by('synth:mono-a000:e0000001', 'synth:mono-a000'))
        self.assertFalse(refs.owned_by('synth:mono-a0001:e1', 'synth:mono-a000'))
        self.assertFalse(refs.owned_by('synth:mono-a000:', 'synth:mono-a000'))


class AnswerReadingTest(unittest.TestCase):
    ANSWER = {
        'answer': 'project:x:e2',
        'because': [{'ref': 'entry:project:x:e2'}, {'ref': 'project:x:e1'},
                    {'ref': 'detail:project:x:e2'}, {'nope': 1}, {'ref': 7}],
        'proof': {'evidence': [{'id': 'entry:project:x:e2'}, {'id': 'detail:project:x:e9'},
                               {'id': None}, 'not-an-object', {'id': 'project:x:e1'}],
                  'nearest_outside': None},
    }

    def test_retrieved_keeps_response_order_and_skips_non_strings(self):
        self.assertEqual(refs.evidence_refs(self.ANSWER), ['project:x:e2', 'project:x:e9', 'project:x:e1'])

    def test_cited_is_a_sorted_set(self):
        self.assertEqual(refs.cited_refs(self.ANSWER), ('project:x:e1', 'project:x:e2'))

    def test_missing_or_malformed_sections_read_as_empty(self):
        for value in ({}, None, [], {'proof': []}, {'proof': {'evidence': {}}}, {'because': 'x'}):
            self.assertEqual(refs.evidence_refs(value), [])
            self.assertEqual(refs.cited_refs(value), ())
            self.assertFalse(refs.is_unknown(value))

    def test_unknown_and_nearest_outside(self):
        answer = {'answer': 'UNKNOWN', 'proof': {'nearest_outside': {'ref': 'project:n:valve'}}}
        self.assertTrue(refs.is_unknown(answer))
        self.assertEqual(refs.nearest_outside_ref(answer), 'project:n:valve')
        self.assertIsNone(refs.nearest_outside_ref(self.ANSWER))


if __name__ == '__main__':
    unittest.main()
