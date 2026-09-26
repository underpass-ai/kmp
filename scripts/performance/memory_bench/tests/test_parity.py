"""Parity oracle and determinism control (BT09).

The oracle is fed real `TraceRecorder` lines; the determinism runner drives a
tiny stand-in MCP server over stdio, so no cargo build is needed. The stand-in
mints a random read handle per page and stamps writes with the wall clock, the
two kinds of A/A noise the release binary shows.
"""
import contextlib
import gzip
import io
import json
from pathlib import Path
import stat
import sys
import tempfile
import textwrap
import unittest

from ...token_harness.native.recorder import TraceRecorder
from ..application import determinism as D
from ..application import parity as P
from ..domain.errors import PrivacyViolation
from ..domain.jsonl import REPO_ROOT

HANDLE_A, HANDLE_B = 'read_' + 'a' * 32, 'read_' + 'b' * 32
CURSOR_A, CURSOR_B = 'kmp1:3:' + '1' * 64, 'kmp1:3:' + '2' * 64


def request(identifier, tool, arguments):
    return json.dumps({'jsonrpc': '2.0', 'id': identifier, 'method': 'tools/call',
                       'params': {'name': tool, 'arguments': arguments}})


def response(identifier, structured):
    return json.dumps({'jsonrpc': '2.0', 'id': identifier,
                       'result': {'content': [{'type': 'text', 'text': 'ok'}],
                                  'structuredContent': structured}}, ensure_ascii=False)


def page(summary, handle=None, cursor=None, **extra):
    action = {'tool': 'kmp_wake', 'arguments': {'continuation': handle}} if handle else None
    return {'summary': summary, 'projection': {'next_action': action,
                                               'page': {'next_cursor': cursor}}, **extra}


def wake_pairs(handle, cursor=CURSOR_A, second=None, sent=None):
    """A two-page wake: the first page returns `handle`, the second request sends `sent`."""
    return [(request(3, 'kmp_wake', {'about': 'fixture:x'}), response(3, page('one', handle, cursor))),
            (request(4, 'kmp_wake', {'continuation': sent or handle}),
             response(4, second or page('two')))]


def write_trace(path, pairs, markers=True):
    with TraceRecorder(Path(path)) as recorder:
        if markers:
            recorder.marker({'session_start': 'x'})
            recorder.pair('x', json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize',
                                           'params': {}}), '{"jsonrpc":"2.0","id":1,"result":{}}')
        for raw_request, raw_response in pairs:
            recorder.pair('x', raw_request, raw_response)
            if markers:
                recorder.marker({'timing': {'wall_ns': 7}})


class Folder(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)

    def capture(self, name, traces):
        folder = Path(tempfile.mkdtemp(prefix=name + '-', dir=self.root))
        for trace, pairs in traces.items():
            write_trace(folder / f'{trace}.jsonl', pairs)
        return P.load_capture(folder)

    def compare(self, left, right):
        return P.compare_captures(self.capture('left', left), self.capture('right', right))


class TraceReadingTest(Folder):
    def test_only_tools_call_exchanges_are_read_with_their_exact_bytes(self):
        pairs = wake_pairs(HANDLE_A)
        capture = self.capture('c', {'q1': pairs})
        exchanges = capture['q1']
        self.assertEqual([e.index for e in exchanges], [0, 1])
        self.assertEqual([(e.request, e.response) for e in exchanges], pairs)
        self.assertEqual(exchanges[0].tool, 'kmp_wake')

    def test_a_sealed_capture_is_read_through_its_manifest_and_gzip(self):
        folder = self.root / 'sealed'
        folder.mkdir()
        write_trace(folder / 'q1.jsonl', wake_pairs(HANDLE_A))
        write_trace(folder / 'process-0.jsonl', wake_pairs(HANDLE_A))
        names = {}
        for path in sorted(folder.iterdir()):
            names[path.name] = {'bytes': 0, 'sha256': '0'}
            path.with_suffix('.jsonl.gz').write_bytes(gzip.compress(path.read_bytes()))
            path.unlink()
        (folder / 'manifest.json').write_text(json.dumps({'uncompressed_files': names}))
        capture = P.load_capture(folder)
        self.assertEqual(sorted(capture), ['q1'])  # process-* holds handshakes only
        self.assertEqual(len(capture['q1']), 2)

    def test_listed_but_missing_and_empty_captures_are_refused(self):
        folder = self.root / 'broken'
        folder.mkdir()
        (folder / 'manifest.json').write_text(json.dumps({'uncompressed_files': {'q.jsonl': {}}}))
        with self.assertRaises(P.ParityInputError):
            P.load_capture(folder)
        empty = self.root / 'empty'
        empty.mkdir()
        with self.assertRaises(P.ParityInputError):
            P.load_capture(empty)
        with self.assertRaises(P.ParityInputError):
            P.load_capture(self.root / 'absent')

    def test_a_truncated_line_is_refused_not_skipped(self):
        folder = self.root / 'cut'
        folder.mkdir()
        (folder / 'q.jsonl').write_text('{"session":"x","request":{"id":1},"response":{"id":1\n')
        with self.assertRaises(P.ParityInputError):
            P.load_capture(folder)


class ParityTest(Folder):
    def test_the_same_bytes_are_identical(self):
        report = self.compare({'q1': wake_pairs(HANDLE_A)}, {'q1': wake_pairs(HANDLE_A)})
        self.assertEqual((report['identical'], report['normalized'], report['different']), (2, 0, 0))
        self.assertEqual(report['parity_rate'], 1.0)
        self.assertEqual(report['byte_identical_rate'], 1.0)
        self.assertEqual(set(report['volatile_reconciled'].values()), {0})

    def test_a_fresh_handle_linked_the_same_way_is_normalized_and_counted(self):
        report = self.compare({'q1': wake_pairs(HANDLE_A)}, {'q1': wake_pairs(HANDLE_B)})
        self.assertEqual((report['normalized'], report['different']), (2, 0))
        self.assertEqual(report['parity_rate'], 1.0)
        self.assertEqual(report['byte_identical_rate'], 0.0)
        # returned by page one, sent back by page two
        self.assertEqual(report['volatile_reconciled']['continuation'], 2)

    def test_a_broken_link_is_a_difference_even_between_volatile_values(self):
        other = 'read_' + 'c' * 32
        report = self.compare({'q1': wake_pairs(HANDLE_A)},
                              {'q1': wake_pairs(HANDLE_B, sent=other)})
        self.assertEqual(report['different'], 1)
        self.assertEqual(report['differences'][0]['paths'],
                         [{'path': 'request.params.arguments.continuation', 'kind': 'value'}])

    def test_one_changed_byte_is_found_and_its_json_path_listed(self):
        changed = page('twO')
        report = self.compare({'q1': wake_pairs(HANDLE_A)},
                              {'q1': wake_pairs(HANDLE_A, second=changed)})
        self.assertEqual(report['different'], 1)
        row = report['differences'][0]
        self.assertEqual((row['trace'], row['index'], row['tool']), ('q1', 1, 'kmp_wake'))
        self.assertEqual(row['paths'], [{'path': '$.result.structuredContent.summary', 'kind': 'value'}])
        self.assertEqual(report['path_counts'], {'$.result.structuredContent.summary': 1})

    def test_one_changed_byte_in_the_raw_line_is_found_even_inside_a_list(self):
        left = wake_pairs(HANDLE_A, second=page('two', refs=['entry:a', 'entry:b']))
        right = [left[0], (left[1][0], left[1][1].replace('entry:b', 'entry:c'))]
        report = self.compare({'q1': left}, {'q1': right})
        self.assertEqual(report['differences'][0]['paths'],
                         [{'path': '$.result.structuredContent.refs[1]', 'kind': 'value'}])
        self.assertEqual(report['path_counts'], {'$.result.structuredContent.refs[]': 1})

    def test_the_same_json_in_other_bytes_is_not_parity(self):
        left = wake_pairs(HANDLE_A)
        spaced = json.dumps(json.loads(left[1][1]), indent=1)
        report = self.compare({'q1': left}, {'q1': [left[0], (left[1][0], spaced.replace('\n', ' '))]})
        self.assertEqual(report['differences'][0]['paths'], [{'path': '$', 'kind': 'serialization'}])

    def test_a_cursor_that_turns_null_is_a_difference(self):
        report = self.compare({'q1': wake_pairs(HANDLE_A, cursor=CURSOR_A)},
                              {'q1': wake_pairs(HANDLE_A, cursor=None)})
        self.assertEqual(report['differences'][0]['paths'][0]['kind'], 'type')
        report = self.compare({'q1': wake_pairs(HANDLE_A, cursor=CURSOR_A)},
                              {'q1': wake_pairs(HANDLE_A, cursor=CURSOR_B)})
        self.assertEqual(report['different'], 0)
        self.assertEqual(report['volatile_reconciled']['next_cursor'], 1)

    def test_a_volatile_key_with_another_shape_is_not_normalized(self):
        report = self.compare({'q1': wake_pairs(HANDLE_A, cursor='page-2')},
                              {'q1': wake_pairs(HANDLE_A, cursor='page-3')})
        self.assertEqual(report['different'], 1)

    def test_only_ingestion_clocks_are_set_aside_among_timestamps(self):
        def answer(ingested, observed, occurred):
            clocks = {'ingested_at': ingested, 'observed_at': observed, 'occurred_at': occurred}
            return [(request(3, 'kmp_ask', {'about': 'a:b', 'question': 'q'}),
                     response(3, {'memory': {'clocks': {'ingested': {'single_value': ingested}}},
                                  'proof': {'path': [{'clocks': clocks}]}}))]
        t1, t2 = '2026-09-26T08:00:00.1Z', '2026-09-26T08:00:05.2Z'
        given = '2026-08-01T09:00:00Z'
        # observed_at defaulted to the ingestion instant: linked, reconciled
        report = self.compare({'q': answer(t1, t1, given)}, {'q': answer(t2, t2, given)})
        self.assertEqual(report['different'], 0)
        self.assertEqual(report['volatile_reconciled']['observed_at=ingested'], 1)
        self.assertEqual(report['volatile_reconciled']['clocks.ingested'], 1)
        # an observed_at the writer gave is never set aside
        report = self.compare({'q': answer(t1, given, given)}, {'q': answer(t2, t1, given)})
        self.assertEqual(report['differences'][0]['paths'],
                         [{'path': '$.result.structuredContent.proof.path[0].clocks.observed_at',
                           'kind': 'value'}])
        # nor an occurred_at
        report = self.compare({'q': answer(t1, t1, given)}, {'q': answer(t2, t2, t1)})
        self.assertEqual(report['different'], 1)

    def test_one_instant_in_two_spellings_keeps_one_label(self):
        def write(nine, trimmed):
            return [(request(2, 'kmp_write_memory', {'about': 'a:b'}),
                     response(2, {'clocks': {'ingested': {'single_value': nine}},
                                  'neighborhood': {'items': [{'clocks': {'ingested_at': [trimmed]}}]}}))]
        # run A happens to land on an instant without a trailing zero, run B with one
        left = write('2026-09-26T08:00:00.198401687Z', '2026-09-26T08:00:00.198401687Z')
        right = write('2026-09-26T08:00:05.134080070Z', '2026-09-26T08:00:05.13408007Z')
        report = self.compare({'w': left}, {'w': right})
        self.assertEqual(report['different'], 0)
        self.assertEqual(P.Normalizer._key(P.VOLATILE_FIELDS[2], '2026-09-26T08:00:00.500+02:00'),
                         ('ingested', '2026-09-26T08:00:00.5+02:00'))
        self.assertEqual(P.Normalizer._key(P.VOLATILE_FIELDS[2], '2026-09-26T08:00:00.000Z'),
                         ('ingested', '2026-09-26T08:00:00Z'))

    def test_a_setup_trace_labels_stamps_before_its_journey_reads_them(self):
        def item(stamp):
            write = [(request(2, 'kmp_ingest', {'about': 'a:b'}),
                      response(2, {'clocks': {'ingested': {'single_value': stamp}}}))]
            ask = [(request(3, 'kmp_ask', {'about': 'a:b'}),
                    response(3, {'proof': {'path': [{'clocks': {'observed_at': stamp}}]}}))]
            return {'case': ask, 'case.setup': write}
        report = self.compare(item('2026-09-26T08:00:00Z'), item('2026-09-26T09:00:00Z'))
        self.assertEqual(report['different'], 0)
        self.assertEqual(report['by_kind'], {'journey': {'calls': 1, 'parity_rate': 1.0},
                                             'setup': {'calls': 1, 'parity_rate': 1.0}})

    def test_missing_calls_and_traces_are_differences(self):
        report = self.compare({'q1': wake_pairs(HANDLE_A), 'q2': wake_pairs(HANDLE_A)},
                              {'q1': wake_pairs(HANDLE_A)[:1]})
        self.assertEqual(report['calls'], 4)
        self.assertEqual(report['different'], 3)
        kinds = sorted(p['kind'] for row in report['differences'] for p in row['paths'])
        self.assertEqual(kinds, ['only_left'] * 3)
        report = P.compare_captures({}, self.capture('r', {'q': wake_pairs(HANDLE_A)}))
        self.assertEqual({p['kind'] for row in report['differences'] for p in row['paths']},
                         {'only_right'})

    def test_many_differing_paths_are_truncated_and_counted(self):
        left = {f'k{i}': i for i in range(P.MAX_LISTED_PATHS + 5)}
        right = {key: value + 1 for key, value in left.items()}
        report = self.compare({'q': [(request(3, 'kmp_ask', {}), response(3, left))]},
                              {'q': [(request(3, 'kmp_ask', {}), response(3, right))]})
        row = report['differences'][0]
        self.assertEqual((len(row['paths']), row['truncated']), (P.MAX_LISTED_PATHS, 5))

    def test_json_diff_names_every_kind_and_quotes_odd_keys(self):
        found = P.json_diff({'a': [1, 2], 'b': 1, 'c d': 'x', 'e': 1},
                            {'a': [1], 'b': '1', 'c d': 'y', 'f': 1})
        self.assertEqual(found, [('$.a', 'length'), ('$.b', 'type'), ('$["c d"]', 'value'),
                                 ('$.e', 'only_left'), ('$.f', 'only_right')])

    def test_the_rules_are_explicit_and_justified(self):
        self.assertEqual([r.name for r in P.VOLATILE_FIELDS],
                         ['continuation', 'next_cursor', 'ingested_at', 'clocks.ingested',
                          'observed_at=ingested', 'review_token'])
        for rule in P.VOLATILE_FIELDS:
            self.assertGreater(len(rule.reason), 40, rule.name)
        report = self.compare({'q': wake_pairs(HANDLE_A)}, {'q': wake_pairs(HANDLE_A)})
        self.assertEqual([r['name'] for r in report['volatile_fields']],
                         [r.name for r in P.VOLATILE_FIELDS])

    def test_unparseable_lines_differ_only_when_their_bytes_do(self):
        left = P.Exchange('t', 0, 'kmp_ask', '{"id":1}', 'not json')
        same = P.compare_exchange(left, left, P.Normalizer(), P.Normalizer())
        self.assertEqual(same.status, 'identical')
        other = P.Exchange('t', 0, 'kmp_ask', '{"id":1}', 'not json!')
        row = P.compare_exchange(left, other, P.Normalizer(), P.Normalizer())
        self.assertEqual(row.paths, [('$', 'unparseable')])


class RenderedAndBaselineTest(Folder):
    def views(self, name, content):
        path = self.root / name
        view = {'about': 'a:b', 'revision': 1, 'content_hash': 'h',
                'rendered': {'content': content, 'content_hash': 'r', 'token_count': 3}}
        path.write_text(json.dumps({'id': 'wake-1', 'view': view}) + '\n')
        return P.load_views(path)

    def test_rendered_content_is_compared_byte_for_byte(self):
        same = P.compare_rendered(self.views('a', '# Context é'), self.views('b', '# Context é'))
        self.assertEqual(same['content_parity_rate'], 1.0)
        self.assertEqual(same['differences'], [])
        other = P.compare_rendered(self.views('c', '# Context é'), self.views('d', '# Context e'))
        self.assertEqual(other['content_parity_rate'], 0.0)
        self.assertEqual(other['differences'][0]['paths'],
                         [{'path': '$.rendered.content', 'kind': 'value'}])
        missing = P.compare_rendered(self.views('e', 'x'), {})
        self.assertEqual(missing['differences'][0]['paths'][0]['kind'], 'only_left')

    def test_views_without_rendered_content_are_refused(self):
        path = self.root / 'bad.json'
        path.write_text(json.dumps([{'about': 'a:b'}]))
        with self.assertRaises(P.ParityInputError):
            P.load_views(path)

    def test_baseline_py_results_are_read_through_baseline_compare(self):
        def output(name, value):
            folder = self.root / name
            (folder / 'shape-a').mkdir(parents=True)
            (folder / 'results.json').write_text(json.dumps([{'shape': 'shape-a', 'rows': []}]))
            with gzip.open(folder / 'shape-a/read-results.json.gz', 'wt') as target:
                json.dump({'wake': {'structuredContent': {'summary': value}}, 'ask': {'x': 1}}, target)
            return P.load_capture(folder)
        report = P.compare_captures(output('b', 'same'), output('c', 'same'))
        self.assertEqual((report['calls'], report['identical']), (2, 2))
        report = P.compare_captures(output('d', 'same'), output('e', 'other'))
        self.assertEqual(report['path_counts'], {'$.structuredContent.summary': 1})

    def test_the_cli_exits_by_parity_and_writes_its_report(self):
        for name, handle in (('l', HANDLE_A), ('r', HANDLE_B), ('x', None)):
            (self.root / name).mkdir()
            second = page('changed') if handle is None else None
            write_trace(self.root / name / 'q.jsonl', wake_pairs(handle or HANDLE_A, second=second))
        (self.root / 'v1.json').write_text(json.dumps({'rendered': {'content': 'a'}}))
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = P.main([str(self.root / 'l'), str(self.root / 'r'), '--out',
                           str(self.root / 'report.json'), '--rendered', str(self.root / 'v1.json'),
                           str(self.root / 'v1.json')])
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out.getvalue())['rendered_content_parity_rate'], 1.0)
        self.assertEqual(json.loads((self.root / 'report.json').read_text())['schema'], P.SCHEMA)
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(P.main([str(self.root / 'l'), str(self.root / 'x')]), 1)
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(P.main([str(self.root / 'l'), str(self.root / 'nothing')]), 2)


# A stand-in kmp-mcp: random read handles, wall-clock write stamps, optional flakiness.
FAKE_SERVER = textwrap.dedent('''\
    #!{python}
    import datetime, json, os, secrets, sys
    data = os.environ['KMP_MCP_DATA_DIR']
    print(json.dumps({{'fields': {{'message': 'embedded backend data dir resolved', 'data_dir': data,
                                   'rule': 'env', 'requested_engine': 'sqlite'}}}}), file=sys.stderr,
          flush=True)
    flaky = os.path.exists(os.path.join(data, 'flaky'))
    handles, stamp_file = {{}}, os.path.join(data, 'stamp')
    def now():
        return datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%S.%fZ')
    def call(name, args):
        if name == 'kmp_ingest':
            stamp = now()
            open(stamp_file, 'w').write(stamp)
            return {{'accepted': True, 'clocks': {{'ingested': {{'single_value': stamp}}}}}}
        if name == 'kmp_ask':
            stamp = open(stamp_file).read() if os.path.exists(stamp_file) else None
            return {{'answer': args['question'].upper(),
                     'proof': {{'path': [{{'clocks': {{'observed_at': stamp, 'ingested_at': stamp}}}}]}}}}
        if 'continuation' in args:
            about = handles.pop(args['continuation'])
            return {{'summary': about + ' page two', 'projection': {{'next_action': None}}}}
        handle = 'read_' + secrets.token_hex(16)
        handles[handle] = args['about']
        summary = args['about'] + (secrets.token_hex(2) if flaky else '')
        return {{'summary': summary, 'projection': {{
            'next_action': {{'tool': 'kmp_wake', 'arguments': {{'continuation': handle}}}}}}}}
    for line in sys.stdin:
        message = json.loads(line)
        if 'id' not in message:
            continue
        if message['method'] == 'initialize':
            result = {{'protocolVersion': '2024-11-05', 'serverInfo': {{'name': 'fake', 'version': '0'}}}}
        elif message['method'] == 'tools/list':
            result = {{'tools': []}}
        else:
            params = message['params']
            structured = call(params['name'], params['arguments'])
            result = {{'content': [{{'type': 'text', 'text': 'ok'}}], 'structuredContent': structured}}
        print(json.dumps({{'jsonrpc': '2.0', 'id': message['id'], 'result': result}}), flush=True)
''')


class DeterminismTest(Folder):
    def setUp(self):
        super().setUp()
        self.binary = self.root / 'fake-kmp-mcp'
        self.binary.write_text(FAKE_SERVER.format(python=sys.executable))
        self.binary.chmod(self.binary.stat().st_mode | stat.S_IXUSR)
        self.template = self.root / 'template'
        (self.template / 'store').mkdir(parents=True)
        (self.template / 'FORMAT_VERSION').write_text('4\n')

    def items(self):
        fresh = D.WorkItem('retrieval.case', 'kmp_ask', {'about': 'a:b', 'question': 'why?',
                                                         'answer_policy': 'evidence_or_unknown'},
                           (('kmp_ingest', {'about': 'a:b', 'memory': {}, 'idempotency_key': 'k'}),),
                           True)
        return D.wake_items(['project:one', 'fixture:two']) + (fresh,)

    def test_fresh_processes_agree_after_the_documented_volatile_fields(self):
        report = D.determinism(self.binary, self.items(), self.root / 'out', runs=3,
                               template=self.template, max_calls=256, timeout_seconds=20)
        self.assertEqual(report['determinism_rate'], 1.0)
        self.assertEqual(report['calls'], 6)  # 2 wakes x 2 pages, 1 setup write, 1 ask
        self.assertEqual(report['by_kind']['setup']['calls'], 1)
        self.assertEqual(len(report['pairs']), 2)
        pair = report['pairs'][0]
        self.assertEqual(pair['volatile_reconciled']['continuation'], 4)
        self.assertEqual(pair['volatile_reconciled']['clocks.ingested'], 1)
        self.assertEqual(pair['volatile_reconciled']['observed_at=ingested'], 1)
        self.assertEqual(report['outcomes'][0]['statuses'], {'completed': 3})
        self.assertEqual(report['machine']['arch'], D.machine()['arch'])
        self.assertTrue((self.root / 'out/run-2/wake.project_one.jsonl').is_file())
        self.assertTrue((self.root / 'out/run-0/retrieval.case.setup.jsonl').is_file())
        self.assertFalse((self.root / 'out/work').exists())
        run = json.loads((self.root / 'out/run-1/determinism-run.json').read_text())
        self.assertEqual(run['schema'], D.RUN_SCHEMA)
        self.assertEqual(run['items_digest'], report['items_digest'])

    def test_a_flaky_binary_is_caught_with_its_paths(self):
        (self.template / 'flaky').write_text('')
        report = D.determinism(self.binary, D.wake_items(['project:one']), self.root / 'out',
                               runs=2, template=self.template, timeout_seconds=20)
        self.assertLess(report['determinism_rate'], 1.0)
        self.assertEqual(report['pairs'][0]['path_counts'], {'$.result.structuredContent.summary': 1})

    def test_the_call_cap_censors_a_long_journey(self):
        report = D.determinism(self.binary, D.wake_items(['project:one']), self.root / 'out',
                               runs=2, template=self.template, max_calls=1, timeout_seconds=20)
        self.assertEqual(report['outcomes'][0], {'statuses': {'call_cap_reached': 1}, 'censored': 1,
                                                 'calls': 1})

    def test_guards_refuse_bad_setups(self):
        with self.assertRaises(D.DeterminismError):
            D.determinism(self.binary, self.items(), self.root / 'a', runs=1, template=self.template)
        with self.assertRaises(D.DeterminismError):
            D.check_template(self.root)
        with self.assertRaises(D.DeterminismError):
            D.run_once(self.binary, D.wake_items(['x:y']), self.root / 'b', self.root / 'w')
        with self.assertRaises(D.DeterminismError):
            D.items_digest(D.wake_items(['x:y', 'x:y']))
        with self.assertRaises(PrivacyViolation):
            D.determinism(self.binary, self.items(), REPO_ROOT / 'tmp/memory-bench/work/never',
                          template=self.template, private=True)

    def test_retrieval_items_mirror_the_scorecard(self):
        cases = {'cases': [
            {'id': 'plain', 'about': 'p:a', 'question': 'q?', 'answer_policy': 'best_effort',
             'memory': {'entries': []}, 'as_of': {'time': '2026-01-01T00:00:00Z'},
             'memories': [{'about': 'p:b', 'memory': {'entries': []}}],
             'writes': [{'about': 'p:a', 'relations': []}]},
            {'id': 'negative', 'kind': 'anchor_absent', 'about': 'p:a', 'question': 'q',
             'answer_policy': 'evidence_or_unknown', 'memory': {}}]}
        path = self.root / 'cases.json'
        path.write_text(json.dumps(cases))
        (item,) = D.retrieval_items(path)
        self.assertEqual(item.id, 'retrieval.plain')
        self.assertTrue(item.fresh_store)
        self.assertEqual([tool for tool, _ in item.setup], ['kmp_ingest', 'kmp_ingest', 'kmp_write_memory'])
        self.assertEqual(item.arguments['budget'], {'tokens': 2048, 'detail': 'balanced', 'max_entries': 10})
        self.assertEqual(item.arguments['as_of'], {'time': '2026-01-01T00:00:00Z'})
        self.assertEqual(item.arguments['depth'], 3)
        self.assertEqual(len(D.retrieval_items(path, subset='all')), 2)

    def test_the_cli_runs_wakes_on_a_template(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = D.main(['--binary', str(self.binary), '--out', str(self.root / 'cli'),
                           '--template', str(self.template), '--wake-about', 'project:one',
                           '--runs', '2', '--timeout', '20'])
        self.assertEqual(code, 0)
        summary = json.loads(out.getvalue())
        self.assertEqual(summary['determinism_rate'], 1.0)
        # a template outside the repository makes the run private
        self.assertTrue(json.loads((self.root / 'cli/determinism.json').read_text())['private'])


if __name__ == '__main__':
    unittest.main()
