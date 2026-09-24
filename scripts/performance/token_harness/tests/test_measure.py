from pathlib import Path
import tempfile
import unittest

from ..application.measure import measure_journey
from ..application.verify import verify
from ..domain.representation import RepresentationId
from .fakes import FakeCounter, sample_events, write_capture


class MeasureTest(unittest.TestCase):
    def setUp(self):
        with tempfile.TemporaryDirectory() as folder:
            run_dir = write_capture(Path(folder) / 'run', {'lesson': sample_events()})
            _, traces, _ = verify(run_dir, 1 << 20)
        self.result = measure_journey(traces[0], [FakeCounter()], list(RepresentationId), 1 << 20)
        self.series = self.result['totals']['fake_chars']

    def test_journey_total_is_the_sum_of_whole_request_and_response_units(self):
        legacy = self.series['mcp_json_compact_legacy_v1']
        rows = [m for m in self.result['measurements']
                if m['representation'] == 'mcp_json_compact_legacy_v1']
        self.assertEqual(legacy['all']['tokens'], sum(m['tokens'] for m in rows))
        stage_sum = sum(kind['tokens'] for stage in legacy['by_stage'].values() for kind in stage.values())
        self.assertEqual(stage_sum, legacy['all']['tokens'])
        self.assertEqual(legacy['by_stage']['guide']['response']['units'], 2)

    def test_lexical_view_is_never_larger_than_the_captured_lexemes(self):
        self.assertLessEqual(self.series['json_compact_lexical_v1']['all']['utf8_bytes'],
                             self.series['mcp_json_compact_legacy_v1']['all']['utf8_bytes'])

    def test_text_only_result_is_absent_in_structured_view(self):
        structured = self.series['structured_compact_v1']['all']
        self.assertEqual(structured['absent'], {'not_a_tool_result': 2, 'text_only_result': 1})
        self.assertEqual(structured['units'], 3)

    def test_complete_capture_has_no_failures(self):
        self.assertTrue(self.result['complete'])


if __name__ == '__main__':
    unittest.main()
