"""Control coverage flow and provenance, not model quality."""
import copy
import json
import unittest
from contracts import candidates
from form import form
from local_client import LocalClient
from test_formation import SOURCE, GOOD, BAD


class Model:
    def __init__(self, draft, revised):
        self.draft, self.revised, self.calls = draft, revised, []

    def generate(self, messages, schema, phase):
        data = json.loads(messages[1]['content'])
        self.calls.append((phase, data))
        if phase == 'formation_extract':
            return {'memories': self.draft}
        if phase == 'formation_review':
            return {'memories': self.revised, 'review_notes': 'Source-based revision for this fixture.'}
        return {'verdicts': {m['id']: {'supported': m['text'] == GOOD['text'],
                             'reason': 'The source explicitly rejects PostgreSQL.'} for m in data['candidates']}}


class CoverageTests(unittest.TestCase):
    def test_empty_draft_is_reviewed_once_and_recovery_still_requires_verification(self):
        model = Model([], [GOOD])
        stages = {}
        extracted, verified = form(SOURCE, model, lambda name, value: stages.update({name: value}))
        self.assertEqual([phase for phase, _ in model.calls],
                         ['formation_extract', 'formation_review', 'formation_verify'])
        self.assertEqual(stages['initial-extracted'], {'memories': []})
        self.assertEqual(stages['coverage-review']['memories'], [GOOD])
        self.assertEqual(extracted['memories'], [GOOD])
        self.assertTrue(verified['verdicts'][0]['supported'])
        for _, data in model.calls:
            self.assertEqual(data['sources'], SOURCE['sources'])

    def test_revision_replaces_draft_and_cannot_bypass_verifier(self):
        model = Model([GOOD], [BAD])
        extracted, verified = form(SOURCE, model)
        self.assertEqual(extracted, {'memories': [BAD]})
        self.assertFalse(verified['verdicts'][0]['supported'])

    def test_foreign_review_quotes_are_not_sent_to_verifier_or_admitted(self):
        foreign = copy.deepcopy(GOOD)
        foreign['citations'][0]['source_id'] = 'foreign'
        model = Model([], [foreign])
        extracted, verified = form(SOURCE, model)
        self.assertEqual(len(model.calls), 2)
        self.assertEqual(verified, {'verdicts': []})
        admitted, rejected = candidates(extracted, SOURCE)
        self.assertEqual(admitted, [])
        self.assertEqual(len(rejected), 1)

    def test_empty_final_review_is_not_retried_or_filled(self):
        model = Model([GOOD], [])
        extracted, verified = form(SOURCE, model)
        self.assertEqual(extracted, {'memories': []})
        self.assertEqual(verified, {'verdicts': []})
        self.assertEqual(len(model.calls), 2)

    def test_only_coverage_review_enables_thinking_and_each_request_records_settings(self):
        client = LocalClient('http://127.0.0.1:8000/v1', 'test', lambda event: None)
        calls = []
        def request(endpoint, body, phase):
            calls.append((phase, body))
            return {'count': 10} if phase.endswith('_tokenize') else {
                'choices': [{'finish_reason': 'stop', 'message': {'content': '{}'}}]}
        client.request = request
        for phase in ('formation_extract', 'formation_review', 'formation_verify'):
            client.generate([], {}, phase)
        generated = {phase: body for phase, body in calls if not phase.endswith('_tokenize')}
        self.assertEqual([generated[p]['chat_template_kwargs']['enable_thinking'] for p in generated],
                         [False, True, False])
        self.assertEqual([generated[p]['temperature'] for p in generated], [0, 1.0, 0])
        self.assertEqual(generated['formation_review']['top_p'], 1.0)
        self.assertTrue(all(b['max_tokens'] == 4096 for b in generated.values()))


if __name__ == '__main__':
    unittest.main()
