"""Store isolation refusals and the driver's follow-the-server rule, without a binary.

The transport tests run a tiny stand-in MCP server (Python, written at test time)
that logs the data-dir event and answers every request, except `tools/call`
of the tool `hang`, which it never answers.
"""
import itertools
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest

from ..capture.trace import read_trace
from ..domain.errors import HarnessError
from ..native.capture import CaptureOptions
from ..native.driver import MAX_CALLS, MAX_CALLS_LIMIT, run_journey
from ..native.isolation import (BASE_LOG_FILTER, IsolationError, confirm_effective_store,
                                create_store, tree_digest)
from ..native.recorder import TraceRecorder
from ..native.resources import NoProbe, ProcessProbe
from ..native.transport import StdioSession, TransportError, TransportTimeout
from ..scenarios import SCENARIOS, scenarios_digest
from ..scenarios.model import JourneySpec


class IsolationTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.home = Path(self.folder.name) / 'real-home'

    def test_store_inside_the_real_data_dir_is_refused(self):
        real = self.home / '.local/share/kmp'
        with self.assertRaises(IsolationError):
            create_store(real / 'work', 'x', {'HOME': str(self.home), 'PATH': '/usr/bin'})

    def test_explicit_real_store_env_is_refused_when_it_contains_the_work_dir(self):
        work = Path(self.folder.name) / 'work'
        with self.assertRaises(IsolationError):
            create_store(work, 'x', {'HOME': str(self.home), 'KMP_MCP_DATA_DIR': str(work)})

    def test_child_env_is_allowlisted_and_contained(self):
        parent = {'HOME': str(self.home), 'PATH': '/usr/bin', 'KMP_KERNEL_GRPC_ENDPOINT': 'x',
                  'OPENAI_API_KEY': 'secret'}
        store = create_store(Path(self.folder.name) / 'work', 'x', parent)
        self.assertNotIn('OPENAI_API_KEY', store.env)
        self.assertNotIn('KMP_KERNEL_GRPC_ENDPOINT', store.env)
        for key in ('HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME', 'KMP_MCP_DATA_DIR'):
            self.assertTrue(Path(store.env[key]).is_relative_to(store.root))

    def test_binary_must_confirm_the_disposable_store(self):
        store = create_store(Path(self.folder.name) / 'work', 'x', {'HOME': str(self.home)})
        line = {'fields': {'message': 'embedded backend data dir resolved', 'rule': 'env',
                           'data_dir': str(store.data_dir)}}
        self.assertEqual(confirm_effective_store(store, json.dumps(line))['rule'], 'env')
        line['fields']['data_dir'] = str(self.home)
        with self.assertRaises(IsolationError):
            confirm_effective_store(store, json.dumps(line))
        with self.assertRaises(IsolationError):
            confirm_effective_store(store, 'banner only')


class VariantEnvTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.home = Path(self.folder.name) / 'real-home'
        self.parent = {'HOME': str(self.home), 'PATH': '/usr/bin', 'TYPESAFE_API_KEY': 'sk-parent'}
        self.work = Path(self.folder.name) / 'work'

    def test_allowlisted_variables_are_passed_explicitly(self):
        cassette = Path(self.folder.name) / 'cassette.json'
        cassette.write_text('{}')
        store = create_store(self.work, 'x', self.parent, env={
            'KMP_MCP_ENGINE': 'redb', 'KMP_TYPESAFE_CASSETTE': str(cassette),
            'KMP_TYPESAFE_CASSETTE_MODE': 'replay', 'RUST_LOG': 'kmp_mcp::judgement=debug'})
        recorded = store.allowlisted_env()
        self.assertEqual(recorded['KMP_MCP_ENGINE'], 'redb')
        self.assertEqual(recorded['KMP_TYPESAFE_CASSETTE'], str(cassette.resolve()))
        self.assertEqual(recorded['RUST_LOG'], BASE_LOG_FILTER + ',kmp_mcp::judgement=debug')
        self.assertNotIn('TYPESAFE_API_KEY', store.env)  # never inherited from the parent

    def test_any_other_variable_is_refused(self):
        for name in ('KMP_KERNEL_GRPC_ENDPOINT', 'KMP_MCP_DATA_DIR', 'TYPESAFE_API_KEY', 'OPENAI_API_KEY'):
            with self.subTest(name=name), self.assertRaises(IsolationError) as caught:
                create_store(self.work, 'x', self.parent, env={name: 'x'})
            self.assertEqual(caught.exception.code, 'STORE_ISOLATION_REFUSED')
        self.assertEqual(list(self.work.iterdir()), [])  # refused before any root exists

    def test_path_variable_into_the_real_store_is_refused(self):
        with self.assertRaises(IsolationError):
            create_store(self.work, 'x', self.parent,
                         env={'KMP_LEXICAL_BRIDGE': str(self.home / '.local/share/kmp/bridge.bin')})

    def test_secret_reaches_the_child_but_never_the_record(self):
        store = create_store(self.work, 'x', self.parent, secrets={'TYPESAFE_API_KEY': 'sk-test-123'})
        self.assertEqual(store.env['TYPESAFE_API_KEY'], 'sk-test-123')
        self.assertNotIn('TYPESAFE_API_KEY', store.allowlisted_env())
        self.assertNotIn('sk-test-123', json.dumps(store.allowlisted_env()))
        self.assertNotIn('sk-test-123', repr(store))
        self.assertIn(('sk-test-123', '<TYPESAFE_API_KEY>'), store.redactions())
        with self.assertRaises(IsolationError):
            create_store(self.work, 'x', self.parent, secrets={'KMP_MCP_ENGINE': 'x'})

    def test_capture_options_refuse_a_bad_cap(self):
        self.assertEqual(CaptureOptions(max_calls=MAX_CALLS_LIMIT).max_calls, 256)
        for bad in (0, 257, True, 3.0):
            with self.subTest(bad=bad), self.assertRaises(HarnessError):
                CaptureOptions(max_calls=bad)


class TemplateTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.home = Path(self.folder.name) / 'real-home'
        self.parent = {'HOME': str(self.home)}
        self.template = Path(self.folder.name) / 'built-store'
        (self.template / 'redb').mkdir(parents=True)
        (self.template / 'redb/kmp.redb').write_bytes(b'\x00store bytes')
        (self.template / 'rerank.json').write_text('{"pool_size": 40}')

    def test_store_is_seeded_from_a_template_and_still_confirmed(self):
        store = create_store(Path(self.folder.name) / 'work', 'x', self.parent, template=self.template)
        self.assertEqual((store.data_dir / 'redb/kmp.redb').read_bytes(), b'\x00store bytes')
        self.assertEqual(store.template_digest, tree_digest(self.template))
        (store.data_dir / 'redb/kmp.redb').write_bytes(b'changed')  # the copy is disposable
        self.assertEqual((self.template / 'redb/kmp.redb').read_bytes(), b'\x00store bytes')
        line = {'fields': {'message': 'embedded backend data dir resolved', 'rule': 'env',
                           'data_dir': str(store.data_dir)}}
        self.assertEqual(confirm_effective_store(store, json.dumps(line))['rule'], 'env')
        line['fields']['data_dir'] = str(self.template)
        with self.assertRaises(IsolationError):
            confirm_effective_store(store, json.dumps(line))

    def test_template_inside_the_real_store_is_refused(self):
        real = self.home / '.local/share/kmp'
        real.mkdir(parents=True)
        with self.assertRaises(IsolationError):
            create_store(Path(self.folder.name) / 'work', 'x', self.parent, template=real)

    def test_template_with_a_symlink_is_refused(self):
        os.symlink(self.template / 'rerank.json', self.template / 'alias.json')
        with self.assertRaises(IsolationError):
            create_store(Path(self.folder.name) / 'work', 'x', self.parent, template=self.template)


FAKE_SERVER = r"""
import json, os, sys, time
data_dir = os.environ['KMP_MCP_DATA_DIR']
event = {'fields': {'message': 'embedded backend data dir resolved', 'rule': 'env',
                    'data_dir': data_dir, 'requested_engine': os.environ.get('KMP_MCP_ENGINE')}}
sys.stderr.write(json.dumps(event) + '\n')
sys.stderr.flush()
for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request:
        continue
    params = request.get('params') or {}
    if request['method'] == 'tools/call' and params.get('name') == 'hang':
        time.sleep(60)
    if request['method'] == 'tools/call':
        result = {'structuredContent': {'echo': params.get('arguments'), 'text': 'é' * 3}}
    else:
        result = {'protocolVersion': '2024-11-05', 'serverInfo': {'name': 'fake'}, 'tools': []}
    sys.stdout.write(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result},
                                ensure_ascii=False) + '\n')
    sys.stdout.flush()
"""


class TransportTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        root = Path(self.folder.name)
        self.binary = root / 'fake-kmp-mcp'
        self.binary.write_text(f'#!{sys.executable}\n' + FAKE_SERVER)
        self.binary.chmod(0o755)
        parent = {'HOME': str(root / 'real-home'), 'PATH': os.environ.get('PATH', '/usr/bin')}
        self.store = create_store(root / 'work', 'x', parent)
        self.trace = root / 'journey.jsonl'

    def open(self, **options):
        recorder = TraceRecorder(self.trace, self.store.redactions())
        session = StdioSession(self.binary, self.store, recorder, 'j', itertools.count(1), **options)
        self.addCleanup(recorder.close)
        self.addCleanup(session.close)
        return session, recorder

    def test_every_exchange_gets_timing_and_resources_markers(self):
        session, recorder = self.open()
        protocol = session.handshake('test')
        self.assertGreater(protocol['startup_ns'], 0)
        session.call('kmp_ask', {'about': 'a'})
        recorder.close()
        trace = read_trace('journey', self.trace.read_bytes())
        self.assertEqual(trace.rpc_pairs, 3)
        self.assertEqual(trace.harness_events, {'timing': 3, 'resources': 3})
        call = trace.calls[-1]
        exchange = json.loads(self.trace.read_text().splitlines()[call['event_index']])
        self.assertEqual(exchange['request']['params']['name'], 'kmp_ask')
        timing = call['timing']
        self.assertEqual(timing['response_path'], f'journey.jsonl#{call["event_index"]}')
        self.assertEqual((timing['process_call_index'], timing['censored']), (0, False))
        self.assertGreater(timing['wall_ns'], 0)
        raw = json.dumps(exchange['response'], ensure_ascii=False).encode()
        self.assertEqual(timing['response_bytes'], len(raw))
        resources = call['resources']
        if Path('/proc/self/io').exists():
            self.assertIsInstance(resources['rss_peak_kb'], int)
            self.assertGreaterEqual(resources['cpu_ns'], 0)
            self.assertEqual(set(resources['io']), {'rchar', 'wchar', 'syscr', 'syscw', 'read_bytes',
                                                   'write_bytes', 'cancelled_write_bytes'})
        self.assertEqual(session.last_observation.tool, 'kmp_ask')

    def test_timeout_is_transport_failed_with_its_time_recorded(self):
        session, recorder = self.open(timeout_seconds=0.3, probe_resources=False)
        session.handshake('test')
        before = time.monotonic_ns()
        with self.assertRaises(TransportTimeout) as caught:
            session.call('hang', {})
        self.assertEqual(caught.exception.code, 'TRANSPORT_FAILED')
        observation = caught.exception.observation
        self.assertTrue(observation.censored)
        self.assertEqual(observation.censor_reason, 'timeout')
        self.assertGreaterEqual(observation.wall_ns, 300_000_000)
        self.assertLess(time.monotonic_ns() - before, 5_000_000_000)
        with self.assertRaises(TransportError):
            session.call('kmp_ask', {})  # a timed-out session is not reused
        session.close()
        recorder.close()
        trace = read_trace('journey', self.trace.read_bytes())
        self.assertEqual(trace.failures, [])  # the unanswered request is not in the trace
        self.assertEqual(trace.calls[-1]['event_index'], None)
        self.assertEqual(trace.calls[-1]['timing']['wall_ns'], observation.wall_ns)
        self.assertEqual(trace.calls[-1]['resources']['absent']['cpu_ns'], NoProbe.reason)

    def test_bad_timeout_is_refused(self):
        for bad in (0, -1, True, '5', 3601):
            with self.subTest(bad=bad), self.assertRaises(HarnessError):
                StdioSession(self.binary, self.store, None, 'j', itertools.count(1), timeout_seconds=bad)


class ProbeTest(unittest.TestCase):
    def test_missing_proc_is_absent_with_a_reason(self):
        with tempfile.TemporaryDirectory() as folder:
            probe = ProcessProbe(123, proc_root=folder)
            usage = probe.after(probe.before())
        self.assertEqual((usage['rss_peak_kb'], usage['cpu_ns'], usage['io']), (None, None, None))
        self.assertEqual(set(usage['absent']), {'rss_peak_kb', 'cpu_ns', 'io'})

    def test_values_are_deltas_over_the_call(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder) / '7'
            (base / 'task/7').mkdir(parents=True)
            (base / 'task/8').mkdir()
            (base / 'clear_refs').write_text('')
            io = 'rchar: {}\nwchar: 1\nsyscr: 2\nsyscw: 3\nread_bytes: 0\nwrite_bytes: 0\n' \
                 'cancelled_write_bytes: 0\n'
            (base / 'io').write_text(io.format(100))
            (base / 'task/7/schedstat').write_text('1000 5 6\n')
            (base / 'task/8/schedstat').write_text('500 1 1\n')
            probe = ProcessProbe(7, proc_root=folder)
            snapshot = probe.before()
            self.assertEqual((base / 'clear_refs').read_text(), '5')
            (base / 'io').write_text(io.format(160))
            (base / 'task/8/schedstat').write_text('900 1 1\n')
            (base / 'status').write_text('Name:\tkmp\nVmHWM:\t  4242 kB\n')
            usage = probe.after(snapshot)
        self.assertEqual(usage['rss_peak_kb'], 4242)
        self.assertEqual(usage['cpu_ns'], 400)
        self.assertEqual(usage['io']['rchar'], 60)
        self.assertEqual(usage['absent'], {})

    def test_a_thread_that_exits_during_the_call_never_makes_cpu_negative(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder) / '7'
            (base / 'task/7').mkdir(parents=True)
            (base / 'task/8').mkdir()
            (base / 'clear_refs').write_text('')
            (base / 'task/7/schedstat').write_text('1000 5 6\n')
            (base / 'task/8/schedstat').write_text('34000000000 1 1\n')
            probe = ProcessProbe(7, proc_root=folder)
            snapshot = probe.before()
            (base / 'task/8/schedstat').unlink()
            (base / 'task/8').rmdir()
            (base / 'task/9').mkdir()
            (base / 'task/7/schedstat').write_text('1700 5 6\n')
            (base / 'task/9/schedstat').write_text('250 1 1\n')
            usage = probe.after(snapshot)
        self.assertEqual(usage['cpu_ns'], 700 + 250)


class FakeSession:
    def __init__(self, pages):
        self.pages, self.sent = list(pages), []

    def call(self, tool, arguments):
        self.sent.append((tool, arguments))
        page = self.pages.pop(0) if self.pages else self.last
        self.last = page
        return {'result': {'structuredContent': page}}


class TimeoutSession:
    def call(self, tool, arguments):
        raise TransportTimeout('j: no answer to tools/call within 1s', None)


class DriverTest(unittest.TestCase):
    def test_driver_executes_the_servers_next_action_verbatim(self):
        action = {'tool': 'kmp_wake', 'arguments': {'about': 'a', 'page': {'cursor': 'kmp1:9:x'}}}
        session = FakeSession([{'projection': {'next_action': action}}, {'projection': {}}])
        outcome = run_journey(session, JourneySpec('kmp_wake', {'about': 'a'}), 4096)
        self.assertEqual(session.sent[0][1], {'about': 'a', 'budget': {'max_bytes': 4096}})
        self.assertEqual(session.sent[1][1], action['arguments'])
        self.assertEqual((outcome.status, outcome.calls), ('completed', 2))

    def test_endless_continuations_stop_at_the_cap(self):
        loop = {'projection': {'next_action': {'tool': 'kmp_wake', 'arguments': {'about': 'a'}}}}
        outcome = run_journey(FakeSession([loop]), JourneySpec('kmp_wake', {'about': 'a'}), None)
        self.assertEqual((outcome.status, outcome.calls), ('call_cap_reached', MAX_CALLS))

    def test_cap_is_configurable_up_to_256_and_censors(self):
        loop = {'projection': {'next_action': {'tool': 'kmp_wake', 'arguments': {'about': 'a'}}}}
        outcome = run_journey(FakeSession([loop]), JourneySpec('kmp_wake', {'about': 'a'}), None,
                              max_calls=MAX_CALLS_LIMIT)
        self.assertEqual((outcome.status, outcome.calls), ('call_cap_reached', 256))
        self.assertEqual(outcome.as_dict()['censor_reason'], 'max_calls')
        self.assertTrue(outcome.censored)
        for bad in (0, 257):
            with self.subTest(bad=bad), self.assertRaises(HarnessError):
                run_journey(FakeSession([loop]), JourneySpec('kmp_wake', {}), None, max_calls=bad)

    def test_completed_journey_under_a_raised_cap_is_not_censored(self):
        session = FakeSession([{'projection': {}}])
        outcome = run_journey(session, JourneySpec('kmp_wake', {'about': 'a'}), None, max_calls=200)
        self.assertEqual(outcome.as_dict(), {'status': 'completed', 'calls': 1, 'error': None,
                                             'max_calls': 200, 'censored': False,
                                             'censor_reason': None})

    def test_timeout_censors_the_journey(self):
        outcome = run_journey(TimeoutSession(), JourneySpec('kmp_wake', {'about': 'a'}), None)
        self.assertEqual((outcome.status, outcome.censor_reason), ('transport_error', 'timeout'))
        self.assertIn('TRANSPORT_FAILED', outcome.error)


class ScenarioTest(unittest.TestCase):
    def test_digest_is_stable_and_sensitive(self):
        self.assertEqual(scenarios_digest(), scenarios_digest(SCENARIOS))
        self.assertNotEqual(scenarios_digest(SCENARIOS[:1]), scenarios_digest(SCENARIOS[1:2]))
        self.assertEqual(len({s.case_id for s in SCENARIOS}), len(SCENARIOS))


if __name__ == '__main__':
    unittest.main()
