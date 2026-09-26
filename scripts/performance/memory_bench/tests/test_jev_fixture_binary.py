"""BT11 on the real binary: a `book` fixture recorded against a stand-in provider replays offline.

Runs only with `KMP_BENCH_BINARY=<kmp-mcp with the P5 verdict book>` (for
example `target/release/kmp-mcp`); skipped otherwise. No test reaches Jev: a
warm-up process names the rerank requests behind an empty cassette, a
stand-in cassette answers them in the provider's place (`jev_stand_in`), the
record sample fills the book behind it, and the replay sample answers from
the copied book with an empty cassette behind it — so a verdict missing from
the book would fail instead of reaching the network.
"""
import itertools
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

from ..runtime import jev_fixture, jev_stand_in, shared_store
from ..runtime.jev_fixture import BOOK, JevFixture
from ..runtime.session import BenchProcess, ProcessConfig

BINARY = os.environ.get('KMP_BENCH_BINARY')
ABOUT = 'project:bt11'
TYPESAFE = ('{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.13.0",'
            '"timeout_ms":20000}')
FACTS = [('g1', 'The gateway that fronts every public endpoint is Envoy, chosen in March.'),
         ('g2', 'Ana mapped the tumi and roro inboxes into one shared queue.'),
         ('g3', 'Backups of the billing database run nightly at 02:00 UTC.'),
         ('g4', 'The mobile app switched its font to Inter for legibility.')]
QUESTIONS = ('Which gateway fronts every public endpoint?', 'Who merged the two inboxes?',
             'When do billing backups run?')


def memory():
    entries = [{'id': f'{ABOUT}:{ref}', 'kind': 'decision', 'text': text,
                'coordinates': [{'dimension': 'work', 'scope_id': 'work:main',
                                 'occurred_at': '2026-09-01T00:00:00Z',
                                 'valid_from': '2026-09-01T00:00:00Z', 'sequence': n + 1}]}
               for n, (ref, text) in enumerate(FACTS)]
    return {'dimensions': [{'id': 'work:main', 'kind': 'work'}], 'entries': entries, 'relations': []}


def ask(call, question):
    reply = call('kmp_ask', {'about': ABOUT, 'question': question,
                             'answer_policy': 'evidence_or_unknown'})
    return reply['result']['structuredContent']


class Binary:
    def __init__(self, path):
        self.path = Path(path).resolve()
        self.label = 'candidate'


@unittest.skipUnless(BINARY, 'set KMP_BENCH_BINARY to a kmp-mcp built with the verdict book')
class BookFixtureOnTheBinaryTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        self.work = self.root / 'work'
        self.binary = Binary(BINARY)
        self.template = self._template()

    def _store(self, label, env=None, template=None):
        store = shared_store.SharedStore(self.work, self.root / 'out' / label, label,
                                         template=template, env=env)
        self.addCleanup(store.close)
        return store

    def _template(self):
        """A seeded data directory with Jev and ask re-ranking opted in."""
        seed = self._store('seed')
        server = seed.open(self.binary, 'seed')
        reply = server.call('kmp_ingest', {'about': ABOUT, 'idempotency_key': 'bt11:seed',
                                           'memory': memory()})
        server.close()
        self.assertTrue(reply.ok, reply.error)
        template = self.root / 'template'
        shutil.copytree(seed.data_dir, template, ignore=shutil.ignore_patterns('logs'))
        (template / 'typesafe.json').write_text(TYPESAFE)
        (template / 'rerank.json').write_text('{"pool_size":40}')
        return template

    def _stand_in(self):
        """Warm-up behind an empty cassette, then a stand-in answering what it asked."""
        empty = self.root / 'empty.cassette.json'
        jev_fixture.empty_cassette(empty, jev_fixture.DEFAULT_MODEL)
        warm = self._store('warm-up', env={jev_fixture.CASSETTE_ENV: str(empty),
                                           jev_fixture.CASSETTE_MODE_ENV: 'replay',
                                           'RUST_LOG': 'kmp_mcp::judgement=debug'},
                           template=self.template)
        server = warm.open(self.binary, 'warm-up')
        for question in QUESTIONS:
            server.call('kmp_ask', {'about': ABOUT, 'question': question,
                                    'answer_policy': 'evidence_or_unknown'})
        events = server.close().log_events
        # Every judgement missed: the warm-up's book holds no verdict.
        self.assertEqual(warm.query(jev_fixture.BOOK_FILE, 'SELECT COUNT(*) FROM verdicts'), [(0,)])
        asked = jev_stand_in.misses(events)
        self.assertEqual(len(asked), len(QUESTIONS), events)
        stand_in = self.root / 'stand-in.cassette.json'
        jev_stand_in.write_stand_in(stand_in, asked, seed=3)
        return stand_in

    def _run(self, sample, label):
        config = ProcessConfig(self.binary.path, self.work, label, env=sample.env({}),
                               template=self.template, probe_resources=False,
                               store_hooks=(sample,))
        out = self.root / 'out' / label
        out.mkdir(parents=True, exist_ok=True)
        process = BenchProcess(config, out, 0, itertools.count(1))
        with process.journey() as tap:
            answers = [ask(tap.call, question) for question in QUESTIONS]
        process.close()
        return answers

    def test_a_book_recorded_behind_a_stand_in_replays_without_the_network(self):
        stand_in = self._stand_in()
        cache = self.root / 'cache' / 'jev' / ('b' * 64)
        record = JevFixture(cache, 'b' * 64, 'record', BOOK, provider_stand_in=stand_in)
        sample = record.sample(0, self.root / 'sample-record')
        recorded = self._run(sample, 'record')
        outcome = sample.seal()
        self.assertEqual((outcome.status, outcome.by_source),
                         ('recorded', {'cassette_hit': len(QUESTIONS)}), outcome)
        self.assertTrue((record.sample_dir(0) / jev_fixture.BOOK_FILE).exists())
        for answer in recorded:
            self.assertFalse(any('rerank unavailable' in w for w in answer.get('warnings', [])),
                             answer.get('warnings'))

        replay = JevFixture(cache, 'b' * 64, 'replay', BOOK)
        sample = replay.sample(0, self.root / 'sample-replay')
        replayed = self._run(sample, 'replay')
        outcome = sample.seal()
        self.assertEqual((outcome.status, outcome.by_source),
                         ('complete', {'book_hit': len(QUESTIONS)}), outcome)
        self.assertEqual(json.dumps(replayed, sort_keys=True), json.dumps(recorded, sort_keys=True),
                         'the replay reads the recorded answers byte for byte')

    def test_a_store_without_typesafe_json_never_gets_a_book(self):
        plain = self.root / 'plain'
        shutil.copytree(self.template, plain)
        for name in ('typesafe.json', 'rerank.json'):
            (plain / name).unlink()
        store = self._store('plain', template=plain)
        server = store.open(self.binary, 'plain')
        for question in QUESTIONS:
            server.call('kmp_ask', {'about': ABOUT, 'question': question,
                                    'answer_policy': 'evidence_or_unknown'})
        server.close()
        self.assertEqual(sorted(p.name for p in store.data_dir.glob('judgements.sqlite3*')), [])


if __name__ == '__main__':
    unittest.main()
