"""The offline Jev stand-in: answers exactly what a warm-up missed, per seed, and nothing it cannot read."""
import json
from pathlib import Path
import tempfile
import unittest

from ..runtime import jev_stand_in


def miss(key, questions=3, site='rerank', source='cassette_miss'):
    return {'fields': {'event': 'kmp_judgement', 'site': site, 'source': source, 'status': 'error',
                       'questions': questions, 'request_key': key}}


class StandInTest(unittest.TestCase):
    def test_every_missed_request_is_answered_per_question(self):
        asked = jev_stand_in.misses([miss('k1', 2), miss('k2', 3), miss('k3', source='cassette_hit'),
                                     {'fields': {'event': 'kmp_mcp_tool'}}])
        self.assertEqual(asked, {'k1': 2, 'k2': 3})
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'stand-in.json'
            self.assertEqual(jev_stand_in.write_stand_in(path, asked, seed=1), 2)
            cassette = json.loads(path.read_text())
        self.assertEqual(cassette['schema'], 'kmp.typesafe.cassette.v1')
        self.assertEqual(sorted(cassette['entries']['k2']['answers']), ['p0', 'p1', 'p2'])
        answer = cassette['entries']['k1']['answers']['p0']
        self.assertEqual(answer['type'], 'noul')
        self.assertTrue(0 <= answer['yes'] <= 1)

    def test_seeds_differ_like_samples_and_repeat_like_a_recording(self):
        self.assertEqual(jev_stand_in.answers_for('k', 8, 0), jev_stand_in.answers_for('k', 8, 0))
        self.assertNotEqual(jev_stand_in.answers_for('k', 8, 0), jev_stand_in.answers_for('k', 8, 1))

    def test_a_site_whose_questions_telemetry_cannot_name_is_refused(self):
        with self.assertRaises(jev_stand_in.StandInError):
            jev_stand_in.misses([miss('k', site='curate_review')])
        with self.assertRaises(jev_stand_in.StandInError):
            jev_stand_in.misses([miss(None)])

    def test_only_a_book_record_takes_a_stand_in_and_its_env_points_at_it(self):
        from ..runtime import jev_fixture
        with tempfile.TemporaryDirectory() as folder:
            root, stand_in = Path(folder), Path(folder) / 'stand-in.json'
            fixture = jev_fixture.JevFixture(root, 'k' * 64, 'record', jev_fixture.BOOK,
                                             provider_stand_in=stand_in)
            env = fixture.sample(0, root / 'work').env({'RUST_LOG': 'x'})
            self.assertEqual(env, {'RUST_LOG': 'x', 'KMP_TYPESAFE_CASSETTE': str(stand_in),
                                   'KMP_TYPESAFE_CASSETTE_MODE': 'replay'})
            for mode, backend in (('replay', jev_fixture.BOOK), ('record', jev_fixture.CASSETTE)):
                with self.assertRaises(jev_fixture.JevFixtureError):
                    jev_fixture.JevFixture(root, 'k' * 64, mode, backend, provider_stand_in=stand_in)


if __name__ == '__main__':
    unittest.main()
