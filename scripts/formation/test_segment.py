"""Lossless boundaries and isolation for long-session preparation."""
import copy
import unittest

from contracts import episode
from segment import segment_session


def session(texts):
    return {'about': 'project:test', 'session_id': 'opaque-session', 'sources': [
        {'id': str(i), 'role': 'user' if i % 2 == 0 else 'assistant', 'text': text,
         'observed_at': '2026-01-01T00:00:00Z'} for i, text in enumerate(texts)]}


class SegmentTests(unittest.TestCase):
    def test_full_text_reconstructs_once_despite_overlap_and_blanks(self):
        source = session([' Aé🙂 ', 'B\nB', '', '   ', 'C C ', 'D D', 'E\tE'])
        before = copy.deepcopy(source)
        plan = segment_session(source, target_chars=11, max_sources=3)
        originals = {s['id']: s for s in source['sources']}
        new = []
        for row in plan['episodes']:
            episode(row['episode'])
            for turn in row['episode']['sources']:
                self.assertEqual(turn, originals[turn['id']])
            new.extend(row['new_source_ids'])
        self.assertEqual(new, ['0', '1', '4', '5', '6'])
        self.assertEqual(plan['excluded_blank_source_ids'], ['2', '3'])
        self.assertTrue(any(r['context_source_ids'] for r in plan['episodes'][1:]))
        self.assertEqual(source, before)
        self.assertEqual(plan, segment_session(source, target_chars=11, max_sources=3))

    def test_oversize_turn_is_intact_and_context_omission_explicit(self):
        source = session(['a' * 4, 'b' * 16, 'c' * 4])
        rows = segment_session(source, target_chars=8)['episodes']
        self.assertEqual([r['new_source_ids'] for r in rows], [['0'], ['1'], ['2']])
        self.assertEqual(rows[1]['episode']['sources'][0]['text'], 'b' * 16)
        self.assertTrue(rows[1]['whole_turn_exceeds_target'])
        self.assertEqual(rows[1]['context_omitted'], '0')
        self.assertEqual(rows[2]['context_omitted'], '1')
        with self.assertRaises(ValueError):
            segment_session(session(['x' * 24001]))

    def test_no_cross_session_future_context_or_duplicate_identity(self):
        for field, value in [('observed_at', '2026-01-02T00:00:00Z'), ('id', '0')]:
            source = session(['earlier fact', 'later correction'])
            source['sources'][1][field] = value
            with self.assertRaises(ValueError):
                segment_session(source)
        source = session(['same clock', 'same clock with offset'])
        source['sources'][1]['observed_at'] = '2026-01-01T01:00:00+01:00'
        self.assertEqual(len(segment_session(source)['episodes']), 1)

    def test_rejects_oracle_fields_and_invalid_policy(self):
        for field in ('question', 'answer', 'has_answer', 'review_criteria'):
            source = session(['fact'])
            source[field] = 'forbidden'
            with self.assertRaises(ValueError):
                segment_session(source)
            source = session(['fact'])
            source['sources'][0][field] = 'forbidden'
            with self.assertRaises(ValueError):
                segment_session(source)
        for policy in ({'target_chars': 24001}, {'target_chars': True}, {'max_sources': 0}, {'overlap': 1}):
            with self.assertRaises(ValueError):
                segment_session(session(['fact']), **policy)

    def test_count_limit_empty_session_and_policy_identity(self):
        source = session(['a'] * 7)
        one = segment_session(source, max_sources=1)
        self.assertEqual(len(one['episodes']), 7)
        self.assertFalse(any(r['context_source_ids'] for r in one['episodes']))
        no_overlap = segment_session(source, max_sources=2, overlap=False)
        self.assertEqual(len(no_overlap['episodes']), 4)
        self.assertNotEqual(one['episodes'][0]['episode']['episode_id'], no_overlap['episodes'][0]['episode']['episode_id'])
        self.assertEqual(segment_session(session([]))['episodes'], [])
        self.assertEqual(segment_session(session(['  ']))['episodes'], [])


if __name__ == '__main__':
    unittest.main()
