"""Deterministic boundary tests; these do not establish the model's accuracy."""
import copy
import json
import unittest
from contracts import candidates, episode, verdicts
from form import form
from local_client import LocalClient
from planner import compile_plan

SOURCE = {'about': 'test:formation', 'episode_id': 'conversation-1', 'sources': [
    {'id': 's1', 'role': 'assistant', 'text': 'I suggest PostgreSQL or SQLite.',
     'observed_at': '2026-09-01T10:00:00Z'},
    {'id': 's2', 'role': 'user', 'text': 'We chose SQLite, not PostgreSQL. The pilot costs 0 euros.',
     'observed_at': '2026-09-01T10:01:00Z'}]}
GOOD = {'text': 'The user chose SQLite, not PostgreSQL.', 'category': 'decision', 'citations': [
    {'source_id': 's2', 'quote': 'We chose SQLite, not PostgreSQL.',
     'why': 'The user explicitly names the selected database and rejects PostgreSQL.'}]}
BAD = {'text': 'The user chose PostgreSQL.', 'category': 'decision', 'citations': [
    {'source_id': 's2', 'quote': 'PostgreSQL', 'why': 'PostgreSQL is mentioned.'}]}


class BoundaryTests(unittest.TestCase):
    def test_source_boundary_refuses_query_gold_and_extra_fields(self):
        for name in ('question', 'answer', 'gold_session_ids'):
            with self.subTest(name=name), self.assertRaises(ValueError):
                episode({**SOURCE, name: 'must not reach writer'})
        source = copy.deepcopy(SOURCE)
        source['sources'][0]['has_answer'] = True
        with self.assertRaises(ValueError):
            episode(source)

    def test_duplicates_and_timezone_are_rejected(self):
        source = copy.deepcopy(SOURCE)
        source['sources'].append(source['sources'][0])
        with self.assertRaises(ValueError):
            episode(source)
        source['sources'].pop()
        source['sources'][0]['observed_at'] = '2026-09-01T10:00:00'
        with self.assertRaises(ValueError):
            episode(source)

    def test_altered_quote_and_foreign_source_do_not_become_memories(self):
        for change in ({'source_id': 'other-about:s2'}, {'quote': 'We chose PostgreSQL.'}):
            memory = copy.deepcopy(GOOD)
            memory['citations'][0].update(change)
            admitted, rejected = candidates({'memories': [memory]}, SOURCE)
            self.assertEqual(admitted, [])
            self.assertEqual(len(rejected), 1)

    def test_literal_substring_is_not_automatically_accepted_as_a_fact(self):
        proposed, rejected = candidates({'memories': [GOOD, BAD]}, SOURCE)
        self.assertEqual(len(proposed), 2)  # Both have real quotes; entailment is separate.
        verified = {'verdicts': [{'id': m['id'], 'supported': m['text'] == GOOD['text'],
                                 'reason': 'The whole source rejects PostgreSQL.'} for m in proposed]}
        plan = compile_plan(SOURCE, {'memories': [GOOD, BAD]}, verified, 'test-model@pinned',
                            '2026-09-02T12:00:00Z')
        self.assertEqual(plan['accepted_count'], 1)
        self.assertEqual(len(plan['rejected']), 1)
        self.assertEqual(len(plan['ingest']['memory']['entries']), 3)
        self.assertFalse(rejected)

    def test_incomplete_duplicate_and_nonboolean_verdicts_fail_closed(self):
        proposed, _ = candidates({'memories': [GOOD, BAD]}, SOURCE)
        row = {'id': proposed[0]['id'], 'supported': True, 'reason': 'Explicit statement.'}
        for value in ([row], [row, row], [row, {**row, 'id': proposed[1]['id'], 'supported': 'true'}]):
            with self.subTest(value=value), self.assertRaises(ValueError):
                verdicts({'verdicts': value}, proposed)

    def test_plan_preserves_sources_lineage_and_clocks(self):
        extracted = {'memories': [GOOD]}
        proposed, _ = candidates(extracted, SOURCE)
        verified = {'verdicts': [{'id': proposed[0]['id'], 'supported': True, 'reason': 'Explicit choice.'}]}
        args = SOURCE, extracted, verified, 'test-model@pinned', '2026-09-02T12:00:00Z'
        plan = compile_plan(*args)
        self.assertEqual(plan, compile_plan(*args))
        memory = plan['ingest']['memory']
        self.assertEqual([e['text'] for e in memory['entries'][:2]], [s['text'] for s in SOURCE['sources']])
        fact = memory['entries'][2]
        self.assertEqual(fact['coordinates'][0]['occurred_at'], '2026-09-01T10:01:00Z')
        self.assertEqual(fact['coordinates'][0]['observed_at'], args[-1])
        self.assertEqual(fact['metadata']['source_kind'], 'derived')
        self.assertEqual(memory['relations'][0]['to'], memory['entries'][1]['id'])
        self.assertEqual(memory['relations'][0]['rel'], 'derived_from')
        self.assertEqual(memory['relations'][0]['evidence'], GOOD['citations'][0]['quote'])
        with self.assertRaises(ValueError):
            compile_plan(*args[:-1], '2026-08-31T12:00:00Z')

    def test_two_quotes_from_one_source_keep_one_derivation(self):
        memory = copy.deepcopy(GOOD)
        memory['citations'] = [
            {'source_id': 's2', 'quote': 'We chose SQLite', 'why': 'The user names the selected database.'},
            {'source_id': 's2', 'quote': 'not PostgreSQL.', 'why': 'The user explicitly excludes PostgreSQL.'}]
        extracted = {'memories': [memory]}
        proposed, _ = candidates(extracted, SOURCE)
        plan = compile_plan(SOURCE, extracted, {'verdicts': [{'id': proposed[0]['id'],
            'supported': True, 'reason': 'The two spans preserve the choice and negation.'}]},
            'test-model@pinned', '2026-09-02T12:00:00Z')
        self.assertEqual(len(plan['ingest']['memory']['relations']), 1)
        self.assertEqual(len(plan['ingest']['memory']['evidence']), 2)

    def test_writer_uses_complete_sources_and_saves_verifier_result(self):
        class Model:
            def __init__(self):
                self.messages = []

            def generate(self, messages, schema, phase):
                self.messages.append(messages)
                if phase == 'formation_extract':
                    return {'memories': [GOOD]}
                data = json.loads(messages[1]['content'])
                return {'verdicts': [{'id': data['candidates'][0]['id'], 'supported': True,
                                     'reason': 'The user explicitly chose SQLite.'}]}
        model = Model()
        extracted, verified = form(SOURCE, model)
        self.assertEqual(extracted['memories'], [GOOD])
        self.assertTrue(verified['verdicts'][0]['supported'])
        for messages in model.messages:
            self.assertEqual(json.loads(messages[1]['content'])['sources'], SOURCE['sources'])

    def test_model_call_rejects_truncation_and_input_overflow(self):
        client = LocalClient('http://127.0.0.1:8000/v1', 'test', lambda event: None)
        client.request = lambda endpoint, body, phase: ({'count': 7900} if phase.endswith('_tokenize')
                                                        else self.fail('must not generate'))
        with self.assertRaisesRegex(ValueError, 'budget'):
            client.generate([], {}, 'test')
        client.request = lambda endpoint, body, phase: ({'count': 5} if phase.endswith('_tokenize')
            else {'choices': [{'finish_reason': 'length', 'message': {'content': '{}'}}]})
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            client.generate([], {}, 'test')

    def test_remote_and_ambiguous_endpoints_are_refused(self):
        for url in ('https://127.0.0.1/v1', 'http://example.com/v1', 'http://localhost/v1',
                    'http://127.0.0.1/v1?key=x', 'http://user@127.0.0.1/v1'):
            with self.subTest(url=url), self.assertRaises(ValueError):
                LocalClient(url, 'test', lambda event: None)


if __name__ == '__main__':
    unittest.main()
