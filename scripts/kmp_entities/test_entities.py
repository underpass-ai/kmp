import copy
import unittest

from kmp_entities.boundary import candidates, context, mention
from kmp_entities.planner import compile_plan
from kmp_entities.resolve import resolve


def fixture():
    sources = [
        {'id': 'a', 'role': 'user', 'text': 'Elena Vega uses the nickname Nora.', 'observed_at': '2024-01-01T00:00:00Z'},
        {'id': 'b', 'role': 'user', 'text': 'Nora owns release Amber.', 'observed_at': '2024-01-02T00:00:00Z'}]
    ctx = {'about': 'eval:entity-test', 'context_id': 'history', 'as_of': '2024-01-02T00:00:00Z', 'sources': sources}
    base = {'about': ctx['about'], 'idempotency_key': 'source-fixture',
        'provenance': {'source_kind': 'derived', 'source_agent': 'agent:test', 'observed_at': ctx['as_of']},
        'memory': {'dimensions': [{'id': 'session-' + s['id'], 'kind': 'conversation'} for s in sources],
            'entries': [{'id': ctx['about'] + ':source:' + s['id'], 'kind': 'observation', 'text': s['text'],
                'metadata': {'formation_record': 'original_source', 'source_id': s['id'], 'source_role': s['role']},
                'coordinates': [{'dimension': 'conversation', 'scope_id': 'session-' + s['id'], 'sequence': 1,
                                 'occurred_at': s['observed_at'], 'observed_at': s['observed_at']}]} for s in sources],
            'relations': [], 'evidence': []}}
    extracted = {'proposals': [{'left': {'source_id': 'a', 'text': 'Elena Vega', 'occurrence': 0},
        'right': {'source_id': 'b', 'text': 'Nora', 'occurrence': 0}, 'kind': 'person', 'basis': 'explicit_alias',
        'why': 'The first source declares the alias used in the second source within this bounded fixture.',
        'citations': [{'source_id': s['id'], 'quote': s['text'], 'why': 'Complete literal fixture statement.'} for s in sources]}]}
    proposed, _ = candidates(extracted, ctx)
    verified = {'verdicts': {p['id']: {'same_entity': True, 'reason': 'Synthetic test verdict, not model evidence.'} for p in proposed}}
    return ctx, base, extracted, verified


class EntityContractTests(unittest.TestCase):
    def test_mentions_are_bounded_and_originals_are_unchanged(self):
        ctx, base, extracted, verified = fixture()
        before = copy.deepcopy((ctx, base, extracted, verified))
        plan = compile_plan(ctx, base, extracted, verified, 'test-model', '2024-02-01T00:00:00Z')
        entries = plan['ingest']['memory']['entries']
        self.assertEqual(entries[:2], base['memory']['entries'])
        self.assertEqual({e['text'] for e in entries[2:]}, {'Elena Vega', 'Nora'})
        refs = {e['id'] for e in entries[2:]}
        relation = next(r for r in plan['ingest']['memory']['relations'] if r['rel'] == 'same_entity_as')
        self.assertIn(relation['from'], refs)
        self.assertIn(relation['to'], refs)
        self.assertEqual(relation['coordinate']['occurred_at'], '2024-01-02T00:00:00Z')
        self.assertEqual(before, (ctx, base, extracted, verified))

    def test_cutoff_roles_text_and_scope_are_hard_boundaries(self):
        for fault in ('future', 'query', 'role', 'text', 'about'):
            ctx, base, extracted, verified = fixture()
            if fault == 'future': ctx['as_of'] = '2024-01-01T00:00:00Z'
            elif fault == 'query': ctx['question'] = 'Who owns Amber?'
            elif fault == 'role': base['memory']['entries'][0]['metadata']['source_role'] = 'assistant'
            elif fault == 'text': base['memory']['entries'][0]['text'] += ' changed'
            else: base['about'] = 'eval:other'
            with self.assertRaises(ValueError):
                compile_plan(ctx, base, extracted, verified, 'test', '2024-02-01T00:00:00Z')

    def test_missing_identity_verdict_cannot_accept_a_pair(self):
        ctx, base, extracted, _ = fixture()
        with self.assertRaises(ValueError):
            compile_plan(ctx, base, extracted, {'verdicts': {}}, 'test', '2024-02-01T00:00:00Z')

    def test_negative_identity_verdict_preserves_sources_without_links(self):
        ctx, base, extracted, verified = fixture()
        for value in verified['verdicts'].values():
            value.update(same_entity=False, reason='The reference could identify different people.')
        plan = compile_plan(ctx, base, extracted, verified, 'test', '2024-02-01T00:00:00Z')
        self.assertEqual(plan['ingest']['memory'], base['memory'])
        self.assertEqual(plan['accepted_count'], 0)
        self.assertEqual(len(plan['rejected']), 1)

    def test_no_identity_from_same_name_basis_and_no_duplicate_pairs(self):
        ctx, _, extracted, _ = fixture()
        duplicate = copy.deepcopy(extracted['proposals'][0])
        duplicate['left'], duplicate['right'] = duplicate['right'], duplicate['left']
        extracted['proposals'].append(duplicate)
        proposed, rejected = candidates(extracted, ctx)
        self.assertEqual((len(proposed), len(rejected)), (1, 1))
        extracted['proposals'][0]['basis'] = 'same_name'
        proposed, rejected = candidates({'proposals': extracted['proposals'][:1]}, ctx)
        self.assertEqual((len(proposed), len(rejected)), (0, 1))

    def test_literal_occurrence_and_quotes_cannot_point_elsewhere(self):
        ctx, _, extracted, _ = fixture()
        ctx['sources'][0]['text'] = 'Alex Martin and Alex Stone are different people.'
        value = mention({'source_id': 'a', 'text': 'Alex', 'occurrence': 1}, {'a': ctx['sources'][0]})
        self.assertEqual(value['start'], 16)
        with self.assertRaises(ValueError):
            mention({'source_id': 'a', 'text': 'Alex', 'occurrence': 2}, {'a': ctx['sources'][0]})
        ctx, _, extracted, _ = fixture()
        extracted['proposals'][0]['citations'].pop()
        self.assertEqual(len(candidates(extracted, ctx)[0]), 0)

    def test_empty_extraction_does_not_make_a_verifier_call(self):
        ctx, _, _, _ = fixture()
        calls = []

        class EmptyModel:
            def generate(self, messages, schema, phase):
                calls.append((messages, schema, phase))
                return {'proposals': []}

        result = resolve(ctx, EmptyModel())
        self.assertEqual(result, ({'proposals': []}, {'verdicts': {}}))
        self.assertEqual([c[2] for c in calls], ['entity_extract'])
        self.assertNotIn('Who owns', calls[0][0][1]['content'])


if __name__ == '__main__':
    unittest.main()
