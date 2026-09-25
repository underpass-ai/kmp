"""BT15 runtime: server journal, core pinning, ABAB runs, censoring and write cost.

The end-to-end tests drive a fake kmp-mcp (a Python script) through the real
token_harness transport: it confirms its data directory like the binary does,
writes a server journal under `<data dir>/logs/`, and behaves by the question
text (pages, hang, boom, judge). A `fake.json` in the store template says
whether it speaks BT03 telemetry.
"""
import gzip
import itertools
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

from ...token_harness.application.verify import verify
from ..application import run_questions
from ..application.run_questions import Arm, RunPlan, RunRefused, StoreRef, run_arms
from ..application.write_cost import WriteCostFailed, WritePlan, ingest_arguments, measure_write_cost
from ..domain import cachekey
from ..domain.question import Question
from ..domain.run_manifest import RunManifest
from ..domain.run_record import CallRecord, JourneyRecord
from ..domain.variant import parse_variant
from ..runtime import probes, server_log
from ..runtime.layout import public_layout
from ..runtime.session import BenchProcess, ProcessConfig, child_env

FAKE_SERVER = r'''
import hashlib, json, os, sys, time
from pathlib import Path
data = Path(os.environ['KMP_MCP_DATA_DIR'])
conf = json.loads((data / 'fake.json').read_text()) if (data / 'fake.json').exists() else {}
bt03 = conf.get('bt03', False)
(data / 'logs').mkdir(exist_ok=True)
journal = (data / 'logs' / 'kmp-mcp.log.2026-09-26').open('a')
def log(fields):
    journal.write(json.dumps({'timestamp': 't', 'level': 'INFO', 'target': 'kmp_mcp', 'fields': fields}) + '\n')
    journal.flush()
resolved = {'message': 'embedded backend data dir resolved', 'rule': 'env', 'data_dir': str(data)}
sys.stderr.write(json.dumps({'fields': resolved}) + '\n')
sys.stderr.flush()
log(resolved)
if bt03:
    for path in sorted(data.glob('*.json')):
        if path.name != 'fake.json':
            loaded = path.name == 'rerank.json'
            log({'event': 'kmp_store_config', 'file': path.name, 'status': 'loaded' if loaded else 'ignored',
                 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                 'reason': None if loaded else 'not read by this kmp-mcp version'})
    log({'event': 'kmp_store_config_summary', 'loaded': '', 'ignored': ''})
for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request:
        continue
    params = request.get('params') or {}
    if request['method'] != 'tools/call':
        result = {'protocolVersion': '2024-11-05', 'serverInfo': {'name': 'fake'}, 'tools': []}
    else:
        name, args = params['name'], params.get('arguments') or {}
        question = args.get('question') or ''
        if 'hang' in question:
            time.sleep(30)
        if 'judge' in question and bt03:
            log({'event': 'kmp_judgement', 'site': 'rerank', 'source': 'cassette_hit', 'status': 'ok',
                 'questions': 3, 'answers': 3, 'requests': 1, 'http_requests': 0, 'input_tokens': 395,
                 'elapsed_us': 127})
        structured, is_error = {'answer': 'UNKNOWN', 'projection': {}}, False
        if name == 'kmp_ingest':
            ok = 'refuse' not in json.dumps(args)
            structured = {'memory': {'read_after_write_ready': ok}}
        elif 'pages' in question or 'continuation' in args:
            page = int(args.get('continuation', 'p0')[1:]) + 1
            action = {'tool': 'kmp_ask', 'arguments': {'continuation': f'p{page}'}} if page < 3 else None
            structured = {'answer': f'page {page}', 'projection': {'next_action': action}}
        elif 'boom' in question:
            structured, is_error = {'error': {'code': 'invalid_argument'}}, True
        fields = {'event': 'kmp_mcp_tool', 'kmp_move': name, 'status': 'error' if is_error else 'success',
                  'duration_ms': 1}
        if bt03:
            fields['duration_us'] = 1234
        log(fields)
        result = {'structuredContent': structured, 'isError': is_error}
    sys.stdout.write(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}) + '\n')
    sys.stdout.flush()
'''


def question(identifier, text, private=False):
    return Question.from_dict({
        'schema': 'kmp.bench.question.v1', 'id': identifier,
        'corpus': 'b-real' if private else 'synth-v1', 'type': 'lookup_exact', 'about': 'synth:a',
        'tool': 'kmp_ask', 'question': text, 'answer_policy': 'evidence_or_unknown',
        'gold': {'answerable': 'KNOWN', 'shape': 'singular',
                 'facets': [{'name': 'f', 'answers': ['synth:a:e1'], 'related': [], 'answerable_facet': True}]},
        'private': private})


def variant(name, **extra):
    return parse_variant({'name': name, 'claim': 'parity', 'targets': ['parity_rate'], 'guards': [], 'n': 1,
                          'store': 'shared', 'jev': 'off', 'binary': {'path': 'fake'}, **extra}, name)


def read_gz_lines(path):
    return [json.loads(line) for line in gzip.decompress(path.read_bytes()).decode().splitlines()]


def journal(*fields):
    return [json.dumps({'fields': item}) for item in fields]


def tool_line(move, us=None, ms=1):
    line = {'event': 'kmp_mcp_tool', 'kmp_move': move, 'status': 'success', 'duration_ms': ms}
    if us is not None:
        line['duration_us'] = us
    return line


JUDGED = {'event': 'kmp_judgement', 'site': 'rerank', 'source': 'cassette_hit', 'status': 'ok',
          'questions': 3, 'requests': 1, 'input_tokens': 395, 'elapsed_us': 127}


class ServerLogTest(unittest.TestCase):
    def test_lines_are_attributed_by_position_with_their_judgements(self):
        telemetry = server_log.parse(journal(
            {'event': 'kmp_store_config_summary'}, tool_line('kmp_ask', 900), JUDGED,
            {**JUDGED, 'site': 'wake_focus'}, tool_line('kmp_wake', 2000, 2)) + ['not json'])
        self.assertEqual(telemetry.malformed, 1)
        first, _ = telemetry.for_call(0, 'kmp_ask')
        second, _ = telemetry.for_call(1, 'kmp_wake')
        self.assertEqual(first.jev(), ((), None))
        evaluations, reason = second.jev()
        self.assertIsNone(reason)
        self.assertEqual([e.site for e in evaluations], ['rerank', 'wake_focus'])
        self.assertEqual(evaluations[0].as_dict(), {'site': 'rerank', 'questions': 3, 'requests': 1,
                                                   'input_tokens': 395, 'source': 'cassette_hit', 'us': 127})
        self.assertEqual((second.tool.duration_ms, second.tool.duration_us), (2, 2000))

    def test_missing_or_mismatched_lines_are_absent_with_a_reason(self):
        telemetry = server_log.parse(journal(tool_line('kmp_ask'), JUDGED))
        self.assertEqual(telemetry.unattributed_judgements, 1)
        self.assertIn('no kmp_mcp_tool line', telemetry.for_call(1, 'kmp_ask')[1])
        self.assertIn('names kmp_ask', telemetry.for_call(0, 'kmp_wake')[1])
        self.assertIn('out of step', telemetry.for_call(0, 'kmp_ask', trusted_below=0)[1])
        self.assertIn('no kmp_mcp_tool line', telemetry.for_call(1, 'kmp_ask', trusted_below=0)[1])
        self.assertIn('never reached', telemetry.for_call(None, 'kmp_ask')[1])

    def test_old_binary_has_no_jev_and_a_failed_evaluation_voids_the_cost(self):
        old = server_log.parse(journal(tool_line('kmp_ask')))
        self.assertEqual(old.calls[0].jev(), (None, server_log.NO_JUDGEMENT))
        failed = server_log.parse(journal({**JUDGED, 'status': 'error', 'error_hash': 'ab12'},
                                          tool_line('kmp_ask', 10)))
        value, reason = failed.calls[0].jev()
        self.assertIsNone(value)
        self.assertIn('rerank/cassette_hit/ab12', reason)

    def test_store_files_are_acknowledged_by_name_and_sha(self):
        lines = journal({'event': 'kmp_store_config', 'file': 'rerank.json', 'status': 'loaded', 'sha256': 'a'},
                        {'event': 'kmp_store_config', 'file': 'ask-gate.json', 'status': 'ignored',
                         'sha256': 'b', 'reason': 'not read by this kmp-mcp version'},
                        {'event': 'kmp_store_config', 'file': 'wake-focus.json', 'status': 'loaded', 'sha256': 'x'},
                        {'event': 'kmp_store_config_summary'})
        acks = server_log.acknowledge((('rerank.json', 'a'), ('ask-gate.json', 'b'), ('wake-focus.json', 'c'),
                                       ('typesafe.json', 'd')), server_log.parse(lines))
        self.assertEqual([a.applied for a in acks], [True, False, False, False])
        self.assertEqual(acks[1].reason, 'not read by this kmp-mcp version')
        self.assertIn('copied c', acks[2].reason)
        self.assertIn('no kmp_store_config line', acks[3].reason)
        old = server_log.acknowledge((('rerank.json', 'a'),), server_log.parse([]))
        self.assertEqual(old[0].as_dict()['reason'], server_log.PREDATES_STORE_CONFIG)

    def test_cursor_reads_only_new_complete_lines_across_days(self):
        with tempfile.TemporaryDirectory() as folder:
            logs = Path(folder) / 'logs'
            logs.mkdir()
            (logs / 'kmp-mcp.log.2026-09-25').write_text('old\n')
            cursor = server_log.LogCursor(folder)
            with (logs / 'kmp-mcp.log.2026-09-25').open('a') as handle:
                handle.write('a\npartial')
            (logs / 'kmp-mcp.log.2026-09-26').write_text('b\n')
            (logs / 'other.txt').write_text('ignored\n')
            self.assertEqual(cursor.read_new(), ['a', 'b'])
            with (logs / 'kmp-mcp.log.2026-09-25').open('a') as handle:
                handle.write(' line\n')
            self.assertEqual(cursor.read_new(), ['partial line'])
        self.assertEqual(server_log.LogCursor('/nonexistent').read_new(), [])


class ProbesTest(unittest.TestCase):
    def test_cpu_lists_parse_and_format_like_taskset(self):
        self.assertEqual(probes.parse_cpu_list('5-9'), frozenset(range(5, 10)))
        self.assertEqual(probes.parse_cpu_list('1,3-4'), frozenset({1, 3, 4}))
        self.assertEqual(probes.format_cpu_list({5, 6, 7, 9}), '5-7,9')
        for bad in ('a', '9-5', '1-', ''):
            with self.subTest(bad=bad), self.assertRaises(probes.PinningError):
                probes.parse_cpu_list(bad)

    def test_pinning_needs_every_requested_cpu(self):
        pinned = probes.resolve_pinning('5-9', available=set(range(20)))
        self.assertTrue(pinned.applied)
        self.assertEqual(pinned.as_dict()['cpus'], '5-9')
        missing = probes.resolve_pinning('5-9', available={0, 1, 5})
        self.assertFalse(missing.applied)
        self.assertIn('6-9', missing.reason)
        self.assertFalse(probes.resolve_pinning(None).applied)

    def test_children_inherit_the_pin_and_the_parent_gets_its_affinity_back(self):
        calls = []
        pinning = probes.resolve_pinning('5-9', available=set(range(20)))
        with pinning.for_children(lambda pid: {0, 1}, lambda pid, cpus: calls.append(set(cpus))):
            calls.append('spawn')
        self.assertEqual(calls, [set(range(5, 10)), 'spawn', {0, 1}])
        with probes.Pinning(None, frozenset(), 'no').for_children(None, None):
            pass

    def test_machine_state_comes_from_proc_and_sys(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'loadavg').write_text('0.50 0.25 0.10 1/2 3\n')
            for cpu, name in ((5, 'performance'), (6, 'performance')):
                (root / f'cpu{cpu}/cpufreq').mkdir(parents=True)
                (root / f'cpu{cpu}/cpufreq/scaling_governor').write_text(name + '\n')
            (root / '4242').mkdir()
            (root / '4242/status').write_text('Name:\tx\nCpus_allowed_list:\t5-6\n')
            pinning = probes.resolve_pinning('5-6', available={5, 6})
            state = probes.machine(pinning, proc_root=root, sys_root=root)
            self.assertEqual((state['cpus'], state['governor'], state['loadavg']),
                             ('5-6', 'performance', [0.5, 0.25, 0.1]))
            self.assertEqual(probes.allowed_cpus(4242, root), '5-6')
            self.assertIsNone(probes.allowed_cpus(1, root))
            (root / 'cpu6/cpufreq/scaling_governor').write_text('powersave\n')
            self.assertEqual(probes.governors({5, 6}, root), ['performance', 'powersave'])
            self.assertIsNone(probes.governors({7}, root))
            self.assertIsNone(probes.loadavg(root / 'none'))


class FakeBinaryCase(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        self.binary = self.root / 'fake-kmp-mcp'
        self.binary.write_text(f'#!{sys.executable}\n' + FAKE_SERVER)
        self.binary.chmod(0o755)
        self.layout = public_layout(self.root / 'cache')
        self.cpu = str(min(os.sched_getaffinity(0)))
        self.templates = itertools.count()

    def template(self, name, bt03):
        path = self.root / f'{name}-{next(self.templates)}'
        path.mkdir()
        (path / 'fake.json').write_text(json.dumps({'bt03': bt03}))
        return path

    def arm(self, name, bt03, **extra):
        store = StoreRef('3' * 64, None, 'fake store', self.template(f'template-{name}', bt03))
        return Arm(variant(name, **extra), f'variants/{name}.toml', self.binary, store, 'fake')


class ProcessTest(FakeBinaryCase):
    def test_child_env_keeps_the_bench_filter_ahead_of_the_variant(self):
        self.assertEqual(child_env({})['RUST_LOG'], server_log.BENCH_LOG_FILTER)
        self.assertEqual(child_env({'RUST_LOG': 'kmp_mcp::x=trace'})['RUST_LOG'],
                         server_log.BENCH_LOG_FILTER + ',kmp_mcp::x=trace')

    def test_a_process_reports_pin_journal_and_store_files(self):
        out = self.root / 'out'
        out.mkdir()
        files = variant('v', store_files={'rerank.json': {'json': {'pool_size': 40}}}).store_files
        config = ProcessConfig(self.binary, self.root / 'work', 'v', store_files=files,
                               template=self.template('t', True),
                               pinning=probes.resolve_pinning(self.cpu))
        process = BenchProcess(config, out, 0, itertools.count(1))
        with process.journey('q~s0~r0') as tap:
            tap.call('kmp_ask', {'question': 'plain'})
        report = process.close()
        self.assertEqual(report.cpus_allowed, self.cpu)
        self.assertEqual((report.tool_calls, len(report.telemetry.calls)), (1, 1))
        self.assertEqual([a.applied for a in report.store_files], [True])
        self.assertFalse(process.store.root.exists())
        self.assertIn('<store-root>', (out / 'process-0.stderr').read_text())
        self.assertEqual(tap.attempts[0].observation.response_path, 'q~s0~r0.jsonl#1')


class RunArmsTest(FakeBinaryCase):
    def plan(self, questions, **options):
        return RunPlan(tuple(questions), mode='test', cpus=self.cpu, timeout_s=options.pop('timeout_s', 5.0),
                       **options)

    def test_two_arms_run_abab_with_warmup_and_valid_records(self):
        questions = [question(f'q{i}', 'judge plain' if i == 0 else 'plain') for i in range(3)]
        arms = [self.arm('base', False), self.arm('cand', True,
                                                  store_files={'rerank.json': {'json': {'pool_size': 40}}})]
        order = []
        real = run_questions.drive

        def spy(process, plan, warmup=False):
            order.append((process.config.label, plan.question.id, plan.repeat, warmup))
            return real(process, plan, warmup)
        with mock.patch.object(run_questions, 'drive', spy):
            results = run_arms(arms, self.plan(questions, block=2, repeats=2, warmup=1), self.layout)
        measured = [(label, qid) for label, qid, repeat, warm in order if not warm and repeat == 0]
        self.assertEqual(measured, [('base', 'q0'), ('base', 'q1'), ('cand', 'q0'), ('cand', 'q1'),
                                    ('base', 'q2'), ('cand', 'q2')])
        self.assertEqual([o for o in order if o[3]], [('base', 'q0', 0, True), ('cand', 'q0', 0, True)])
        base, cand = results
        for result, position in ((base, 'A'), (cand, 'B')):
            manifest = RunManifest.from_dict(read_gz_json(result.run_dir / 'run.json.gz')).data
            self.assertEqual(manifest['order'], {'scheme': 'ABAB', 'block': 2, 'arms': ['base', 'cand'],
                                                 'warmup_journeys': 1, 'interleaved': True,
                                                 'position': position})
            self.assertEqual(manifest['machine']['cpus'], self.cpu)
            self.assertEqual(manifest['isolation']['processes'][0]['cpus_allowed'], self.cpu)
            calls = [CallRecord.from_dict(r) for r in read_gz_lines(result.run_dir / 'calls.jsonl.gz')]
            journeys = [JourneyRecord.from_dict(r) for r in read_gz_lines(result.run_dir / 'journeys.jsonl.gz')]
            self.assertEqual(len(calls), 6)
            self.assertEqual(len(journeys), 6)
            self.assertEqual({c.phase for c in calls}, {'warm', 'repeat'})
            self.assertTrue(all(c.wall_ns > 0 and c.server_ms == 1 for c in calls))
            self.assertIn('warm-up', journeys[0].absent['startup_ns'])
            _, _, verification = verify(result.run_dir, 1 << 20)
            self.assertTrue(verification['ok'])
            self.assertEqual(len(verification['journeys']), 6)
        base_calls = read_gz_lines(base.run_dir / 'calls.jsonl.gz')
        self.assertIsNone(base_calls[0]['jev'])
        self.assertIn('predates BT03', base_calls[0]['absent']['jev'])
        cand_calls = read_gz_lines(cand.run_dir / 'calls.jsonl.gz')
        self.assertEqual(cand_calls[0]['server_us'], 1234)
        self.assertEqual([e['site'] for e in cand_calls[0]['jev']['evaluations']], ['rerank'])
        self.assertEqual(cand_calls[2]['jev'], {'evaluations': []})
        cand_manifest = read_gz_json(cand.run_dir / 'run.json.gz')
        self.assertTrue(cand_manifest['isolation']['variant_applied'])
        base_manifest = read_gz_json(base.run_dir / 'run.json.gz')
        self.assertEqual(base_manifest['failures'], [])
        again = run_arms(arms, self.plan(questions, block=2, repeats=2, warmup=1), self.layout)
        self.assertEqual([r.cached for r in again], [True, True])
        self.assertEqual(again[0].run_id, base.run_id)

    def test_timeouts_are_censored_and_the_next_journey_gets_a_fresh_process(self):
        questions = [question('q0', 'plain'), question('q1', 'hang'), question('q2', 'plain')]
        [result] = run_arms([self.arm('solo', True)],
                            self.plan(questions, warmup=0, timeout_s=0.5), self.layout)
        calls = read_gz_lines(result.run_dir / 'calls.jsonl.gz')
        journeys = {j['question_id']: j for j in read_gz_lines(result.run_dir / 'journeys.jsonl.gz')}
        hung = next(c for c in calls if c['question_id'] == 'q1')
        self.assertEqual((hung['status'], hung['censored'], hung['censor_reason']),
                         ('transport_error', True, 'timeout'))
        self.assertGreaterEqual(hung['wall_ns'], 500_000_000)
        self.assertIsNone(hung['server_ms'])
        self.assertIn('no kmp_mcp_tool line', hung['absent']['server_ms'])
        self.assertEqual((journeys['q1']['status'], journeys['q1']['censor_reason']),
                         ('transport_error', 'timeout'))
        self.assertEqual(journeys['q0']['startup_ns'] > 0, True)
        self.assertGreater(journeys['q2']['startup_ns'], 0)  # a new process served it
        first = next(c for c in calls if c['question_id'] == 'q0')
        after = next(c for c in calls if c['question_id'] == 'q2')
        self.assertEqual((first['phase'], after['phase'], after['process_call_index']), ('first', 'first', 0))
        self.assertEqual(after['server_us'], 1234)
        manifest = read_gz_json(result.run_dir / 'run.json.gz')
        self.assertEqual([p['process'] for p in manifest['isolation']['processes']], [0, 1])
        self.assertEqual(manifest['order']['scheme'], 'single')
        _, _, verification = verify(result.run_dir, 1 << 20)
        self.assertTrue(verification['ok'])

    def test_next_action_is_followed_verbatim_until_none_or_the_cap(self):
        questions = [question('pages', 'three pages')]
        [full] = run_arms([self.arm('pager', True)], self.plan(questions, warmup=0), self.layout)
        calls = read_gz_lines(full.run_dir / 'calls.jsonl.gz')
        self.assertEqual([c['arguments'] for c in calls[1:]], [{'continuation': 'p1'}, {'continuation': 'p2'}])
        self.assertEqual([c['call_index'] for c in calls], [0, 1, 2])
        self.assertEqual([c['response_path'] for c in calls],
                         [f'pages~s0~r0.jsonl#{n}' for n in (1, 4, 7)])
        # max_calls is not part of the result key (the mode pins it), so the capped run is another mode
        capped_plan = RunPlan(tuple(questions), mode='test-capped', cpus=self.cpu, warmup=0, max_calls=2)
        [capped] = run_arms([self.arm('capped', True)], capped_plan, self.layout)
        [journey] = read_gz_lines(capped.run_dir / 'journeys.jsonl.gz')
        self.assertEqual((journey['status'], journey['censor_reason'], journey['calls']),
                         ('call_cap_reached', 'max_calls', 2))

    def test_tool_errors_and_unapplied_store_files_are_reported(self):
        arm = self.arm('bad', True, store_files={'ask-gate.json': {'json': {'on': True}}})
        [result] = run_arms([arm], self.plan([question('boom', 'boom')], warmup=0), self.layout)
        [call] = read_gz_lines(result.run_dir / 'calls.jsonl.gz')
        self.assertEqual(call['status'], 'tool_error')
        manifest = read_gz_json(result.run_dir / 'run.json.gz')
        self.assertFalse(manifest['isolation']['variant_applied'])
        self.assertEqual(manifest['failures'][0]['code'], 'VARIANT_NOT_APPLIED')
        self.assertIn('not read by this kmp-mcp version', manifest['failures'][0]['detail'])

    def test_refusals(self):
        with self.assertRaises(RunRefused):
            run_arms([self.arm('p', True)], self.plan([question('x', 'plain', private=True)]), self.layout)
        with self.assertRaises(RunRefused):
            run_arms([], self.plan([question('x', 'plain')]), self.layout)
        with self.assertRaises(RunRefused):
            run_arms([self.arm('same', True), self.arm('same', False)], self.plan([question('x', 'y')]),
                     self.layout)
        for bad in ({'repeats': 0}, {'max_calls': 257}, {'warmup': -1}):
            with self.subTest(bad=bad), self.assertRaises(RunRefused):
                run_arms([self.arm(f'a{len(bad)}{list(bad)[0][0]}', True)],
                         self.plan([question('x', 'plain')], **bad), self.layout)
        with self.assertRaises(RunRefused):
            RunPlan((), mode='test').check()

    def test_an_arm_whose_binary_cannot_start_is_a_recorded_failure(self):
        arm = self.arm('broken', True)
        broken = self.root / 'broken-binary'
        broken.write_text('#!/bin/sh\nexit 3\n')
        broken.chmod(0o755)
        arm = Arm(arm.variant, arm.variant_path, broken, arm.store)
        [result] = run_arms([arm], self.plan([question('x', 'plain')], warmup=0, timeout_s=2.0), self.layout)
        manifest = read_gz_json(result.run_dir / 'run.json.gz')
        codes = [f['code'] for f in manifest['failures']]
        self.assertEqual(len(codes), 2)  # the start fails as TRANSPORT_FAILED or a broken pipe
        self.assertIn(codes[0], ('TRANSPORT_FAILED', 'PROCESS_START_FAILED'))
        self.assertEqual(codes[1], 'PROCESS_UNAVAILABLE')
        self.assertEqual(read_gz_lines(result.run_dir / 'journeys.jsonl.gz'), [])


def read_gz_json(path):
    return json.loads(gzip.decompress(path.read_bytes()))


class WriteCostTest(FakeBinaryCase):
    def test_twenty_single_entry_writes_give_p50_and_p95(self):
        out = self.root / 'wc'
        result = measure_write_cost(self.binary, self.root / 'work', WritePlan(cpus=self.cpu),
                                    template=self.template('wt', True), out_dir=out)
        self.assertEqual((result['writes'], result['accepted'], result['censored']), (20, 20, 0))
        self.assertEqual(result['wall_ns']['n'], 20)
        self.assertLessEqual(result['wall_ns']['p50'], result['wall_ns']['p95'])
        self.assertEqual(result['server_us']['p50'], 1234)
        self.assertEqual(result['cpus_allowed'], self.cpu)
        self.assertEqual(json.loads((out / 'write-cost.json').read_text())['schema'], 'kmp.bench.write_cost.v1')
        self.assertTrue((out / 'write-cost~w019.jsonl').is_file())
        self.assertFalse((out / 'write-cost~w020.jsonl').exists())

    def test_entries_are_deterministic_single_entry_ingests(self):
        first, later = ingest_arguments('bench:w', 0), ingest_arguments('bench:w', 7)
        self.assertEqual(first['memory']['dimensions'], [{'id': 'work:main', 'kind': 'work'}])
        self.assertEqual(later['memory']['dimensions'], [])
        self.assertEqual(len(later['memory']['entries']), 1)
        self.assertEqual(later, ingest_arguments('bench:w', 7))
        self.assertEqual(cachekey.canonical_json(later), cachekey.canonical_json(ingest_arguments('bench:w', 7)))

    def test_a_refused_write_fails_the_measure(self):
        with self.assertRaises(WriteCostFailed):
            measure_write_cost(self.binary, self.root / 'work', WritePlan(writes=2, warmup=0, about='refuse:x',
                                                                          cpus=None),
                               template=self.template('wr', False))
        with self.assertRaises(WriteCostFailed):
            WritePlan(writes=1).check()


if __name__ == '__main__':
    unittest.main()
