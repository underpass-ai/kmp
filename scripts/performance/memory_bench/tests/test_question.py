"""kmp.bench.question.v1: validation, round trip, first call, digests and privacy."""
import copy
from pathlib import Path
import tempfile
import unittest

from ..domain import question as q
from ..domain.errors import PrivacyViolation, QuestionInvalid

A = 'synth:mono-a000'


def ref(n):
    return f'{A}:e{n:07d}'


def facet(name, answers=(), related=(), answerable=None):
    return {'name': name, 'answers': list(answers), 'related': list(related),
            'answerable_facet': bool(answers) if answerable is None else answerable}


def ask(**overrides):
    record = {'schema': q.SCHEMA, 'id': 'q-1', 'corpus': 'synth-v1', 'type': 'lookup_exact',
              'about': A, 'tool': 'kmp_ask', 'question': 'Which valve failed?',
              'answer_policy': 'evidence_or_unknown',
              'gold': {'answerable': 'KNOWN', 'shape': 'singular', 'facets': [facet('valve', [ref(2)])]}}
    record.update(overrides)
    return record


def gold(**overrides):
    value = {'answerable': 'KNOWN', 'shape': 'singular', 'facets': [facet('valve', [ref(2)])]}
    value.update(overrides)
    return value


def negative(kind='anchor_absent', **gold_overrides):
    fields = dict(answerable='UNKNOWN', facets=[facet('valve', related=[ref(3)])],
                  unknown_reason='anchor_not_found', absent_terms=['#288'])
    return ask(id='hn-1', corpus='hard-negatives', private=True, type=kind, anchors=['#288'],
               gold=gold(**{**fields, **gold_overrides}))


class ValidTest(unittest.TestCase):
    def test_minimal_ask_round_trips_to_the_canonical_form(self):
        parsed = q.Question.from_dict(ask())
        again = q.Question.from_dict(parsed.as_dict())
        self.assertEqual(parsed, again)
        self.assertEqual(parsed.as_dict(), again.as_dict())
        self.assertEqual(parsed.digest(), again.digest())
        self.assertEqual(parsed.arguments, {})
        self.assertIsNone(parsed.as_dict()['as_of'])

    def test_set_like_lists_are_sorted_and_ordered_lists_are_kept(self):
        record = ask(tags=['z', 'a'], type='multihop_why_k',
                     gold=gold(facets=[facet('why', [ref(9), ref(1)], [ref(5), ref(4)])],
                               chain={'refs': [ref(9), ref(1), ref(5)]}))
        parsed = q.Question.from_dict(record)
        self.assertEqual(parsed.tags, ('a', 'z'))
        self.assertEqual(parsed.gold.facets[0].answers, (ref(1), ref(9)))
        self.assertEqual(parsed.gold.chain.refs, (ref(9), ref(1), ref(5)))

    def test_every_negative_type_is_accepted_with_its_gold(self):
        for kind in q.NEGATIVE_TYPES:
            with self.subTest(kind=kind):
                q.Question.from_dict(negative(kind))

    def test_digest_ignores_absent_versus_null(self):
        explicit = ask(asked_as=None, as_of=None, arguments={}, anchors=[], tags=[], source=None)
        self.assertEqual(q.Question.from_dict(ask()).digest(), q.Question.from_dict(explicit).digest())

    def test_mutating_a_serialized_copy_does_not_touch_the_question(self):
        parsed = q.Question.from_dict(ask(arguments={'budget': {'max_entries': 10}}))
        parsed.as_dict()['arguments']['budget']['max_entries'] = 99
        self.assertEqual(parsed.arguments, {'budget': {'max_entries': 10}})


class InvalidTest(unittest.TestCase):
    def refuse(self, record, fragment):
        with self.assertRaises(QuestionInvalid) as caught:
            q.Question.from_dict(record)
        self.assertIn(fragment, str(caught.exception))

    def test_structural_refusals(self):
        cases = [
            (dict(ask(), extra=1), 'unknown field'),
            (ask(schema='kmp.bench.question.v0'), 'schema'),
            (ask(id='has space'), 'id'),
            (ask(corpus='elsewhere'), 'corpus'),
            (ask(type='vibes'), 'not a question type'),
            (ask(tool='kmp_inspect'), 'tool'),
            (ask(answer_policy=None), 'answer_policy'),
            (ask(question=None), 'kmp_ask needs question'),
            (ask(about='two words'), 'about'),
            (ask(as_of={'time': 'x', 'ref': 'y'}), 'exactly one'),
            (ask(as_of={'time': 'x'}, interval={'start': 'y'}), 'exclusive'),
            (ask(axis='occurred'), 'applies only with'),
            (ask(interval={'middle': 'x'}), 'takes start'),
            (ask(arguments={'question': 'again'}), 'dedicated fields'),
            (ask(arguments={'continuation': 'read_0'}), 'dedicated fields'),
            (ask(private='yes'), 'boolean'),
            (ask(min_level=0), 'min_level'),
            (ask(corpus='retrieval-judged', min_level=1000), 'ladder'),
            (ask(source={'kind': {'nested': 1}}), 'scalar'),
            (ask(corpus='b-real'), 'private by definition'),
            (ask(tags=['Upper']), 'tags'),
        ]
        for record, fragment in cases:
            with self.subTest(fragment=fragment):
                self.refuse(record, fragment)

    def test_gold_refusals(self):
        cases = [
            (gold(facets=[facet('a', ['entry:' + ref(1)])]), 'canonical'),
            (gold(facets=[facet('a', [ref(1), ref(1)])]), 'duplicate'),
            (gold(facets=[facet('a', [ref(1)], answerable=False)]), 'answerable'),
            (gold(facets=[facet('a', [ref(1)], [ref(1)])]), 'both answer'),
            (gold(shape='singular', facets=[facet('a', [ref(1)]), facet('b')]), 'exactly one'),
            (gold(shape='enumerative'), 'at least two'),
            (gold(facets=[facet('a', [ref(1)]), facet('a')], shape='enumerative'), 'duplicate facet'),
            (gold(wrong_subject=[ref(2)]), 'overlaps'),
            (gold(answerable='UNKNOWN'), 'no answering facet'),
            (gold(unknown_reason='anchor_not_found'), 'only an UNKNOWN'),
            (gold(unknown_reason='vibes', answerable='UNKNOWN', facets=[facet('a')]), 'unknown_reason'),
            (gold(answerable='maybe'), 'answerable'),
            (gold(facets=[{'name': 'a', 'answers': [ref(1)]}]), 'answerable_facet'),
            (gold(shape=None), 'shape'),
            (gold(facets=[facet('a', answerable=True)]), 'at least one answering entry'),
        ]
        for value, fragment in cases:
            with self.subTest(fragment=fragment):
                self.refuse(ask(gold=value), fragment)

    def test_type_rules(self):
        cases = [
            (ask(type='enumerative_anchored', anchors=['C6.4']), 'enumerative'),
            (ask(type='singular_anchored'), 'anchors'),
            (negative(absent_terms=[]), 'absent_terms'),
            (negative(unknown_reason=None), 'unknown_reason'),
            (ask(type='negated_anchor', anchors=['Y']), 'excluded refs'),
            (ask(type='multihop_why_k'), 'chain'),
            (ask(type='current_after_supersession', gold=gold(current_ref=ref(2))), 'stale_refs'),
            (ask(type='current_after_supersession', gold=gold(current_ref=ref(7), stale_refs=[ref(3)])), 'current_ref'),
            (ask(type='as_of_historical'), 'as_of'),
            (ask(type='interval_scoped'), 'interval'),
            (ask(type='wake_resume'), 'kmp_wake'),
            (ask(gold=gold(answerable='UNKNOWN', facets=[facet('a')], nearest_outside=ref(1))), 'interval'),
        ]
        for record, fragment in cases:
            with self.subTest(fragment=fragment):
                self.refuse(record, fragment)


class ToolTest(unittest.TestCase):
    def path_question(self, **overrides):
        record = {'schema': q.SCHEMA, 'id': 'p-1', 'corpus': 'jev-judged', 'type': 'path_between',
                  'about': 'service:billing', 'tool': 'kmp_trace',
                  'arguments': {'from': 'service:billing:b02', 'to': 'service:billing:b03'},
                  'gold': {'answerable': 'KNOWN',
                           'path': {'from': 'service:billing:b02', 'to': 'service:billing:b03',
                                    'steps': ['service:billing:b02', 'service:billing:b01',
                                              'service:billing:b03']}}}
        record.update(overrides)
        return record

    def test_trace_path_question(self):
        parsed = q.Question.from_dict(self.path_question())
        tool, arguments = parsed.first_call()
        self.assertEqual(tool, 'kmp_trace')
        self.assertEqual(arguments, {'about': 'service:billing', 'from': 'service:billing:b02',
                                     'to': 'service:billing:b03'})

    def test_path_rules(self):
        base = self.path_question()
        bad_steps = copy.deepcopy(base)
        bad_steps['gold']['path']['steps'] = ['service:billing:b01', 'service:billing:b03']
        open_with_to = dict(base, type='path_open')
        wrong_from = dict(base, arguments={'from': 'service:billing:b99', 'to': 'service:billing:b03'})
        no_route = copy.deepcopy(base)
        no_route['gold']['path']['steps'] = []
        cases = [(bad_steps, 'reference route'), (open_with_to, 'path_open has none'),
                 (wrong_from, 'from differs'), (no_route, 'give the reference route'),
                 (dict(base, arguments={'from': 'service:billing:b02'}), 'to or search'),
                 (dict(base, answer_policy='best_effort'), 'takes no answer_policy'),
                 (dict(base, dimensions={'scope': 'abouts'}), 'takes no dimensions'),
                 (dict(base, tool='kmp_ask', question='x', answer_policy='best_effort'), 'asked through')]
        for record, fragment in cases:
            with self.subTest(fragment=fragment), self.assertRaises(QuestionInvalid) as caught:
                q.Question.from_dict(record)
            self.assertIn(fragment, str(caught.exception))

    def test_curate_needs_mode_and_takes_no_as_of(self):
        base = self.path_question(tool='kmp_curate')
        with self.assertRaises(QuestionInvalid):
            q.Question.from_dict(base)
        record = dict(base, arguments=dict(base['arguments'], mode='paths'))
        q.Question.from_dict(record)
        with self.assertRaises(QuestionInvalid):
            q.Question.from_dict(dict(record, as_of={'time': '2026-01-01T00:00:00Z'}))

    def test_wake_question_sends_intent_from_arguments_only(self):
        record = {'schema': q.SCHEMA, 'id': 'w-1', 'corpus': 'token-harness', 'type': 'wake_resume',
                  'about': 'fixture:w01', 'tool': 'kmp_wake', 'question': 'Resume the ledger work.',
                  'arguments': {'intent': 'Finish the ledger cutover.'},
                  'as_of': {'time': '2026-09-01T10:03:00Z'},
                  'gold': {'answerable': 'KNOWN', 'wake_required': ['fixture:w01:a']}}
        tool, arguments = q.Question.from_dict(record).first_call(max_bytes=4096)
        self.assertEqual(tool, 'kmp_wake')
        self.assertEqual(arguments, {'about': 'fixture:w01', 'intent': 'Finish the ledger cutover.',
                                     'as_of': {'time': '2026-09-01T10:03:00Z'},
                                     'budget': {'max_bytes': 4096}})

    def test_ask_first_call_budget_rules(self):
        pinned = q.Question.from_dict(ask(asked_as='¿Qué válvula falló?', dimensions={'scope': 'current_about'},
                                          arguments={'depth': 3, 'budget': {'tokens': 2048, 'max_bytes': 512}}))
        tool, arguments = pinned.first_call(max_bytes=10000, default_budget={'max_entries': 5})
        self.assertEqual(arguments['budget'], {'tokens': 2048, 'max_bytes': 10000})
        self.assertEqual(arguments['asked_as'], '¿Qué válvula falló?')
        self.assertEqual(arguments['dimensions'], {'scope': 'current_about'})
        self.assertEqual(pinned.arguments['budget']['max_bytes'], 512, 'first_call must not mutate')
        free = q.Question.from_dict(ask())
        _, arguments = free.first_call(default_budget={'max_entries': 5})
        self.assertEqual(arguments, {'about': A, 'question': 'Which valve failed?',
                                     'answer_policy': 'evidence_or_unknown', 'budget': {'max_entries': 5}})


class FileTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.folder = Path(folder.name)

    def test_jsonl_round_trip_is_byte_stable(self):
        questions = [q.Question.from_dict(ask(id=f'q-{n}')) for n in (2, 1)] + [q.Question.from_dict(negative())]
        path = q.write_questions(self.folder / 'out/questions.jsonl', questions, repo_root=self.folder / 'repo')
        loaded = q.load_questions(path)
        self.assertEqual(loaded, tuple(questions))
        again = q.write_questions(self.folder / 'again.jsonl', loaded, repo_root=self.folder / 'repo')
        self.assertEqual(path.read_bytes(), again.read_bytes())
        with self.assertRaises(FileExistsError):
            q.write_questions(path, questions, repo_root=self.folder / 'repo')

    def test_questions_digest_is_order_free_and_content_sensitive(self):
        a, b = q.Question.from_dict(ask(id='a')), q.Question.from_dict(ask(id='b'))
        changed = q.Question.from_dict(ask(id='b', question='Which pump failed?'))
        self.assertEqual(q.questions_digest([a, b]), q.questions_digest([b, a]))
        self.assertNotEqual(q.questions_digest([a, b]), q.questions_digest([a, changed]))

    def test_parse_reports_line_numbers_and_duplicates(self):
        line = q.jsonl.dump_lines([q.Question.from_dict(ask()).as_dict()])
        with self.assertRaises(QuestionInvalid) as caught:
            q.parse_questions(line + '\n' + line, 'f.jsonl')
        self.assertIn('f.jsonl:3', str(caught.exception))
        for text, fragment in (('{"a":1,"a":2}', 'duplicate JSON key'), ('[1]', 'JSON object'),
                               ('{', 'not JSON')):
            with self.assertRaises(QuestionInvalid) as caught:
                q.parse_questions(text, 'g.jsonl')
            self.assertIn(fragment, str(caught.exception))

    def test_private_questions_never_land_inside_the_repository(self):
        repo = self.folder / 'repo'
        private = [q.Question.from_dict(negative())]
        for inside in (repo / 'tmp/memory-bench/q.jsonl', repo / 'q.jsonl'):
            with self.assertRaises(PrivacyViolation):
                q.write_questions(inside, private, repo_root=repo)
            self.assertFalse(inside.exists())
        public = [q.Question.from_dict(ask())]
        self.assertTrue(q.write_questions(repo / 'tmp/memory-bench/p.jsonl', public, repo_root=repo).exists())
        self.assertTrue(q.write_questions(self.folder / 'private/q.jsonl', private, repo_root=repo).exists())

    def test_the_real_repository_root_is_guarded_by_default(self):
        with self.assertRaises(PrivacyViolation):
            q.write_questions(q.jsonl.REPO_ROOT / 'tmp/memory-bench/never.jsonl',
                              [q.Question.from_dict(negative())])


if __name__ == '__main__':
    unittest.main()
