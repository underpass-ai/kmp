import copy
import unittest

from contracts import VERSION, candidates
from segment import segment_session
from session_plan import compile_session


def inputs():
    session = {'about': 'eval:session-contract', 'session_id': 'one-session', 'sources': [
        {'id': str(i), 'role': 'user', 'text': text, 'observed_at': '2026-01-01T00:00:00Z'}
        for i, text in enumerate(['Name.', 'Pick.', 'Count.'])]}
    attempts = []
    for row in segment_session(session, target_chars=11)['episodes']:
        source = row['episode']['sources'][-1]
        extracted = {'memories': [{'text': source['text'], 'category': 'fact', 'citations': [
            {'source_id': source['id'], 'quote': source['text'], 'why': 'Synthetic exact-quotation contract control.'}]}]}
        proposed, _ = candidates(extracted, row['episode'])
        attempts.append({'episode_id': row['episode']['episode_id'], 'status': 'completed',
            'version': VERSION, 'observed_at': '2026-01-02T00:00:00Z', 'model_revision': 'fixture@1',
            'extracted': extracted, 'verified': {'verdicts': [
                {'id': p['id'], 'supported': True, 'reason': 'Synthetic supplied verdict, not model quality.'} for p in proposed]}})
    return session, attempts


class SessionPlanTests(unittest.TestCase):
    def test_overlap_originals_are_identical_once_in_both_variants(self):
        source, attempts = inputs()
        before = copy.deepcopy((source, attempts))
        result = compile_session(source, attempts, target_chars=11)
        raw = result['baseline']['memory']['entries']
        formed = result['formed']['memory']
        self.assertEqual(len(raw), 3)
        self.assertEqual(formed['entries'][:3], raw)
        self.assertEqual(len(formed['entries']), 5)
        self.assertEqual([e['text'] for e in raw], [s['text'] for s in source['sources']])
        originals = {e['id'] for e in raw}
        self.assertEqual(len({e['id'] for e in formed['entries']}), 5)
        self.assertTrue(all(r['to'] in originals for r in formed['relations']))
        self.assertTrue(all(e['source'] in originals for e in formed['evidence']))
        self.assertEqual(before, (source, attempts))
        reversed_result = compile_session(source, attempts[::-1], target_chars=11)
        self.assertEqual(reversed_result['formed'], result['formed'])

    def test_failed_episode_remains_in_denominator_and_originals_survive(self):
        source, attempts = inputs()
        baseline = compile_session(source, attempts, target_chars=11)['baseline']
        attempts[0] = {k: v for k, v in attempts[0].items() if k in ('episode_id', 'observed_at', 'model_revision')}
        attempts[0].update(status='failed', error='incomplete generation: length')
        result = compile_session(source, attempts, target_chars=11)
        self.assertEqual(result['status'], 'partial')
        self.assertEqual(result['failed_episodes'], 1)
        self.assertEqual(len(result['results']), 2)
        self.assertEqual(result['baseline'], baseline)
        self.assertEqual(result['formed']['memory']['entries'][:3], baseline['memory']['entries'])

    def test_missing_duplicate_foreign_version_and_incomplete_verdict_refused(self):
        source, attempts = inputs()
        variants = [attempts[:1], [attempts[0], attempts[0]]]
        for mutate in ('version', 'verified', 'episode_id'):
            changed = copy.deepcopy(attempts)
            changed[0][mutate] = {'verdicts': []} if mutate == 'verified' else 'foreign'
            variants.append(changed)
        for invalid in variants:
            with self.assertRaises(ValueError):
                compile_session(source, invalid, target_chars=11)


if __name__ == '__main__':
    unittest.main()
