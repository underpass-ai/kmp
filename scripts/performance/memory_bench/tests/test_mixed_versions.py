"""BT18: mixed versions and several processes on one store.

The scenarios are driven against a fake store and fake servers that implement the
Store and Server ports in-process (threads stand in for processes): a log file,
an optional lexical sidecar that catches up from the log, and an optional SQLite
verdict book. Each fake can be broken the way the real feature could break
(lost writes, a sidecar that does not catch up, a book where the last verdict
wins), and the scenario must then fail. The end-to-end case runs the real release
binary on the cached 10^3 store and is skipped with a reason when either is absent.
"""
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import tempfile
import threading
import time
import unittest

from ..application import mixed_versions as mv
from ..application import mixed_versions_cli as cli
from ..runtime import shared_store
from ..runtime.layout import public_layout
from ..runtime.shared_store import OpenRefused, Reply, ServerExit
from ..runtime.store_cache import StoreCache

ABOUT = 'synth:mono-a000'
STOP = frozenset({'the', 'did', 'where', 'move', 'moved', 'after', 'review', 'every', 'which',
                  'who', 'into', 'one', 'of', 'to', 'and'})


@dataclass(frozen=True)
class Behaviour:
    """What a fake binary does; it rides in BinaryRef.path."""
    format: int = 4
    readable: tuple = (4,)
    sidecar: bool = False
    catch_up_bug: bool = False
    book: bool = False
    last_wins_bug: bool = False
    lose_writes: bool = False
    touch_on_refusal: bool = False
    conflict_first: bool = False  # every write is refused once as a concurrency conflict


def binary(role, **behaviour):
    shape = Behaviour(**behaviour)
    capabilities = frozenset(name for name, on in ((mv.SIDECAR, shape.sidecar), (mv.BOOK, shape.book)) if on)
    return mv.BinaryRef(role, shape, 'f' * 64, f'fake {role}', capabilities)


def words(text):
    return {w for w in re.findall(r'[a-z]+', text.lower()) if w not in STOP and len(w) > 2}


class FakeStore:
    SIDECAR = 'store/lexical-index.sqlite3'  # JSON inside: the fake only needs the name
    BOOK = 'store/judgements.sqlite3'

    def __init__(self, root, seeded=True, jev=False, offline=False):
        self.root = Path(root)
        (self.root / 'store').mkdir(parents=True, exist_ok=True)
        self.lock = threading.Lock()
        self.jev, self.offline = jev, offline
        self.conflicted = set()
        if seeded and not (self.root / 'FORMAT_VERSION').exists():
            (self.root / 'FORMAT_VERSION').write_text('4')
            self._append([{'id': f'{ABOUT}:e0', 'text': 'The edge gateway fronts every public endpoint.'},
                          {'id': f'{ABOUT}:e1', 'text': 'Alba mapped the tumi and roro inbox into one.'}])

    @property
    def log(self):
        return self.root / 'store' / 'log.jsonl'

    def _append(self, entries):
        with self.log.open('a') as handle:
            for entry in entries:
                handle.write(json.dumps(entry) + '\n')

    def entries(self):
        if not self.log.exists():
            return []
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def format_stamp(self):
        stamp = self.root / 'FORMAT_VERSION'
        return stamp.read_text() if stamp.exists() else None

    def open(self, binary, name):
        shape = binary.path
        stamp = self.format_stamp()
        if stamp is None:
            (self.root / 'FORMAT_VERSION').write_text(str(shape.format))
        return FakeServer(self, shape, name)

    def fingerprint(self):
        digest = hashlib.sha256((self.format_stamp() or '').encode())
        digest.update(self.log.read_bytes() if self.log.exists() else b'')
        return digest.hexdigest()

    def has_file(self, relative):
        return (self.root / relative).exists()

    def remove_file(self, relative):
        (self.root / relative).unlink(missing_ok=True)

    def query(self, relative, sql):
        connection = sqlite3.connect(self.root / relative)
        try:
            return [tuple(r) for r in connection.execute(sql)]
        finally:
            connection.close()

    def integrity(self):
        return 'ok'

    def events(self, binary):
        return len(self.entries()), 'sha256:' + self.fingerprint()

    def transfer(self, writer, reader, label):
        if int(self.format_stamp()) not in reader.path.readable:
            raise OpenRefused(f'bundle format {self.format_stamp()} is not supported')
        target = FakeStore(tempfile.mkdtemp(dir=self.root.parent), seeded=False)
        (target.root / 'FORMAT_VERSION').write_text(str(reader.path.format))
        shutil.copyfile(self.log, target.log)
        return target

    def fork(self, label, offline=False):
        target = Path(tempfile.mkdtemp(dir=self.root.parent)) / 'data'
        shutil.copytree(self.root, target)
        return FakeStore(target, seeded=False, jev=self.jev, offline=offline)

    def close(self):
        shutil.rmtree(self.root, ignore_errors=True)


class FakeServer:
    def __init__(self, store, shape, name):
        self.store, self.shape, self.name, self.events = store, shape, name, []
        self.refused = int(store.format_stamp()) not in shape.readable

    def _reply(self, body=None, error=None):
        return Reply(error is None, body, error, 1.0)

    def call(self, tool, arguments):
        if self.refused:
            if self.shape.touch_on_refusal:
                (self.store.root / 'FORMAT_VERSION').write_text(str(self.shape.format))
            return self._reply(None, f'store uses format version {self.store.format_stamp()}, newer '
                                     'than this binary supports')
        handler = {'kmp_ingest': self._ingest, 'kmp_inspect': self._inspect, 'kmp_ask': self._ask,
                   'kmp_wake': self._wake}[tool]
        return handler(arguments)

    def _ingest(self, arguments):
        key = arguments['idempotency_key']
        if self.shape.conflict_first and key not in self.store.conflicted:
            self.store.conflicted.add(key)
            return self._reply(None, '{"code": "conflict", "message": "the store moved"}')
        entries = [{'id': e['id'], 'text': e['text']} for e in arguments['memory']['entries']]
        with self.store.lock:
            if not self.shape.lose_writes or not self.name.endswith('b'):
                self.store._append(entries)
        return self._reply({'memory': {'accepted': {'entries': len(entries)}}})

    def _inspect(self, arguments):
        found = [e for e in self.store.entries() if e['id'] == arguments['ref']]
        return self._reply({'object': {'ref': found[0]['id']}}) if found else self._reply(None, 'not found')

    def _scan(self, entries, asked):
        scores = {e['id']: len(asked & words(e['text'])) for e in entries}
        best = max(scores.values(), default=0)
        return sorted(ref for ref, score in scores.items() if best and score == best)

    def _indexed(self, asked):
        path = self.store.root / FakeStore.SIDECAR
        with self.store.lock:
            entries = self.store.entries()
            index = json.loads(path.read_text()) if path.exists() else {'position': 0, 'docs': []}
            if not self.shape.catch_up_bug or index['position'] == 0:
                index['docs'] += entries[index['position']:]
                index['position'] = len(entries)
            path.write_text(json.dumps(index))
        found = self._scan(index['docs'], asked)
        full = self._scan(entries, asked)
        self.events.append({'fields': {'event': 'kmp_lexical_shadow', 'differences': int(found != full)}})
        return found

    def _judge(self, question):
        key = hashlib.sha256(question.encode()).digest()
        connection = sqlite3.connect(self.store.root / FakeStore.BOOK, timeout=5)
        try:
            with connection:
                connection.execute('CREATE TABLE IF NOT EXISTS verdicts(key BLOB '
                                   + ('' if self.shape.last_wins_bug else 'PRIMARY KEY')
                                   + ', value BLOB)')
            row = connection.execute('SELECT value FROM verdicts WHERE key = ?', (key,)).fetchone()
            if row is not None and not self.shape.last_wins_bug:
                self.events.append({'fields': {'event': 'kmp_judgement', 'source': 'book',
                                               'status': 'ok', 'http_requests': 0}})
                return row[0].decode()
            if self.store.offline:
                self.events.append({'fields': {'event': 'kmp_judgement', 'source': 'cassette_miss',
                                               'status': 'error'}})
                return None
            time.sleep(0.02)  # the provider: slow enough that both processes miss together
            mine = self.name.encode()
            with connection:
                connection.execute(('INSERT INTO' if self.shape.last_wins_bug else 'INSERT OR IGNORE INTO')
                                   + ' verdicts VALUES (?, ?)', (key, mine))
            self.events.append({'fields': {'event': 'kmp_judgement', 'source': 'remote',
                                           'status': 'ok', 'http_requests': 1}})
            if self.shape.last_wins_bug:
                return mine.decode()
            row = connection.execute('SELECT value FROM verdicts WHERE key = ?', (key,)).fetchone()
            return row[0].decode()
        finally:
            connection.close()

    def _ask(self, arguments):
        asked = words(arguments['question'])
        refs = self._indexed(asked) if self.shape.sidecar else self._scan(self.store.entries(), asked)
        body = {'answer': 'UNKNOWN' if not refs else 'found',
                'because': [{'ref': 'entry:' + ref} for ref in refs]}
        if self.shape.book and self.store.jev:
            body['verdict'] = self._judge(arguments['question'])
        return self._reply(body)

    def _wake(self, arguments):
        return self._reply({'projection': {'total': len(self.store.entries())}})

    def close(self):
        return ServerExit(0, tuple(self.events))


class FakeWorld:
    def __init__(self, test):
        self.root = Path(tempfile.mkdtemp(prefix='bt18-fake-'))
        test.addCleanup(shutil.rmtree, self.root, True)

    def new_store(self, label, seeded=True, jev=False):
        return FakeStore(tempfile.mkdtemp(prefix=label + '-', dir=self.root) + '/data', seeded, jev)

    def env(self, reference=None, candidate=None, old=None, jev=False):
        return mv.Environment(self.new_store, reference or binary('reference'),
                              candidate or binary('candidate'), old, jev)


SETTINGS = mv.Settings(writes=3, reads=2, questions=('Which gateway fronts the endpoint?',
                                                      'Who mapped the tumi inbox?'))


def names(result, ok=None):
    return [c.name for c in result.checks if ok is None or c.ok == ok]


class ValuesTest(unittest.TestCase):
    def test_planned_writes_are_unique_single_tokens(self):
        writes = mv.planned_writes('ca', 30, ABOUT, declare_dimensions=True)
        self.assertEqual(len({w.token for w in writes}), 30)
        self.assertTrue(all(re.fullmatch(r'[a-z]+', w.token) for w in writes))
        self.assertEqual(len({w.ref for w in writes}), 30)
        self.assertTrue(writes[0].arguments['memory']['dimensions'])
        self.assertFalse(writes[1].arguments['memory']['dimensions'])
        self.assertIn(writes[4].token, writes[4].question)
        with self.assertRaises(ValueError):
            mv.planned_writes('C1', 1, ABOUT)

    def test_summary_and_conclude(self):
        self.assertEqual(mv.summary([]), {'n': 0, 'p50': None, 'p95': None, 'max': None})
        self.assertEqual(mv.summary([1.0, 2.0, 3.0])['p50'], 2.0)
        result = mv.conclude('s', 'c', [mv.Check('a', True), mv.Check('b', False, gating=False)],
                             {'x': [1.0]})
        self.assertEqual(result.status, mv.PASS)
        failing = mv.conclude('s', 'c', [mv.Check('a', False)])
        self.assertEqual((failing.status, failing.reason), (mv.FAIL, 'failed: a'))
        self.assertEqual(mv.skipped('s', 'c', mv.NOT_IN_BINARY).as_dict()['reason'], 'not in this binary')
        with self.assertRaises(ValueError):
            mv.Settings(writes=0)

    def test_comparable_sets_aside_minted_handles_only(self):
        left = {'projection': {'next_action': {'arguments': {'continuation': 'read_1'}},
                               'page': {'next_cursor': 'kmp1:a'}}, 'answer': 'x'}
        right = json.loads(json.dumps(left).replace('read_1', 'read_2').replace('kmp1:a', 'kmp1:b'))
        self.assertEqual(mv.comparable(left), mv.comparable(right))
        self.assertNotEqual(mv.comparable(left), mv.comparable({**right, 'answer': 'y'}))
        a, b = Reply(True, left, None, 1.0), Reply(True, right, None, 1.0)
        self.assertEqual(mv.differing([a, a], [b, Reply(False, None, 'e', 1.0)]), [1])

    def test_format_refusal_matches_the_real_messages(self):
        for text in ('uses format version 4, newer than this binary supports (2)',
                     'uses unsupported format version 2; current KMP opens format 4 only',
                     'import failed: bundle format 2 is not supported (this binary reads 3)'):
            self.assertTrue(mv.FORMAT_REFUSAL.search(text), text)
        self.assertFalse(mv.FORMAT_REFUSAL.search('missing required argument `about`'))

    def test_shadow_differences_and_judgement_lines(self):
        events = [{'fields': {'event': 'kmp_lexical_shadow', 'differences': 2}},
                  {'fields': {'event': 'kmp_lexical_shadow', 'diffs': 1}},
                  {'fields': {'event': 'kmp_judgement', 'source': 'book'}}, {'x': 1}]
        self.assertEqual(mv.shadow_differences(events), (3, 2))
        self.assertEqual(len(mv.judgement_lines(events)), 1)

    def test_run_parallel_starts_together_and_reraises(self):
        self.assertEqual(mv.run_parallel([lambda: 1, lambda: 2]), [1, 2])

        def boom():
            raise RuntimeError('x')
        with self.assertRaises(RuntimeError):
            mv.run_parallel([lambda: 1, boom])

    def test_declares_reads_marker_bytes(self):
        folder = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, folder, True)
        path = folder / 'bin'
        path.write_bytes(b'\0junk judgements.sqlite3\0more')
        self.assertEqual(shared_store.declares(path, mv.CAPABILITY_MARKERS), frozenset({mv.BOOK}))

    def test_log_events_skips_banner_and_bad_lines(self):
        text = 'banner\n{"fields": {"event": "x"}}\n{broken\n[1]\n'
        self.assertEqual(shared_store.log_events(text), ({'fields': {'event': 'x'}},))


class WriteThenReadTest(unittest.TestCase):
    def test_direct_read_passes(self):
        world = FakeWorld(self)
        env = world.env()
        result = mv.write_then_read(env, env.reference, env.candidate, SETTINGS)
        self.assertEqual(result.status, mv.PASS, result)
        self.assertEqual(result.facts['path'], 'direct')
        self.assertEqual(result.latency['reader_ask']['n'], 5)

    def test_clean_refusal_passes_and_reports_the_bundle(self):
        world = FakeWorld(self)
        old = binary('old', format=2, readable=(1, 2))
        env = world.env(old=old)
        newer_to_old = mv.write_then_read(env, env.reference, old, SETTINGS)
        self.assertEqual(newer_to_old.status, mv.PASS, newer_to_old)
        self.assertEqual(newer_to_old.facts['path'], 'refused_by_format')
        self.assertIn('bundle_path', names(newer_to_old, ok=False))
        old_to_newer = mv.write_then_read(env, old, env.reference, SETTINGS)
        self.assertEqual(old_to_newer.status, mv.PASS, old_to_newer)
        self.assertEqual(old_to_newer.facts['format_before'], '2')

    def test_bundle_path_passes_when_the_reader_imports(self):
        world = FakeWorld(self)
        old = binary('old', format=2, readable=(2,))
        reader = binary('reference', readable=(2, 4))
        store = world.new_store('x', seeded=False)
        server = store.open(old, 'w')
        mv.write_all(server, mv.planned_writes('wr', 2, ABOUT), [])
        check = mv._bundle_check(store, old, reader, SETTINGS, mv.planned_writes('wr', 2, ABOUT))
        self.assertTrue(check.ok, check)

    def test_refusal_that_restamps_the_store_fails(self):
        world = FakeWorld(self)
        old = binary('old', format=2, readable=(2,), touch_on_refusal=True)
        env = world.env(old=old)
        result = mv.write_then_read(env, env.reference, old, SETTINGS)
        self.assertEqual(result.status, mv.FAIL)
        self.assertIn('store_untouched', names(result, ok=False))


class ConcurrencyTest(unittest.TestCase):
    def test_two_writers_pass(self):
        env = FakeWorld(self).env()
        result = mv.concurrent_writers(env, env.reference, env.candidate, SETTINGS)
        self.assertEqual(result.status, mv.PASS, result)
        self.assertEqual(result.facts['events_after'] - result.facts['events_before'], 6)

    def test_conflicts_are_retried_with_the_same_key(self):
        env = FakeWorld(self).env(candidate=binary('candidate', conflict_first=True))
        result = mv.concurrent_writers(env, env.reference, env.candidate, SETTINGS)
        self.assertEqual(result.status, mv.PASS, result)
        self.assertEqual(result.facts['conflicts_retried'], [0, 3])
        server = FakeWorld(self).new_store('x').open(binary('c', conflict_first=True), 'w')
        replies = mv.write_all(server, mv.planned_writes('wr', 1, ABOUT), [])
        self.assertTrue(mv.accepted(replies[0]))

    def test_run_survives_a_broken_setup(self):
        env = FakeWorld(self).env()

        def broken(label, seeded=True, jev=False):
            raise mv.OpenRefused('no store')
        report = mv.run(mv.Environment(broken, env.reference, env.candidate), SETTINGS,
                        ['concurrent_writers'])
        self.assertEqual(report['counts'][mv.FAIL], 2)

    def test_a_lost_write_fails(self):
        env = FakeWorld(self).env(candidate=binary('candidate', lose_writes=True))
        result = mv.concurrent_writers(env, env.reference, env.candidate, SETTINGS)
        self.assertEqual(result.status, mv.FAIL)
        self.assertIn('writes_visible', names(result, ok=False))

    def test_reads_during_writes_pass_and_measure_both_phases(self):
        env = FakeWorld(self).env()
        result = mv.reads_during_writes(env, env.reference, env.candidate, SETTINGS)
        self.assertEqual(result.status, mv.PASS, result)
        self.assertEqual(result.latency['ask_idle']['n'], 2)
        self.assertGreaterEqual(result.latency['wake_under_write']['n'], 2)

    def test_run_reports_every_case(self):
        env = FakeWorld(self).env(old=binary('old', format=2, readable=(2,)))
        seen = []
        report = mv.run(env, SETTINGS, progress=seen.append)
        self.assertEqual(report['schema'], mv.SCHEMA)
        self.assertEqual(len(report['results']), 10)
        self.assertEqual(report['counts'], {mv.PASS: 8, mv.FAIL: 0, mv.SKIPPED: 2})
        self.assertTrue(all(mv.render_line(r) for r in seen))
        with self.assertRaises(ValueError):
            mv.run(env, SETTINGS, ['nope'])


class SidecarTest(unittest.TestCase):
    def test_skipped_when_the_binary_has_none(self):
        result = mv.sidecar_catch_up(FakeWorld(self).env(), SETTINGS)
        self.assertEqual((result.status, result.reason), (mv.SKIPPED, 'not in this binary'))

    def test_catch_up_after_an_old_writer_passes(self):
        env = FakeWorld(self).env(candidate=binary('candidate', sidecar=True))
        result = mv.sidecar_catch_up(env, SETTINGS)
        self.assertEqual(result.status, mv.PASS, result)
        self.assertTrue(result.facts['writer_ignores_sidecar'])
        self.assertGreater(result.facts['shadow_lines'], 0)

    def test_a_sidecar_that_does_not_catch_up_fails(self):
        env = FakeWorld(self).env(candidate=binary('candidate', sidecar=True, catch_up_bug=True))
        result = mv.sidecar_catch_up(env, SETTINGS)
        self.assertEqual(result.status, mv.FAIL)
        self.assertTrue({'catch_up_retrieves_new_writes', 'equal_to_rebuild',
                         'no_shadow_differences'} <= set(names(result, ok=False)))


class BookTest(unittest.TestCase):
    def test_skipped_without_the_capability_or_a_jev_setup(self):
        world = FakeWorld(self)
        self.assertEqual(mv.book_first_wins(world.env(), SETTINGS).reason, 'not in this binary')
        result = mv.book_first_wins(world.env(candidate=binary('candidate', book=True)), SETTINGS)
        self.assertEqual(result.status, mv.SKIPPED)
        self.assertIn('Jev', result.reason)

    def test_first_verdict_wins_across_processes(self):
        env = FakeWorld(self).env(candidate=binary('candidate', book=True), jev=True)
        result = mv.book_first_wins(env, SETTINGS)
        self.assertEqual(result.status, mv.PASS, result)
        self.assertEqual(result.facts['verdicts'], len(SETTINGS.questions))

    def test_a_book_where_the_last_verdict_wins_fails(self):
        env = FakeWorld(self).env(candidate=binary('candidate', book=True, last_wins_bug=True), jev=True)
        result = mv.book_first_wins(env, SETTINGS)
        self.assertEqual(result.status, mv.FAIL)
        self.assertTrue({'no_duplicate_verdicts', 'first_verdict_wins'} <= set(names(result, ok=False)))


REFERENCE_ENV = 'MEMORY_BENCH_REFERENCE_BINARY'


class RealBinaryTest(unittest.TestCase):
    """The release binary on the cached 10^3 store: v0.23.0 has neither sidecar nor book."""

    def setUp(self):
        candidate =Path(os.environ.get(REFERENCE_ENV) or shared_store.binaries.DEFAULT_BINARY)
        if not candidate.is_file():
            self.skipTest(f'{candidate} not built')
        layout = public_layout()
        try:
            self.entry = cli.find_store(StoreCache(layout))
        except cli.MixedSetupError as failure:
            self.skipTest(str(failure))
        self.binary = cli.binary_ref('candidate', candidate)
        self.out = Path(tempfile.mkdtemp(prefix='bt18-test-', dir=layout.scratch()))
        self.addCleanup(shutil.rmtree, self.out, True)
        self.env = mv.Environment(cli.store_factory(self.entry, layout.scratch(), self.out),
                                  cli.binary_ref('reference', candidate), self.binary)

    def test_release_binary_on_one_store(self):
        settings = mv.Settings(writes=2, reads=1)
        report = mv.run(self.env, settings, ['write_then_read', 'concurrent_writers',
                                              'sidecar_catch_up', 'book_first_wins'])
        statuses = {(r['scenario'], r['case']): r['status'] for r in report['results']}
        self.assertEqual(statuses[('write_then_read', 'reference->candidate')], mv.PASS, report)
        self.assertEqual(statuses[('concurrent_writers', 'candidate+candidate')], mv.PASS, report)
        self.assertEqual(statuses[('sidecar_catch_up', 'reference writes behind candidate sidecar')],
                         mv.SKIPPED)
        self.assertTrue(all(r['reason'] == mv.NOT_IN_BINARY for r in report['results']
                            if r['status'] == mv.SKIPPED))


if __name__ == '__main__':
    unittest.main()
