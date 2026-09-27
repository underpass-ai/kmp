"""refs.normalize and the answer readers match retrieval_kmp_scorecard.rs exactly."""
import json
from pathlib import Path
import re
import unittest

from ..domain import refs

ROOT = Path(__file__).resolve().parents[4]
SCORECARD = ROOT / 'crates/kmp-testkit/src/bin/retrieval_kmp_scorecard.rs'
MEMORY_REF = ROOT / 'crates/kmp-testkit/src/memory_ref.rs'
PARITY = ROOT / 'crates/kmp-testkit/judged/metric_parity.json'


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
        # Evidence nodes cite the entry they are evidence of (decision of 28 Sept 2026).
        ('detail:evidence:project:made:entry:decision:x:current', 'project:made:entry:decision:x'),
        ('detail:evidence:project:made:entry:decision:x:relation:1', 'project:made:entry:decision:x'),
        ('detail:evidence:fixture:lab:entry:observation:y:relation:d115992883596a55',
         'fixture:lab:entry:observation:y'),
        ('detail:evidence:guide:kmp-agent:verb:time', 'guide:kmp-agent:verb:time'),
        ('evidence:project:x:e1:current', 'project:x:e1'),
        ('entry:detail:evidence:project:x:e1:current', 'detail:evidence:project:x:e1:current'),
        ('detail:evidence:x:relation:abc', 'x:relation:abc'),
        ('detail:evidence:x:relation:D115992883596A55', 'x:relation:D115992883596A55'),
        ('detail:evidence::current', 'evidence::current'),
        ('detail:evidence:', 'evidence:'),
    )

    def test_table(self):
        for value, expected in self.CASES:
            with self.subTest(value=value):
                self.assertEqual(refs.normalize(value), expected)

    def test_shared_parity_table(self):
        """`cargo test -p kmp-testkit --test metric_parity` reads the same table for memory_ref.rs."""
        table = json.loads(PARITY.read_text(encoding='utf-8'))['refs']
        self.assertGreaterEqual(len(table), 20)
        for row in table:
            with self.subTest(value=row['value']):
                self.assertEqual(refs.normalize(row['value']), row['normalized'])

    def test_shared_retrieved_table(self):
        table = json.loads(PARITY.read_text(encoding='utf-8'))['retrieved']
        self.assertTrue(table)
        for row in table:
            with self.subTest(ids=row['ids']):
                self.assertEqual(refs.retrieved(row['ids']), row['retrieved'])

    def test_the_rust_scorecard_reads_refs_through_memory_ref(self):
        """Drift guard: the scorecard must not grow its own prefix rule again."""
        scorecard = SCORECARD.read_text(encoding='utf-8')
        self.assertIn('use kmp_testkit::memory_ref::{self, normalize};', scorecard)
        self.assertIn('memory_ref::retrieved(', scorecard)
        self.assertNotIn('strip_prefix("entry:")', scorecard)
        source = MEMORY_REF.read_text(encoding='utf-8')
        for constant in (refs.ENTRY_PREFIX, refs.DETAIL_PREFIX, refs.EVIDENCE_PREFIX, refs.CURRENT_SUFFIX,
                         refs.RELATION_MARK):
            self.assertIn(f'"{constant}"', source)

    def test_canonical_refs(self):
        self.assertTrue(refs.is_canonical('project:x:e1'))
        for value in ('entry:project:x:e1', 'detail:x', 'evidence:project:x:e1:current', '', 'a b', None, 3):
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

    def test_an_entry_and_its_evidence_are_both_retrieved_as_they_arrive(self):
        answer = {'proof': {'evidence': [{'id': 'entry:project:x:entry:decision:a'},
                                         {'id': 'detail:evidence:project:x:entry:decision:b:current'},
                                         {'id': 'detail:evidence:project:x:entry:decision:a:current'},
                                         {'id': 'entry:project:x:entry:decision:b'}]}}
        self.assertEqual(refs.evidence_refs(answer),
                         ['project:x:entry:decision:a', 'project:x:entry:decision:b',
                          'project:x:entry:decision:a', 'project:x:entry:decision:b'])

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
