"""BT11: Jev fixtures (one per sample) and the judgement log they are checked against.

No test reaches Jev: record mode runs against `FakeJev`, a provider whose
answers vary between samples like a real model's. `FakeKmp` stands in for the
binary's judgement path (verdict book in front of the per-body cassette in
front of the provider, cassette_judgement.rs semantics) and writes BT03
`kmp_judgement` lines to its journal, so the fixtures are checked through the
same journal parse the bench uses. The book is a stand-in SQLite file: the
fixture copies it as opaque bytes, so its schema does not matter here.
"""
import hashlib
import itertools
import json
import os
from pathlib import Path
import random
import shutil
import sqlite3
import statistics
import sys
import tempfile
import unittest

from ..domain.variant import parse_variant
from ..runtime import jev_fixture, judgement_log, server_log
from ..runtime.jev_fixture import (BOOK, CASSETTE, JevFixtureError, JevFixtureMissing, fixture_key,
                                   judged_config, open_fixture, sample_work_dir)
from ..runtime.layout import public_layout
from ..runtime.session import BenchProcess, ProcessConfig

MODEL = 'jev-1.13.0'
HEX = {name: hashlib.sha256(name.encode()).hexdigest() for name in ('binary', 'store', 'questions')}


def journal(*fields):
    return [json.dumps({'timestamp': 't', 'level': 'DEBUG', 'target': 'kmp_mcp', 'fields': f})
            for f in fields]


def judged(key, source='cassette_hit', status='ok', site='rerank'):
    line = {'event': 'kmp_judgement', 'site': site, 'model': MODEL, 'source': source, 'status': status,
            'questions': 1, 'request_key': key, 'elapsed_us': 10}
    if status == 'ok':
        line.update(answers=1, requests=1, http_requests=0, input_tokens=100)
    else:
        line['error_hash'] = 'ab12cd34ab12cd34'
    return line


def tool(move='kmp_ask', us=5):
    line = {'event': 'kmp_mcp_tool', 'kmp_move': move, 'status': 'success', 'duration_ms': 1}
    if us is not None:
        line['duration_us'] = us
    return line


class NetworkBlocked(Exception):
    pass


class FakeJev:
    """The provider. `seed` stands for the model's sampling: another sample, other answers."""

    def __init__(self, seed):
        self.seed, self.calls = seed, 0

    def __call__(self, key):
        self.calls += 1
        return 'yes' if random.Random(f'{self.seed}:{key}').random() < 0.5 else 'no'


class FakeKmp:
    """One kmp-mcp process: book (if the binary has one) -> cassette (if named) -> provider."""

    def __init__(self, book, scratch, telemetry=True):
        self.book, self.scratch, self.telemetry = book, scratch, telemetry

    def run(self, sample, variant_env, questions, jev, secrets=None):
        sample.check_secrets(secrets)
        env = sample.env(variant_env)
        data = Path(tempfile.mkdtemp(dir=self.scratch))
        try:
            sample.prepare_store(data)
            cursor = server_log.LogCursor(data)
            (data / 'logs').mkdir()
            self.journal = (data / 'logs' / 'kmp-mcp.log.2026-09-26').open('a')
            answers = [self._ask(q, env, jev, data) for q in questions]
            self.journal.close()
            sample.harvest_store(data, cursor.read_new())
            return answers
        finally:
            shutil.rmtree(data)

    def _log(self, fields):
        if self.telemetry or fields['event'] != 'kmp_judgement':
            if not self.telemetry:
                fields = {k: v for k, v in fields.items() if k != 'duration_us'}
            self.journal.write(json.dumps({'fields': fields}) + '\n')

    def _ask(self, question, env, jev, data):
        key = hashlib.sha256(json.dumps({'model': MODEL, 'q': question}).encode()).hexdigest()
        answer, source = self._book_get(data, key), 'book_hit'
        if answer is None:
            answer, source = self._inner(key, env, jev)
            if answer is not None and self.book:
                self._book_put(data, key, answer)
        line = {'event': 'kmp_judgement', 'site': 'rerank', 'model': MODEL, 'source': source,
                'status': 'ok' if answer else 'error', 'questions': 1, 'request_key': key, 'elapsed_us': 3}
        line.update({'requests': 1, 'input_tokens': 50} if answer else {'error_hash': key[:16]})
        self._log(line)
        self._log(tool())
        return answer

    def _inner(self, key, env, jev):
        path = env.get('KMP_TYPESAFE_CASSETTE')
        if path is None:
            return self._remote(key, jev), 'remote'
        path = Path(path)
        document = json.loads(path.read_text()) if path.exists() else {
            'schema': 'kmp.typesafe.cassette.v1', 'model': MODEL, 'entries': {}}
        if key in document['entries']:
            return document['entries'][key]['answer'], 'cassette_hit'
        if env['KMP_TYPESAFE_CASSETTE_MODE'] == 'replay':
            return None, 'cassette_miss'
        document['entries'][key] = {'answer': self._remote(key, jev)}
        path.write_text(json.dumps(document))
        return document['entries'][key]['answer'], 'cassette_miss'

    @staticmethod
    def _remote(key, jev):
        if jev is None:
            raise NetworkBlocked(key)
        return jev(key)

    def _book_get(self, data, key):
        if not self.book or not (data / jev_fixture.BOOK_FILE).exists():
            return None
        with sqlite3.connect(data / jev_fixture.BOOK_FILE) as db:
            row = db.execute('select answer from verdicts where key = ?', (key,)).fetchone()
        return row[0] if row else None

    def _book_put(self, data, key, answer):
        db = sqlite3.connect(data / jev_fixture.BOOK_FILE)
        with db:
            db.execute('create table if not exists verdicts (key text primary key, answer text)')
            db.execute('insert into verdicts values (?, ?)', (key, answer))
        db.close()


def variant(jev='replay', env=None, **extra):
    data = {'name': 'jev-arm', 'claim': 'quality', 'n': 1, 'store': 'shared', 'jev': jev,
            'targets': [{'metric': 'useful_rate', 'direction': 'up', 'effect': 0.1}], 'guards': [],
            'binary': {'path': 'fake'},
            'store_files': {'typesafe.json': {'json': {'model': MODEL}},
                            'rerank.json': {'json': {'pool_size': 40}}},
            'env': env or {}, **extra}
    return parse_variant(data, 'v')


def yes_rate(answers):
    return sum(a == 'yes' for a in answers) / len(answers)


QUESTIONS = tuple(f'question {i}' for i in range(40))


class JudgementLogTest(unittest.TestCase):
    def test_bt03_lines_keep_request_key_model_and_failures(self):
        lines = journal(judged('k1'), judged('k2', 'cassette_miss', 'error'), tool(),
                        {'event': 'kmp_store_config_summary'})
        log = judgement_log.parse(lines + ['not json', '{"fields": 3}'])
        self.assertTrue(log.telemetry_present)
        self.assertEqual([line.request_key for line in log.lines], ['k1', 'k2'])
        self.assertEqual((log.lines[0].model, log.lines[0].input_tokens), (MODEL, 100))
        self.assertIsNone(log.lines[1].input_tokens)
        self.assertEqual(log.lines[1].error_hash, 'ab12cd34ab12cd34')
        self.assertEqual(log.by_source(), {'cassette_hit': 1, 'error': 1})
        self.assertEqual([line.request_key for line in log.asked_provider()], ['k2'])
        self.assertEqual(log.request_keys(), frozenset({'k1', 'k2'}))

    def test_a_binary_without_telemetry_is_tolerated_and_named(self):
        old = judgement_log.parse(journal(tool(us=None), tool(us=None)))
        self.assertFalse(old.telemetry_present)
        self.assertEqual((old.lines, old.absent_reason), ((), server_log.NO_JUDGEMENT))
        self.assertFalse(judgement_log.parse([]).telemetry_present)
        quiet = judgement_log.parse(journal(tool()))
        self.assertTrue(quiet.telemetry_present)  # BT03 binary that judged nothing
        self.assertEqual(quiet.by_source(), {})

    def test_reuses_an_existing_server_log_parse_and_merges_processes(self):
        lines = journal(judged('k1', 'book_hit'), tool())
        log = judgement_log.parse(lines, server_log.parse(lines))
        merged = judgement_log.merge([log, judgement_log.parse(journal(tool(us=None)))])
        self.assertTrue(merged.telemetry_present)
        self.assertEqual(merged.by_source(), {'book_hit': 1})
        self.assertFalse(judgement_log.merge([judgement_log.parse([])]).telemetry_present)


class FixtureKeyTest(unittest.TestCase):
    def key(self, v, backend=CASSETTE):
        return fixture_key(binary_sha256=HEX['binary'], judged=judged_config(v), store_key=HEX['store'],
                           questions_digest=HEX['questions'], backend=backend)

    def test_record_and_replay_of_one_arm_share_a_key(self):
        record = variant('record', {'KMP_TYPESAFE_CASSETTE': '/nonexistent/c.json',
                                    'KMP_TYPESAFE_CASSETTE_MODE': 'record', 'RUST_LOG': 'kmp_mcp=debug'})
        self.assertEqual(self.key(record), self.key(variant('replay')))
        self.assertNotEqual(self.key(record), self.key(record, BOOK))
        other = variant(store_files={'typesafe.json': {'json': {'model': MODEL}}})
        self.assertNotEqual(self.key(other), self.key(variant()))
        with self.assertRaises(JevFixtureError):
            self.key(record, 'sqlite')


class FixtureCase(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        self.layout = public_layout(self.root / 'cache')
        self.key = hashlib.sha256(b'fixture').hexdigest()

    def fixture(self, mode, backend, variant_env=None):
        return open_fixture(self.layout, self.key, mode, backend, MODEL, variant_env)

    def sample(self, mode, backend, index, variant_env=None):
        work = sample_work_dir(self.layout, self.key, index)
        return self.fixture(mode, backend, variant_env).sample(index, work)

    def record(self, backend, samples=jev_fixture.DEFAULT_SAMPLES, processes=1):
        """Record `samples` samples, Jev varying between them; returns (rates, outcomes, jevs)."""
        binary, rates, outcomes, jevs = FakeKmp(backend == BOOK, self.root), [], [], []
        for index in range(samples):
            sample, jev = self.sample('record', backend, index), FakeJev(seed=index)
            answers = []
            for part in range(processes):
                answers += binary.run(sample, {}, QUESTIONS[part::processes], jev,
                                      secrets={'TYPESAFE_API_KEY': 'fake'})
            rates.append(yes_rate(answers))
            outcomes.append(sample.seal())
            jevs.append(jev)
        return rates, outcomes, jevs

    def replay(self, backend, index, variant_env=None, telemetry=True):
        sample = self.sample('replay', backend, index, variant_env)
        answers = FakeKmp(backend == BOOK, self.root, telemetry).run(sample, variant_env, QUESTIONS, jev=None)
        return answers, sample.seal()


class RecordReplayTest(FixtureCase):
    def check_backend(self, backend, recorded_source):
        rates, outcomes, jevs = self.record(backend)
        self.assertGreater(statistics.pvariance(rates), 0.0)
        self.assertEqual([o.status for o in outcomes], ['recorded'] * 3)
        for outcome, jev in zip(outcomes, jevs):  # every sample started empty: Jev asked for all
            self.assertEqual(outcome.by_source, {recorded_source: len(QUESTIONS)})
            self.assertEqual(jev.calls, len(QUESTIONS))
        for index, rate in enumerate(rates):
            answers, outcome = self.replay(backend, index)
            self.assertEqual(yes_rate(answers), rate)  # each sample replays its own verdicts
            self.assertEqual((outcome.status, outcome.missing), ('complete', ()))
        manifest = json.loads((self.fixture('replay', backend).sample_dir(1) / 'fixture.json').read_text())
        self.assertEqual((manifest['sample'], manifest['backend'], len(manifest['request_keys'])),
                         (1, backend, len(QUESTIONS)))

    def test_book_samples_record_from_empty_books_and_replay_offline(self):
        self.check_backend(BOOK, 'remote')

    def test_cassette_samples_record_from_empty_cassettes_and_replay_offline(self):
        self.check_backend(CASSETTE, 'cassette_miss')

    def test_one_shared_book_would_flatten_the_samples(self):
        """Why a book per sample: carried over, sample 1's verdicts answer samples 2 and 3."""
        binary, rates = FakeKmp(True, self.root), []
        for index in range(3):
            sample = self.sample('record', BOOK, index)
            if index:
                jev_fixture._move_book(self.fixture('record', BOOK).sample_dir(0), sample.work)
            rates.append(yes_rate(binary.run(sample, {}, QUESTIONS, FakeJev(index))))
            sample.seal()
        self.assertEqual(statistics.pvariance(rates), 0.0)

    def test_a_sample_keeps_one_book_across_process_restarts(self):
        rates, outcomes, jevs = self.record(BOOK, samples=1, processes=2)
        self.assertEqual(outcomes[0].status, 'recorded')
        answers, outcome = self.replay(BOOK, 0)
        self.assertEqual((yes_rate(answers), outcome.by_source), (rates[0], {'book_hit': len(QUESTIONS)}))

    def test_a_verdict_missing_from_the_book_is_no_comparable_not_a_guess(self):
        self.record(BOOK, samples=1)
        book = self.fixture('replay', BOOK).sample_dir(0) / jev_fixture.BOOK_FILE
        with sqlite3.connect(book) as db:
            db.execute("delete from verdicts where rowid = 1")
        db.close()
        answers, outcome = self.replay(BOOK, 0)
        self.assertEqual(answers.count(None), 1)  # the empty replay cassette refused it: no network
        self.assertEqual(outcome.status, 'no_comparable')
        self.assertFalse(outcome.comparable)
        self.assertEqual(outcome.missing[0][1], 'error')
        self.assertEqual(outcome.as_dict()['by_source'], {'book_hit': len(QUESTIONS) - 1, 'error': 1})

    def test_a_cassette_miss_is_no_comparable(self):
        self.record(CASSETTE, samples=1)
        path = self.fixture('replay', CASSETTE).sample_dir(0) / jev_fixture.CASSETTE_FILE
        document = json.loads(path.read_text())
        document['entries'].pop(sorted(document['entries'])[0])
        path.write_text(json.dumps(document))
        _, outcome = self.replay(CASSETTE, 0)
        self.assertEqual((outcome.status, len(outcome.missing)), ('no_comparable', 1))

    def test_replay_without_telemetry_is_unverified(self):
        self.record(CASSETTE, samples=1)
        _, outcome = self.replay(CASSETTE, 0, telemetry=False)
        self.assertEqual(outcome.status, 'unverified')
        self.assertIn(server_log.NO_JUDGEMENT, outcome.reason)

    def test_a_failed_recording_is_not_comparable(self):
        sample = self.sample('record', CASSETTE, 0)
        sample.harvest_store(self.root, journal(judged('k', 'remote', 'error'), tool()))
        outcome = sample.seal()
        self.assertEqual(outcome.status, 'no_comparable')
        cassette = json.loads((sample.recorded / jev_fixture.CASSETTE_FILE).read_text())
        self.assertEqual(cassette, {'schema': 'kmp.typesafe.cassette.v1', 'model': MODEL, 'entries': {}})


class EnvironmentTest(FixtureCase):
    def test_cassette_binary_gets_a_per_sample_cassette_in_place_of_the_variants(self):
        named = self.root / 'named.cassette.json'
        jev_fixture.empty_cassette(named, MODEL)
        variant_env = {'KMP_TYPESAFE_CASSETTE': str(named), 'RUST_LOG': 'x=debug'}
        sample = self.sample('replay', CASSETTE, 2, variant_env)  # nothing recorded: the named one
        env = sample.env(variant_env)
        self.assertEqual(env['KMP_TYPESAFE_CASSETTE'], str(sample.work / 'cassette.json'))
        self.assertEqual((env['KMP_TYPESAFE_CASSETTE_MODE'], env['RUST_LOG']), ('replay', 'x=debug'))
        self.assertEqual((sample.work / 'cassette.json').read_bytes(), named.read_bytes())
        record = self.sample('record', CASSETTE, 0, variant_env)
        self.assertEqual(record.env(variant_env)['KMP_TYPESAFE_CASSETTE_MODE'], 'record')
        self.assertFalse((record.work / 'cassette.json').exists())  # starts empty, not from the named one

    def test_book_binary_records_against_the_provider_and_replays_behind_an_empty_cassette(self):
        self.record(BOOK, samples=1)
        record = self.sample('record', BOOK, 0).env({'KMP_TYPESAFE_CASSETTE': '/x', 'RUST_LOG': 'y'})
        self.assertEqual(record, {'RUST_LOG': 'y'})
        replay = self.sample('replay', BOOK, 0)
        env = replay.env({})
        self.assertEqual(env['KMP_TYPESAFE_CASSETTE_MODE'], 'replay')
        block = json.loads(Path(env['KMP_TYPESAFE_CASSETTE']).read_text())
        self.assertEqual(block['entries'], {})

    def test_replay_refuses_the_key_and_a_missing_fixture(self):
        self.record(CASSETTE, samples=1)
        with self.assertRaises(JevFixtureError):
            self.sample('replay', CASSETTE, 0).check_secrets({'TYPESAFE_API_KEY': 'k'})
        with self.assertRaises(JevFixtureMissing):
            self.sample('replay', CASSETTE, 1)
        with self.assertRaisesRegex(JevFixtureError, 'another fixture, sample or backend'):
            self.sample('replay', BOOK, 0)  # sample 0 holds a cassette recording
        with self.assertRaises(JevFixtureMissing):
            self.sample('replay', BOOK, 1)
        with self.assertRaises(JevFixtureError):
            self.fixture('off', CASSETTE)
        with self.assertRaises(JevFixtureError):
            self.fixture('record', CASSETTE).sample_dir(-1)

    def test_fixtures_live_under_the_jev_cache_kind(self):
        fixture = self.fixture('record', BOOK)
        self.assertEqual(fixture.root, self.layout.root / 'jev' / self.key)
        self.assertEqual(fixture.sample_dir(2).name, 'sample-2')

    def test_record_removes_a_book_a_template_carried(self):
        sample = self.sample('record', BOOK, 0)
        data = self.root / 'data'
        data.mkdir()
        for suffix in ('', '-wal', '-shm'):
            (data / (jev_fixture.BOOK_FILE + suffix)).write_text('stale')
        sample.prepare_store(data)
        self.assertEqual(list(data.iterdir()), [])


BOOK_SERVER = r'''
import json, os, sys
from pathlib import Path
data = Path(os.environ['KMP_MCP_DATA_DIR'])
resolved = {'message': 'embedded backend data dir resolved', 'rule': 'env', 'data_dir': str(data)}
sys.stderr.write(json.dumps({'fields': resolved}) + '\n')
sys.stderr.flush()
book = data / 'judgements.sqlite3'
seen = book.read_text().split() if book.exists() else []
(data / 'logs').mkdir(exist_ok=True)
log = (data / 'logs' / 'kmp-mcp.log.2026-09-26').open('a')
for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request:
        continue
    result = {'protocolVersion': '2024-11-05', 'serverInfo': {'name': 'fake'}, 'tools': []}
    if request['method'] == 'tools/call':
        key = request['params']['arguments']['question']
        source = 'book_hit' if key in seen else 'remote'
        seen.append(key)
        book.write_text(' '.join(seen))
        for fields in ({'event': 'kmp_judgement', 'site': 'rerank', 'source': source, 'status': 'ok',
                        'questions': 1, 'requests': 1, 'input_tokens': 5, 'elapsed_us': 1,
                        'request_key': key},
                       {'event': 'kmp_mcp_tool', 'kmp_move': 'kmp_ask', 'status': 'success',
                        'duration_ms': 1, 'duration_us': 9}):
            log.write(json.dumps({'fields': fields}) + '\n')
        log.flush()
        result = {'structuredContent': {'answer': source}, 'isError': False}
    sys.stdout.write(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}) + '\n')
    sys.stdout.flush()
'''


class SessionHookTest(FixtureCase):
    """The store hooks through a real BenchProcess: a book survives a process restart of its sample."""

    def test_book_is_seeded_and_harvested_around_each_process(self):
        binary = self.root / 'fake-kmp-mcp'
        binary.write_text(f'#!{sys.executable}\n' + BOOK_SERVER)
        binary.chmod(0o755)
        sample = self.sample('record', BOOK, 0)
        ids = itertools.count(1)
        sources = []
        for index, keys in enumerate((('a' * 64, 'b' * 64), ('a' * 64,))):
            config = ProcessConfig(binary, self.root / 'work', 'jev', env=sample.env({}),
                                   probe_resources=False,
                                   store_hooks=(sample,))
            out = self.root / f'out-{index}'
            out.mkdir()
            process = BenchProcess(config, out, index, ids)
            with process.journey() as tap:
                for k in keys:
                    response = tap.call('kmp_ask', {'question': k})
                    sources.append(response['result']['structuredContent']['answer'])
            process.close()
        self.assertEqual(sources, ['remote', 'remote', 'book_hit'])
        outcome = sample.seal()
        self.assertEqual(outcome.by_source, {'book_hit': 1, 'remote': 2})
        recorded = (sample.recorded / jev_fixture.BOOK_FILE).read_text().split()
        self.assertEqual(len(recorded), 3)
        self.assertTrue(os.path.exists(sample.recorded / 'fixture.json'))


if __name__ == '__main__':
    unittest.main()
