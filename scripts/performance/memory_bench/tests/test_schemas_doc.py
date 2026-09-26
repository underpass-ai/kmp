"""Every example in SCHEMAS.md is valid against the code, and the fixtures are canonical."""
import json
from pathlib import Path
import re
import unittest

from ..application import jev_tail, mixed_versions
from ..domain import jsonl
from ..domain.run_record import JEV_SOURCES
from ..runtime import jev_fixture
from ..domain.question import Question, load_questions
from ..domain.run_manifest import RunManifest
from ..domain.run_record import CallRecord, JourneyRecord
from ..domain.variant import parse_variant_toml
from ..runtime.store_cache import StoreRecord

PACKAGE = Path(__file__).resolve().parents[1]
FIXTURES = Path(__file__).resolve().parent / 'fixtures'
REPORT_SECTIONS = ('provenance', 'headlines', 'by_type', 'scale', 'latency_resources', 'tokens',
                   'jev_by_site', 'controls', 'power', 'limitations', 'verdict')
def check_jev_fixture(test, value):
    test.assertEqual(value['schema'], jev_fixture.SCHEMA)
    test.assertEqual(set(value), {'schema', 'fixture_key', 'sample', 'backend', 'model', 'file', 'sha256',
                                  'request_keys', 'by_source', 'telemetry'})
    test.assertIn(value['backend'], jev_fixture.BACKENDS)
    test.assertLessEqual(set(value['by_source']), set(JEV_SOURCES))


def check_public_recall(test, value):
    test.assertEqual(value['schema'], 'kmp.bench.public_recall.v1')
    test.assertEqual(set(value), {'schema', 'corpus', 'binary_sha256', 'variant', 'max_calls', 'run', 'load',
                                  'load_elapsed_s', 'recall', 'scope'})
    test.assertEqual(set(value['recall']), {'run_id', 'statuses', 'not_run', 'corpora'})


def check_mixed_versions(test, value):
    test.assertEqual(value['schema'], mixed_versions.SCHEMA)
    test.assertEqual(set(value), {'schema', 'binaries', 'settings', 'counts', 'results'})
    test.assertEqual(set(value['settings']), set(mixed_versions.Settings().as_dict()))
    test.assertEqual(set(value['counts']), {mixed_versions.PASS, mixed_versions.FAIL, mixed_versions.SKIPPED})
    shape = mixed_versions.ScenarioResult('s', 'c', mixed_versions.PASS, None).as_dict()
    for result in value['results']:
        test.assertEqual(set(result), set(shape))
        test.assertIn(result['scenario'], mixed_versions.SCENARIOS)


def check_jev_tail(test, value):
    expected = jev_tail.skipped(jev_tail.NO_KEY, jev_tail.TailSettings())
    test.assertEqual(set(value), set(expected))
    test.assertEqual({k: value[k] for k in ('schema', 'status', 'reason', 'settings', 'levels')},
                     {k: expected[k] for k in ('schema', 'status', 'reason', 'settings', 'levels')})


OTHER_CONTRACTS = {'jev-fixture': check_jev_fixture, 'public-recall': check_public_recall,
                   'mixed-versions': check_mixed_versions, 'jev-tail': check_jev_tail}
BLOCK = re.compile(r'^```(json|toml) ([a-z-]+)\n(.*?)^```$', re.M | re.S)


def examples():
    text = (PACKAGE / 'SCHEMAS.md').read_text(encoding='utf-8')
    return [(match.group(2), match.group(3)) for match in BLOCK.finditer(text)]


class SchemasDocTest(unittest.TestCase):
    def test_every_contract_has_examples(self):
        tags = [tag for tag, _ in examples()]
        for tag in ('question', 'call', 'journey', 'run', 'variant', 'report', 'world-entry',
                    'world-relation', 'world-write', 'world-manifest', 'world-ingest', 'world-about', 'store',
                    *OTHER_CONTRACTS):
            self.assertIn(tag, tags)

    def test_examples_validate(self):
        readers = {'question': Question.from_dict, 'call': CallRecord.from_dict,
                   'journey': JourneyRecord.from_dict, 'run': RunManifest.from_dict,
                   'store': StoreRecord.from_dict}
        for index, (tag, body) in enumerate(examples()):
            with self.subTest(index=index, tag=tag):
                if tag == 'variant':
                    parse_variant_toml(body, f'SCHEMAS.md example {index}', jsonl.REPO_ROOT)
                    continue
                value = json.loads(body)
                if tag in readers:
                    readers[tag](value)
                elif tag in OTHER_CONTRACTS:
                    OTHER_CONTRACTS[tag](self, value)
                elif tag == 'report':
                    self.assertEqual(value['schema'], 'kmp.bench.report.v1')
                    self.assertEqual(tuple(k for k in value if k in REPORT_SECTIONS), REPORT_SECTIONS)
                else:
                    self.assertTrue(tag.startswith('world-'), tag)

    def test_world_ingest_example_follows_the_conversion_rule(self):
        blocks = dict(examples())
        entry, ingest = json.loads(blocks['world-entry']), json.loads(blocks['world-ingest'])
        sent = ingest['memory']['entries'][0]
        self.assertEqual(sent, {'id': entry['ref'], 'kind': entry['kind'], 'text': entry['text'],
                                'coordinates': entry['coordinates']})
        self.assertTrue(entry['ref'].startswith(entry['about'] + ':e'))
        self.assertEqual(entry['coordinates'][0]['sequence'], entry['ordinal'] + 1)

    def test_fixtures_are_valid_and_canonical(self):
        path = FIXTURES / 'questions.sample.jsonl'
        self.assertEqual(jsonl.dump_lines(q.as_dict() for q in load_questions(path)),
                         path.read_text(encoding='utf-8'))
        for name, reader in (('calls.sample.jsonl', CallRecord), ('journeys.sample.jsonl', JourneyRecord)):
            text = (FIXTURES / name).read_text(encoding='utf-8')
            rows = jsonl.parse_lines(text, name, ValueError)
            self.assertEqual(jsonl.dump_lines(reader.from_dict(row).as_dict() for _, row in rows), text)


if __name__ == '__main__':
    unittest.main()
