import json
import unittest

from ..capture.strict_json import member_spans
from ..domain.exposure import Exposure, ExposureKind, Stage
from ..domain.representation import RepresentationId
from ..representations import units
from ..representations.lexical import compact_lexical


def exposure(kind=ExposureKind.RESPONSE):
    return Exposure('j/00001/' + kind.value, 'j', 1, kind, Stage.MEMORY, 'tools/call', 'kmp_recall', 1)


class LexicalTest(unittest.TestCase):
    def test_strings_numbers_and_escapes_survive(self):
        raw = '{ "text" : "a  b",\n  "n": 1.0e10, "x": -0.50, "u": "\\u00e9 é", "q": "\\"  \\\\" }'
        compacted = compact_lexical(raw)
        self.assertEqual(compacted, '{"text":"a  b","n":1.0e10,"x":-0.50,"u":"\\u00e9 é","q":"\\"  \\\\"}')
        self.assertEqual(json.loads(compacted), json.loads(raw))

    def test_unterminated_string_rejected(self):
        with self.assertRaises(ValueError):
            compact_lexical('{"a": "b')

    def test_member_spans_are_exact(self):
        line = '{"request": {"id": 1}, "response": {"n": 1.0e10}}'
        spans = member_spans(line)
        self.assertEqual(line[slice(*spans['response'])], '{"n": 1.0e10}')


class ViewTest(unittest.TestCase):
    def test_legacy_matches_surface_acceptance_metrics_serializer(self):
        payload = {'b': 'é', 'a': [1, 2.5]}
        [row] = units(RepresentationId.MCP_JSON_COMPACT_LEGACY_V1, exposure(), payload, '')
        self.assertEqual(row.text, json.dumps(payload, ensure_ascii=False, separators=(',', ':')))

    def test_missing_structured_content_is_a_text_result_not_empty(self):
        payload = {'result': {'content': [{'type': 'text', 'text': 'hi'}]}}
        [row] = units(RepresentationId.STRUCTURED_COMPACT_V1, exposure(), payload, '')
        self.assertIsNone(row.text)
        self.assertEqual(row.absent_reason, 'text_only_result')

    def test_structured_view_does_not_apply_to_requests(self):
        self.assertEqual(units(RepresentationId.STRUCTURED_COMPACT_V1,
                               exposure(ExposureKind.REQUEST), {'method': 'x'}, ''), [])

    def test_content_blocks_are_standalone_and_skip_binary(self):
        payload = {'result': {'content': [{'type': 'text', 'text': 'one'},
                                          {'type': 'image', 'data': 'AAAA'}]}}
        rows = units(RepresentationId.CONTENT_BLOCKS_STANDALONE_V1, exposure(), payload, '')
        self.assertEqual([(r.unit_id, r.text, r.absent_reason) for r in rows],
                         [('j/00001/response/content/0', 'one', None),
                          ('j/00001/response/content/1', None, 'non_text_block')])


if __name__ == '__main__':
    unittest.main()
