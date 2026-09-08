import copy
import unittest

from kmp_entities.expand import expand
from kmp_entities.planner import compile_plan
from test_entities import fixture


def graph():
    plan = compile_plan(*fixture(), 'synthetic', '2024-02-01T00:00:00Z')
    memory = plan['ingest']['memory']
    pages = {}
    for entry in memory['entries']:
        ref = entry['id']
        pages[ref] = {'object': {'ref': ref, 'kind': entry['kind'], 'text': entry['text'],
                                 'metadata': entry['metadata']},
            'evidence': [p for p in memory['evidence'] if ref in p['supports']],
            'links': {'incoming': [r for r in memory['relations'] if r['to'] == ref] +
                        [{'from': 'dimension', 'to': ref, 'rel': 'contains_entry', 'coordinate': entry['coordinates'][0]}],
                      'outgoing': [r for r in memory['relations'] if r['from'] == ref]},
            'page': {'has_more': False}}
    return plan, copy.deepcopy(pages)


class ExpansionTests(unittest.TestCase):
    def run_graph(self, plan, pages, cutoff='2024-01-31T00:00:00Z', budget=32):
        return expand(plan, [plan['ingest']['about'] + ':source:b'], cutoff,
                      lambda name, args: copy.deepcopy(pages[args['ref']]), max_calls=budget)

    def test_alias_requires_returned_endpoints_and_complete_literal_proof(self):
        plan, pages = graph()
        result = self.run_graph(plan, pages)
        self.assertEqual(len(result['groups']), 1)
        self.assertEqual({p['text'] for p in result['groups'][0]['quotes']},
                         {'Elena Vega uses the nickname Nora.', 'Nora owns release Amber.'})
        self.assertEqual(result['inspection_calls'], 4)
        self.assertEqual(result['rejected'], [])

    def test_missing_proof_cannot_be_filled_from_the_plan(self):
        plan, pages = graph()
        missing = plan['bindings'][0]['evidence_ids'][0]
        for page in pages.values():
            page['evidence'] = [e for e in page['evidence'] if e['id'] != missing]
        result = self.run_graph(plan, pages)
        self.assertEqual(result['groups'], [])
        self.assertTrue(any('absent' in e['reason'] for e in result['rejected']))

    def test_source_after_cutoff_is_not_admitted(self):
        plan, pages = graph()
        result = self.run_graph(plan, pages, cutoff='2024-01-01T00:00:00Z')
        self.assertEqual(result['groups'], [])
        self.assertTrue(any('cutoff' in e['reason'] for e in result['rejected']))

    def test_later_identity_cannot_be_backdated_to_an_earlier_seed(self):
        plan, pages = graph()
        result = expand(plan, [plan['ingest']['about'] + ':source:a'], '2024-01-01T12:00:00Z',
                        lambda name, args: copy.deepcopy(pages[args['ref']]))
        self.assertEqual(result['groups'], [])
        self.assertTrue(any('future' in e['reason'] for e in result['rejected']))

    def test_changed_returned_mention_or_original_is_rejected(self):
        for ref_kind in ('left', 'source'):
            plan, pages = graph()
            ref = plan['bindings'][0]['left'] if ref_kind == 'left' else plan['ingest']['about'] + ':source:a'
            pages[ref]['object']['text'] = 'A fabricated different person'
            result = self.run_graph(plan, pages)
            self.assertEqual(result['groups'], [])
            self.assertTrue(result['rejected'])

    def test_budget_and_stalled_pagination_never_supply_partial_groups(self):
        plan, pages = graph()
        result = self.run_graph(plan, pages, budget=2)
        self.assertEqual(result['inspection_calls'], 2)
        self.assertEqual(result['groups'], [])
        self.assertTrue(result['budget_exhausted'])
        ref = plan['ingest']['about'] + ':source:b'
        pages[ref]['page'] = {'has_more': True, 'next_cursor': 'unchanged'}
        result = self.run_graph(plan, pages)
        self.assertEqual(result['inspection_calls'], 2)
        self.assertEqual(result['groups'], [])
        self.assertTrue(any('stalled' in e['reason'] for e in result['rejected']))

    def test_source_clock_and_relation_proof_are_required_from_the_graph(self):
        for tamper in ('clock', 'relation'):
            plan, pages = graph()
            for page in pages.values():
                if tamper == 'clock':
                    page['links']['incoming'] = [r for r in page['links']['incoming'] if r['rel'] != 'contains_entry']
                else:
                    for direction in ('incoming', 'outgoing'):
                        for r in page['links'][direction]:
                            if r['rel'] == 'same_entity_as':
                                r['evidence'] = 'Not the declared evidence'
            self.assertEqual(self.run_graph(plan, pages)['groups'], [])


if __name__ == '__main__':
    unittest.main()
