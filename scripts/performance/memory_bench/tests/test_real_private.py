"""B-real (BT07): intake, freeze, blind pools, label schema, kappa, resolution, privacy.

Everything here is synthetic: invented transcripts, a tiny SQLite store in the
shape KMP writes, a fake `kmp-mcp` that only answers `--version` and `export`,
and a fake kernel. Temporary directories come from `tempfile`, outside the
repository, and one group of tests proves that no B-real writer accepts a path
inside it.
"""
import contextlib
import io
import json
import os
from pathlib import Path
import sqlite3
import stat
import tempfile
import unittest
from unittest import mock

from .. import cli as BENCH_CLI
from ..corpora import real_freeze as RF
from ..corpora import real_private as RP
from ..corpora.store_snapshot import AboutDocs, EntryDoc, read_store
from ..domain.errors import PrivacyViolation
from ..domain.gold import ANSWERABLE, SHAPES, UNKNOWN_REASONS, Gold
from ..domain.jsonl import REPO_ROOT
from ..domain.question import write_questions
from ..labeling import agreement as AG
from ..labeling import cli as CLI
from ..labeling import labels as LB
from ..labeling import pool as PL
from ..labeling.schema import SchemaViolation, load_schema, validate
from ..labeling.session import LabelingSession, search

ABOUT = 'project:demo'
REFS = [f'{ABOUT}:entry:decision:e{i:02d}' for i in range(40)]
TEXTS = {REFS[0]: 'Issue #188 pauses the release until preflight passes.',
         REFS[1]: 'C6.4 local execution adapter keeps one process per task.',
         REFS[2]: 'The #188 preflight checks the host fence before resuming.',
         REFS[3]: 'Unrelated note about the viewer colours.'}
RULES = PL.PoolRules(seed=7, kernel_top=5, random=4, answer_policy='best_effort',
                     budget={'max_entries': 5}, max_calls=3)


def about_docs():
    docs = tuple(EntryDoc(ref, ABOUT, 'decision', True, TEXTS.get(ref, f'Note number {i} on storage.'),
                          (TEXTS.get(ref, f'Note number {i} on storage.'),), 1)
                 for i, ref in enumerate(REFS))
    return AboutDocs(ABOUT, docs, (), ())


def gold(shape='enumerative', facets=None, wrong=()):
    facets = facets or [{'name': 'state', 'words': 'implementation state', 'answers': [REFS[0]],
                         'related': [REFS[2]], 'answerable_facet': True},
                        {'name': 'work', 'words': 'outstanding work', 'answers': [],
                         'related': [], 'answerable_facet': False}]
    known = any(f['answers'] for f in facets)
    return {'answerable': 'KNOWN' if known else 'UNKNOWN', 'shape': shape, 'facets': facets,
            'wrong_subject': sorted(wrong), 'chain': None, 'path': None, 'current_ref': None,
            'stale_refs': [], 'nearest_outside': None, 'unknown_reason': None, 'absent_terms': [],
            'excluded_refs': [], 'wake_required': []}


def intake(question='What implementation state and outstanding work exist for issue #188?', about=ABOUT):
    call = {'about': about, 'question': question} if about else {'question': question}
    return RP.make_intake(call, '2026-09-19T21:40:00Z', 'codex', '0' * 64)


def sqlite_store(data_dir):
    """A KMP-shaped data directory: FORMAT_VERSION and store/kernel.sqlite3 with the fixture entries."""
    path = Path(data_dir) / 'store' / 'kernel.sqlite3'
    path.parent.mkdir(parents=True)
    (Path(data_dir) / 'FORMAT_VERSION').write_text('4\n')
    connection = sqlite3.connect(path)
    connection.execute('CREATE TABLE nodes (k TEXT PRIMARY KEY, v BLOB)')
    connection.execute('CREATE TABLE details (k TEXT PRIMARY KEY, v BLOB)')
    connection.execute('CREATE TABLE relations_by_source (k1 TEXT, k2 TEXT, k3 TEXT, v BLOB)')
    for i, ref in enumerate(REFS):
        text = TEXTS.get(ref, f'Note number {i} on storage.')
        payload = {'text': text, 'metadata': {}}
        value = {'node_id': ref, 'node_kind': 'decision', 'summary': text, 'status': 'ACTIVE',
                 'properties': {'memory_about': ABOUT, 'memory_entity_kind': 'memory_entry',
                                'entry_kind': 'decision', 'memory_payload_json': json.dumps(payload)}}
        connection.execute('INSERT INTO nodes VALUES (?, ?)', (ref, json.dumps(value)))
    connection.commit()
    connection.close()


FAKE_BINARY = '''#!/usr/bin/env python3
import json, sys
if sys.argv[1:] == ['--version']:
    print('kmp-mcp 0.0.0-fake (store format 4 (sqlite))')
elif sys.argv[1] == 'export':
    open(sys.argv[2], 'w').write('{"bundle_format":3}\\n')
    print(json.dumps({"content_digest": "sha256:" + "ab" * 32, "event_count": 40, "abouts": ["project:demo"]}))
else:
    sys.exit(3)
'''


def fake_binary(root):
    path = Path(root) / 'fake-kmp-mcp'
    path.write_text(FAKE_BINARY)
    path.chmod(path.stat().st_mode | stat.S_IEXEC)
    return path


class AnchorsAndTemplates(unittest.TestCase):
    def test_anchor_forms_in_words(self):
        cases = {'What is the state of issue #188 pause resume?': ('#188',),
                 'Was GitHub issue 185 fully implemented?': ('#185',),
                 'patterns from issues 190 and 191 must issue 192 preserve': ('#190', '#191', '#192'),
                 'GitHub issues 185 and 187 through 189': ('#185', '#187', '#188', '#189'),
                 'context for C6.8+C6.9 artifacts': ('C6.8', 'C6.9'),
                 'pruebas para C6/C6.4 y el adaptador': ('C6.4',),
                 'review of the KMP v0.18.6 submission': ('v0.18.6',),
                 'Why did the Lambda have a timeout of 300 seconds?': ()}
        for text, expected in cases.items():
            self.assertEqual(RP.extract_anchors(text), expected, text)

    def test_facet_template_from_the_enumeration(self):
        template = RP.facet_template('What decisions, implementation status, known gaps, and validation '
                                     'evidence are stored for issue 203?')
        self.assertEqual([f['words'] for f in template],
                         ['decisions', 'implementation status', 'known gaps', 'validation evidence'])
        self.assertEqual(template[1]['name'], 'implementation-status')
        spanish = RP.facet_template('¿Qué decisiones, contratos, límites y trabajo pendiente están almacenados?')
        self.assertEqual([f['name'] for f in spanish], ['decisiones', 'contratos', 'limites', 'trabajo-pendiente'])
        trailing = RP.facet_template('What durable context exists for C6.8 artifacts backup, retention, GC, '
                                     'and leases?')
        self.assertEqual([f['words'] for f in trailing], ['C6.8 artifacts backup', 'retention', 'GC', 'leases'])
        single = RP.facet_template('Why did the Lambda have a timeout of 300 seconds?')
        self.assertEqual(single, ({'name': 'answer', 'words': 'Why did the Lambda have a timeout of 300 seconds?'},))


class Intake(unittest.TestCase):
    def calls(self):
        base = {'about': ABOUT, 'question': 'What is the state of #188?', 'context_id': 'c1', 'purpose': 'answer'}
        return [RP.TranscriptCall(dict(base), '2026-09-16T10:00:00Z', 'codex', 'a' * 64),
                RP.TranscriptCall(dict(base, context_id='c2'), '2026-09-16T09:00:00Z', 'claude', 'b' * 64),
                RP.TranscriptCall({**base, 'continuation': 'x'}, '2026-09-16T11:00:00Z', 'codex', 'a' * 64),
                RP.TranscriptCall({'about': ABOUT, 'question': 'old'}, '2026-09-01T00:00:00Z', 'codex', 'a' * 64),
                RP.TranscriptCall({'question': 'Q without about'}, '2026-09-17T00:00:00Z', 'codex', 'a' * 64),
                RP.TranscriptCall({'about': ABOUT, 'question': 'Q without about'}, '2026-09-17T00:01:00Z',
                                  'codex', 'a' * 64),
                RP.TranscriptCall({'about': ABOUT, 'question': 'late'}, '2026-09-30T00:00:00Z', 'codex', 'a' * 64)]

    def test_window_paging_host_keys_and_first_occurrence(self):
        records = RP.collect(self.calls(), validation_from='2026-09-20T00:00:00Z')
        questions = [r.question for r in records]
        self.assertEqual(questions, ['What is the state of #188?', 'Q without about', 'Q without about', 'late'])
        first = records[0]
        self.assertEqual((first.asked_at, first.host), ('2026-09-16T09:00:00Z', 'claude'))
        self.assertNotIn('context_id', first.call)
        refused, retried = records[1], records[2]
        self.assertEqual((refused.status, refused.duplicate_of), ('not_labelable', retried.id))
        self.assertEqual([r.split for r in records], ['development'] * 3 + ['validation'])

    def test_ids_are_content_hashes_and_merge_keeps_existing(self):
        records = RP.collect(self.calls())
        again = RP.collect(list(reversed(self.calls())))
        self.assertEqual([r.id for r in records], [r.id for r in again])
        self.assertEqual(RP.merge(records[:2], records), records)

    def test_round_trip_and_refusals(self):
        record = intake()
        self.assertEqual(RP.Intake.from_dict(record.as_dict()), record)
        broken = dict(record.as_dict(), status='not_labelable', status_reason=None)
        with self.assertRaises(RP.IntakeInvalid):
            RP.Intake.from_dict(broken)
        with self.assertRaises(RP.IntakeInvalid):
            RP.Intake.from_dict(dict(record.as_dict(), call={'question': 'q', 'context_id': 'x'}))

    def test_transcript_readers(self):
        with tempfile.TemporaryDirectory() as home:
            claude = Path(home) / '.claude' / 'projects' / 'p' / 's.jsonl'
            codex = Path(home) / '.codex' / 'sessions' / '2026' / 'r.jsonl'
            claude.parent.mkdir(parents=True)
            codex.parent.mkdir(parents=True)
            part = {'type': 'tool_use', 'name': RP.CLAUDE_TOOL, 'input': {'about': ABOUT, 'question': 'A?'}}
            claude.write_text(json.dumps({'timestamp': '2026-09-16T00:00:00Z',
                                          'message': {'content': [part]}}) + '\nnot json\n')
            item = {'type': 'McpToolCall', 'server': 'kmp', 'tool': 'kmp_ask',
                    'arguments': {'about': ABOUT, 'question': 'B?'}}
            codex.write_text(json.dumps({'timestamp': '2026-09-16T00:00:01Z', 'payload': {'item': item}},
                                        separators=(',', ':')) + '\n')
            paths = RP.transcript_paths(home)
            calls = list(RP.read_claude(paths[0])) + list(RP.read_codex(paths[1]))
        self.assertEqual([(c.host, c.arguments['question']) for c in calls], [('claude', 'A?'), ('codex', 'B?')])


class Pools(unittest.TestCase):
    def test_sources_counts_and_determinism(self):
        docs = about_docs()
        kernel = (REFS[5], REFS[0], REFS[6])
        pool = PL.build_pool('breal-000000000001', docs, ('#188',), kernel, RULES, 'f' * 64)
        again = PL.build_pool('breal-000000000001', docs, ('#188',), kernel, RULES, 'f' * 64)
        self.assertEqual(pool, again)
        self.assertEqual(pool.counts, {'anchor': 2, 'kernel': 3, 'random': 4, 'total': 8})
        self.assertEqual(pool.sources[REFS[0]], ('anchor', 'kernel'))
        self.assertEqual(set(PL.anchor_entries(docs, ('#188',))), {REFS[0], REFS[2]})
        self.assertNotEqual(list(pool.refs), sorted(pool.refs))  # shuffled, not ranked
        other = PL.build_pool('breal-000000000002', docs, ('#188',), kernel, RULES, 'f' * 64)
        self.assertNotEqual(pool.refs, other.refs)

    def test_the_labeler_record_hides_order_and_sources(self):
        pool = PL.build_pool('breal-000000000001', about_docs(), ('C6.4',), (REFS[9], REFS[1]), RULES, 'f' * 64)
        record = pool.as_dict()
        self.assertEqual(set(record), {'schema', 'question_id', 'about', 'rules_sha256', 'refs', 'counts', 'digest'})
        self.assertEqual(PL.parse_pool(record).refs, pool.refs)
        with self.assertRaises(PL.PoolFailed):
            PL.parse_pool(dict(record, refs=list(reversed(record['refs']))))
        self.assertIn('do not open', pool.provenance()['note'])

    def test_kernel_top_follows_continuations_and_maps_evidence_to_entries(self):
        pages = [{'proof': {'evidence': [{'id': 'entry:' + REFS[3]}, {'id': 'detail:evidence:x',
                                                                      'supports': [REFS[4]]}]},
                  'projection': {'next_action': {'tool': 'kmp_ask', 'arguments': {'continuation': 'c'}}}},
                 {'proof': {'evidence': [{'id': 'entry:' + REFS[3]}, {'id': 'entry:' + REFS[7]}]},
                  'projection': {}}]
        seen = []

        def call(tool, arguments):
            seen.append((tool, arguments))
            return pages[len(seen) - 1]
        refs = PL.kernel_top(call, {'about': ABOUT, 'question': 'Q?', 'budget': {'x': 1}}, RULES, set(REFS))
        self.assertEqual(refs, (REFS[3], REFS[4], REFS[7]))
        self.assertEqual(seen[0][1]['answer_policy'], 'best_effort')
        self.assertEqual(seen[0][1]['budget'], {'max_entries': 5})
        self.assertEqual(seen[1][1], {'continuation': 'c'})
        with self.assertRaises(PL.PoolFailed):
            PL.kernel_top(lambda tool, arguments: {'error': {'code': 'x'}}, {'about': ABOUT, 'question': 'Q'},
                          RULES, set(REFS))


class LabelSchema(unittest.TestCase):
    def record(self, **gold_changes):
        return LB.label_record('breal-000000000001', 'ana', '2026-09-26T10:00:00Z', 'a' * 64, 'b' * 64,
                               dict(gold(), **gold_changes))

    def test_schema_validates_the_section_4_1_gold(self):
        schema = load_schema()
        validate(self.record(), schema)
        gold_schema = schema['$defs']['gold']
        self.assertTrue({'answerable', 'shape', 'facets', 'wrong_subject'} <= set(gold_schema['required']))
        self.assertEqual(set(schema['$defs']['facet']['required']),
                         {'name', 'words', 'answers', 'related', 'answerable_facet'})
        self.assertEqual(tuple(gold_schema['properties']['answerable']['enum']), ANSWERABLE)
        self.assertEqual(tuple(gold_schema['properties']['shape']['enum']), SHAPES)
        self.assertEqual(tuple(gold_schema['properties']['unknown_reason']['enum'][1:]), UNKNOWN_REASONS)
        Gold.from_dict(self.record()['gold'])  # the label's gold is exactly the question gold object

    def test_schema_refusals(self):
        schema = load_schema()
        bad = [self.record(shape=None), self.record(chain={'refs': []}),
               self.record(wrong_subject=['entry:' + REFS[1]]),
               self.record(facets=[]), dict(self.record(), extra=1), dict(self.record(), labeler='Ana Z')]
        for record in bad:
            with self.assertRaises(SchemaViolation):
                validate(record, schema)

    def test_checker_refuses_keywords_it_does_not_implement(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 's.json'
            path.write_text(json.dumps({'type': 'object', 'maxItems': 3}))
            with self.assertRaises(SchemaViolation):
                load_schema(path)

    def test_check_label_against_the_freeze(self):
        item = intake()
        record = LB.label_record(item.id, 'ana', '2026-09-26T10:00:00Z', 'a' * 64, 'b' * 64, gold())
        label = LB.check_label(record, item, set(REFS), 'a' * 64, 'b' * 64)
        self.assertEqual(label.gold.answerable, 'KNOWN')
        for record, message in (
                (dict(record, pool_digest='c' * 64), 'another pool'),
                (dict(record, rules_sha256='c' * 64), 'other labeling rules'),
                (dict(record, gold=gold(wrong=['project:other:entry:x'])), 'not in about'),
                (dict(record, gold=gold(shape='singular')), 'singular')):
            with self.assertRaisesRegex(LB.LabelInvalid, message):
                LB.check_label(record, item, set(REFS), 'a' * 64, 'b' * 64)


class Kappa(unittest.TestCase):
    def test_cohen_kappa_values(self):
        self.assertEqual(AG.cohen_kappa([('a', 'a'), ('b', 'b')]), 1.0)
        # Textbook: 20 yes/yes, 5 yes/no, 10 no/yes, 15 no/no -> po 0.7, pe 0.5, kappa 0.4.
        pairs = [(1, 1)] * 20 + [(1, 0)] * 5 + [(0, 1)] * 10 + [(0, 0)] * 15
        self.assertAlmostEqual(AG.cohen_kappa(pairs), 0.4)
        self.assertEqual(AG.cohen_kappa([('none', 'none')] * 4), 1.0)
        self.assertIsNone(AG.cohen_kappa([]))

    def test_units_align_facets_by_name_and_count_missing_facets(self):
        a = Gold.from_dict(gold())
        b = Gold.from_dict(gold(facets=[{'name': 'state', 'words': 'state', 'answers': [REFS[0]],
                                         'related': [], 'answerable_facet': True}], shape='singular'))
        units = AG.question_units(a, b, REFS[:4])
        self.assertEqual(len(units['facet_entry']), 2 * 4)
        self.assertIn(('related', 'none'), units['facet_entry'])
        self.assertIn((False, 'absent'), units['answerable_facet'])
        self.assertEqual(units['shape'], [('enumerative', 'singular')])

    def test_report_is_aggregates_only_and_gates_on_n(self):
        a = Gold.from_dict(gold())
        b = Gold.from_dict(gold(wrong=[REFS[5]]))
        paired = [(f'breal-{i:012x}', a, a if i % 5 else b, REFS[:10]) for i in range(30)]
        report, disagreeing = AG.agreement_report(paired, ('ana', 'ben'))
        self.assertEqual(report['gate']['status'], 'pass')
        self.assertEqual(len(disagreeing), 6)
        text = json.dumps(report)
        self.assertFalse(any(ref in text for ref in REFS) or 'breal-' in text or 'implementation' in text)
        short, _ = AG.agreement_report(paired[:29], ('ana', 'ben'))
        self.assertEqual(short['gate']['status'], 'insufficient_n')
        c = Gold.from_dict(gold(facets=[{'name': 'state', 'words': 'x', 'answers': [REFS[9]], 'related': [],
                                         'answerable_facet': True}, {'name': 'work', 'words': 'y',
                                         'answers': [REFS[8]], 'related': [], 'answerable_facet': True}]))
        failing, _ = AG.agreement_report([(q, a, c, r) for q, a, _, r in paired], ('ana', 'ben'))
        self.assertEqual(failing['gate']['status'], 'fail')


class Resolution(unittest.TestCase):
    def labels(self, *golds):
        return [LB.Label('q', f'l{i}', Gold.from_dict(g), {}) for i, g in enumerate(golds)]

    def test_resolution_order(self):
        one, two = gold(), gold(wrong=[REFS[5]])
        self.assertEqual(LB.resolve_one(self.labels(one, one)).how, 'agreed')
        self.assertEqual(LB.resolve_one(self.labels(one)).how, 'single')
        self.assertEqual(LB.resolve_one(self.labels(one, two)).how, 'unresolved')
        self.assertIsNone(LB.resolve_one(self.labels(one, two)).gold)
        adjudicated = self.labels(two)[0]
        self.assertEqual(LB.resolve_one(self.labels(one, two), adjudicated).gold, adjudicated.gold)
        self.assertEqual(LB.resolve_one([]).how, 'unlabeled')

    def test_to_question_is_a_private_b_real_question(self):
        question = LB.to_question(intake(), Gold.from_dict(gold()), 'b' * 64, 'agreed')
        self.assertEqual((question.corpus, question.type, question.private), ('b-real', 'enumerative_anchored', True))
        self.assertEqual(question.answer_policy, 'evidence_or_unknown')
        self.assertIn('split:development', question.tags)
        free = RP.make_intake({'about': ABOUT, 'question': 'Why a 300 s timeout?', 'answer_policy': 'best_effort'},
                              '2026-09-16T00:00:00Z', 'codex')
        single = gold(shape='singular', facets=[{'name': 'answer', 'words': 'why', 'answers': [REFS[3]],
                                                 'related': [], 'answerable_facet': True}])
        question = LB.to_question(free, Gold.from_dict(single), 'b' * 64, 'single')
        self.assertEqual((question.type, question.answer_policy), ('lookup_exact', 'best_effort'))


class Session(unittest.TestCase):
    def test_scripted_session_derives_answerability_and_needs_a_shape(self):
        item = intake()
        pool = PL.build_pool(item.id, about_docs(), item.anchors, (REFS[5],), RULES, 'f' * 64)
        out, saved = [], []
        session = LabelingSession(item, pool, about_docs(), out=out.append)
        number = str(pool.refs.index(REFS[0]) + 1)
        facets = [f['name'] for f in item.facet_template]
        script = ['save', 'search "preflight checks"', f'answers {facets[0]} f1', f'related {facets[0]} {number}',
                  f'wrong {REFS[3]}', 'shape enumerative', 'note checked twice', 'save', 'quit']
        session.run(script, lambda g, n, s: saved.append((g, n, s)) or 'ok')
        self.assertTrue(any('set the shape first' in line for line in out))
        gold_value, notes, searches = saved[-1]
        self.assertEqual(gold_value['answerable'], 'KNOWN')
        self.assertEqual(gold_value['facets'][0]['answers'], [REFS[2]])
        self.assertTrue(gold_value['facets'][0]['answerable_facet'])
        self.assertFalse(gold_value['facets'][1]['answerable_facet'])
        self.assertEqual((notes, searches), ('checked twice', ['"preflight checks"']))
        self.assertFalse(any('kernel' in line or 'anchor:' in line for line in out))

    def test_free_search_folds_case_and_accents(self):
        docs = about_docs().entries
        self.assertEqual(search(docs, 'PREFLIGHT host'), [REFS[2]])
        self.assertEqual(search(docs, '"local execution"'), [REFS[1]])
        self.assertEqual(search(docs, '   '), [])


class Privacy(unittest.TestCase):
    """No B-real writer accepts a destination inside the repository (tmp/ included)."""

    def test_every_writer_refuses_the_repository(self):
        inside = REPO_ROOT / 'tmp' / 'memory-bench' / 'breal-test' / '2026-09-26'
        with self.assertRaises(PrivacyViolation):
            RF.FreezeDir.at(inside)
        with self.assertRaises(PrivacyViolation):
            RF.register(REPO_ROOT / 'tmp' / 'memory-bench', '0' * 64, 'x')
        question = LB.to_question(intake(), Gold.from_dict(gold()), 'b' * 64, 'agreed')
        with self.assertRaises(PrivacyViolation):
            write_questions(REPO_ROOT / 'tmp' / 'memory-bench' / 'gold.jsonl', [question])
        with tempfile.TemporaryDirectory() as root:
            fd = RF.FreezeDir.at(Path(root) / '2026-09-26')
            for escape in (Path(root) / 'outside.json', REPO_ROOT / 'x.json'):
                with self.assertRaises((RF.FreezeInvalid, PrivacyViolation)):
                    fd.write_json(escape, {})
            with self.assertRaises(RF.FreezeInvalid):
                fd.path('..', 'escape.json')
            os.symlink(REPO_ROOT, Path(root) / '2026-09-26-link')
            with self.assertRaises(PrivacyViolation):
                RF.FreezeDir.at(Path(root) / '2026-09-26-link' / 'tmp')
        self.assertFalse(inside.exists())

    def test_cli_refuses_a_freeze_inside_the_repository(self):
        inside = str(REPO_ROOT / 'tmp' / 'memory-bench' / 'breal-test')
        for argv in (['status', '--freeze', inside], ['import', '--freeze', inside],
                     ['rules', '--freeze', inside], ['resolve', '--freeze', inside]):
            with contextlib.redirect_stderr(io.StringIO()) as err:
                self.assertEqual(CLI.main(argv), 2, argv)
            self.assertIn('inside the repository', err.getvalue())
        with contextlib.redirect_stderr(io.StringIO()) as err:  # through `memory_bench real`
            self.assertEqual(BENCH_CLI.main(['real', 'status', '--freeze', inside]), 2)
        self.assertIn('inside the repository', err.getvalue())
        self.assertFalse(Path(inside).exists())


class FakeKernel:
    def __init__(self, fd, out_dir):
        self.calls = 0

    def __call__(self, tool, arguments):
        self.calls += 1
        return {'proof': {'evidence': [{'id': 'entry:' + REFS[7]}, {'id': 'entry:' + REFS[0]}]}, 'projection': {}}

    def close(self):
        pass


class EndToEnd(unittest.TestCase):
    """freeze -> import -> rules -> pool -> label x2 -> agreement -> resolve, all in a temp private root."""

    def run_cli(self, *argv):
        with contextlib.redirect_stdout(io.StringIO()) as out:
            code = CLI.main(list(argv))
        return code, out.getvalue()

    def test_pipeline(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            freeze = root / 'private' / '2026-09-26'
            sqlite_store(freeze / 'store')
            binary = fake_binary(root)
            home = root / 'home'
            transcript = home / '.codex' / 'sessions' / 'r.jsonl'
            transcript.parent.mkdir(parents=True)
            item = {'type': 'McpToolCall', 'server': 'kmp', 'tool': 'kmp_ask',
                    'arguments': {'about': ABOUT, 'question': 'What implementation state and outstanding '
                                                              'work exist for issue #188?'}}
            transcript.write_text(json.dumps({'timestamp': '2026-09-19T00:00:00Z', 'payload': {'item': item}},
                                             separators=(',', ':')) + '\n')
            F = str(freeze)
            self.assertEqual(self.run_cli('freeze', '--freeze', F, '--binary', str(binary))[0], 0)
            manifest = json.loads((freeze / 'freeze.json').read_text())
            self.assertEqual(manifest['store']['integrity_check'], 'ok')
            self.assertEqual(manifest['bundle']['content_digest'], 'sha256:' + 'ab' * 32)
            self.assertEqual(self.run_cli('import', '--freeze', F, '--home', str(home))[0], 0)
            with self.assertRaises(RF.FreezeInvalid):
                CLI.load_rules(RF.FreezeDir.at(freeze))
            self.assertEqual(self.run_cli('rules', '--freeze', F)[0], 0)
            self.assertEqual(self.run_cli('rules', '--freeze', F)[0], 0)  # idempotent, registered once
            self.assertEqual(len(RF.registrations(freeze.parent)), 1)
            with mock.patch.object(CLI, 'KernelCaller', FakeKernel):
                self.assertEqual(self.run_cli('pool', '--freeze', F)[0], 0)
            fd = RF.FreezeDir.at(freeze)
            record = RP.load_intake(fd.questions)[0]
            pool = json.loads(fd.pool(record.id).read_text())
            self.assertIn(REFS[0], pool['refs'])
            names = [f['name'] for f in record.facet_template]
            script = root / 'label.txt'
            for labeler in ('ana', 'ben'):
                number = pool['refs'].index(REFS[0]) + 1
                script.write_text(f'answers {names[0]} {number}\nshape enumerative\nsave\nquit\n')
                code, _ = self.run_cli('label', '--freeze', F, '--labeler', labeler, '--question', record.id,
                                       '--script', str(script))
                self.assertEqual(code, 0)
            self.assertEqual(self.run_cli('validate', '--freeze', F)[0], 0)
            code, out = self.run_cli('agreement', '--freeze', F, '--labelers', 'ana,ben')
            self.assertEqual(code, 1)  # one question: insufficient_n, never pass
            self.assertEqual(json.loads(out)['gate']['status'], 'insufficient_n')
            self.assertEqual(json.loads(out)['kappa']['facet_entry'], 1.0)
            self.assertEqual(self.run_cli('resolve', '--freeze', F)[0], 0)
            gold_line = json.loads(fd.gold.read_text().splitlines()[0])
            self.assertEqual((gold_line['corpus'], gold_line['private']), ('b-real', True))
            self.assertIn('gold:agreed', gold_line['tags'])
            (freeze / 'store' / 'store' / 'kernel.sqlite3').write_bytes(b'changed')
            with self.assertRaises(RF.FreezeInvalid):
                RF.verify(fd)

    def test_freeze_refuses_a_non_data_directory(self):
        with tempfile.TemporaryDirectory() as root:
            fd = RF.FreezeDir.at(Path(root) / '2026-09-26')
            (fd.root / 'store').mkdir(parents=True)
            with self.assertRaises(RF.FreezeInvalid):
                RF.freeze(fd, fake_binary(root), Path(root) / 'work')

    def test_read_store_sees_the_fixture(self):
        with tempfile.TemporaryDirectory() as root:
            sqlite_store(Path(root) / 'data')
            self.assertEqual(len(read_store(Path(root) / 'data').about(ABOUT).entries), len(REFS))
            self.assertEqual(RF.integrity(Path(root) / 'data' / 'store' / 'kernel.sqlite3'), 'ok')


if __name__ == '__main__':
    unittest.main()
