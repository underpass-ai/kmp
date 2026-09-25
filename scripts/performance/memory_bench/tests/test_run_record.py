"""kmp.bench.call.v1 / journey.v1 / run.v1: absent values, censoring and key coherence."""
import copy
import unittest

from ..domain import cachekey
from ..domain.errors import RunRecordInvalid
from ..domain.run_manifest import RunManifest
from ..domain.run_record import CallRecord, JourneyRecord, journey_name, parse_response_path

RUN = cachekey.result_key(binary_sha256='1' * 64, config_digest='2' * 64, store_key='3' * 64,
                          questions_digest='4' * 64, mode='quick-a')
IO = {'rchar': 1, 'wchar': 2, 'syscr': 3, 'syscw': 4, 'read_bytes': 0, 'write_bytes': 0,
      'cancelled_write_bytes': 0}


def call(**overrides):
    record = {'schema': 'kmp.bench.call.v1', 'run_id': RUN, 'question_id': 'q.1', 'variant': 'baseline',
              'sample': 0, 'repeat': 0, 'journey': journey_name('q.1', 0, 0), 'call_index': 0,
              'process_call_index': 0, 'phase': 'first', 'binary_sha256': '1' * 64, 'tool': 'kmp_ask',
              'arguments': {'about': 'a:b', 'question': 'x'}, 'status': 'ok', 'rpc_id': 3,
              'response_path': 'q.1~s0~r0.jsonl#4', 'response_sha256': '7' * 64, 'response_bytes': 10,
              'wall_ns': 5, 'server_ms': 1, 'server_us': 900, 'cpu_ns': 4, 'rss_peak_kb': 2, 'io': IO,
              'jev': {'evaluations': [{'site': 'p', 'questions': 12, 'requests': 1, 'input_tokens': 5800,
                                       'source': 'cassette_hit', 'us': 1500}]},
              'censored': False, 'censor_reason': None, 'absent': {}}
    record.update(overrides)
    return record


def journey(**overrides):
    record = {'schema': 'kmp.bench.journey.v1', 'run_id': RUN, 'question_id': 'q.1', 'variant': 'baseline',
              'sample': 0, 'repeat': 0, 'journey': journey_name('q.1', 0, 0), 'tool': 'kmp_ask',
              'status': 'completed', 'calls': 2, 'max_calls': 256, 'censored': False,
              'censor_reason': None, 'startup_ns': 10, 'error': None, 'absent': {}}
    record.update(overrides)
    return record


def manifest(**overrides):
    record = {'schema': 'kmp.bench.run.v1', 'run_id': RUN,
              'key': {'binary_sha256': '1' * 64, 'config_digest': '2' * 64, 'store_key': '3' * 64,
                      'questions_digest': '4' * 64, 'mode': 'quick-a', 'bench_version': 'kmp.memory_bench.v1',
                      'nonce': None},
              'variant': {'name': 'baseline', 'path': 'v.toml', 'preregistration_digest': '5' * 64,
                          'config_digest': '2' * 64},
              'binary': {'sha256': '1' * 64, 'version': '0.23.0', 'provenance': None},
              'store': {'key': '3' * 64, 'content_digest': None, 'label': 'x'},
              'questions': {'digest': '4' * 64, 'count': 1, 'by_corpus': {'synth-v1': 1}},
              'driver_version': 'kmp.native_driver.v1',
              'encoders': [{'encoding': 'o200k_base', 'asset_sha256': 'a' * 64}],
              'samples': 1, 'repeats': 1, 'max_calls': 256, 'timeout_s': 60, 'private': False}
    record.update(overrides)
    return record


class CallRecordTest(unittest.TestCase):
    def refuse(self, record, fragment):
        with self.assertRaises(RunRecordInvalid) as caught:
            CallRecord.from_dict(record)
        self.assertIn(fragment, str(caught.exception))

    def test_round_trip(self):
        parsed = CallRecord.from_dict(call())
        self.assertEqual(CallRecord.from_dict(parsed.as_dict()), parsed)
        self.assertEqual(parsed.as_dict(), call())
        self.assertEqual(parsed.jev[0].input_tokens, 5800)
        self.assertEqual(parsed.response_location(), ('q.1~s0~r0.jsonl', 4))

    def test_absent_values_carry_a_reason_and_only_absent_values_do(self):
        missing = call(server_us=None, jev=None,
                       absent={'server_us': 'duration_ms only', 'jev': 'no judgement telemetry'})
        self.assertIsNone(CallRecord.from_dict(missing).jev)
        self.refuse(call(cpu_ns=None), 'present or absent')
        self.refuse(call(absent={'cpu_ns': 'no schedstat'}), 'present or absent')
        self.refuse(call(cpu_ns=None, absent={'cpu_ns': ''}), 'non-empty reason')
        self.refuse(call(absent={'question_id': 'x'}), 'not a measured field')

    def test_empty_evaluations_mean_telemetry_without_judgements(self):
        self.assertEqual(CallRecord.from_dict(call(jev={'evaluations': []})).jev, ())

    def test_timeouts_are_censored_transport_errors_with_a_lower_bound(self):
        absent = {name: 'timed out' for name in ('response_path', 'response_sha256', 'response_bytes',
                                                 'server_ms', 'server_us', 'jev')}
        timed_out = call(status='transport_error', censored=True, censor_reason='timeout',
                         response_path=None, response_sha256=None, response_bytes=None,
                         server_ms=None, server_us=None, jev=None, absent=absent)
        self.assertTrue(CallRecord.from_dict(timed_out).censored)
        self.refuse(dict(timed_out, censor_reason=None), 'go together')
        self.refuse(dict(timed_out, status='tool_error'), 'lower bound')
        self.refuse(dict(timed_out, wall_ns=None, absent=dict(absent, wall_ns='x')), 'lower bound')
        self.refuse(call(censored=True, censor_reason='max_calls'), 'censor_reason')

    def test_structural_refusals(self):
        cases = [(call(status='ok', response_path=None, absent={'response_path': 'lost'}), 'points at'),
                 (call(call_index=256), 'call_index'), (call(run_id='abc'), 'run_id'),
                 (call(phase='hot'), 'phase'), (call(tool='ask'), 'tool'),
                 (call(response_path='q.1.jsonl'), 'response_path'),
                 (call(io=dict(IO, rchar=-1)), 'rchar'), (call(io={'rchar': 1}), 'required'),
                 (call(jev={'evaluations': [{'site': 'p'}]}), 'required'),
                 (call(jev={'evaluations': [], 'total': 1}), 'unknown field'),
                 (call(wall_ns=True), 'integer'), (dict(call(), extra=1), 'unknown field'),
                 (call(schema='kmp.bench.call.v0'), 'schema')]
        for record, fragment in cases:
            with self.subTest(fragment=fragment):
                self.refuse(record, fragment)

    def test_response_path_parser(self):
        self.assertEqual(parse_response_path('a~s1~r2.jsonl#0'), ('a~s1~r2.jsonl', 0))
        for bad in ('a.jsonl#01', 'dir/a.jsonl#1', 'a.json#1', '', None):
            with self.assertRaises(RunRecordInvalid):
                parse_response_path(bad)


class JourneyRecordTest(unittest.TestCase):
    def test_round_trip_and_rules(self):
        parsed = JourneyRecord.from_dict(journey())
        self.assertEqual(parsed.as_dict(), journey())
        capped = journey(status='call_cap_reached', calls=256, censored=True, censor_reason='max_calls')
        JourneyRecord.from_dict(capped)
        for record, fragment in ((journey(calls=257), 'calls'), (journey(calls=5, max_calls=4), 'more calls'),
                                 (journey(status='call_cap_reached'), 'max_calls'),
                                 (journey(censored=True), 'go together'),
                                 (journey(startup_ns=None), 'present or absent'),
                                 (journey(status='done'), 'status')):
            with self.subTest(fragment=fragment), self.assertRaises(RunRecordInvalid) as caught:
                JourneyRecord.from_dict(record)
            self.assertIn(fragment, str(caught.exception))

    def test_journey_names_never_collide_with_run_files(self):
        self.assertEqual(journey_name('retrieval-exact-term', 2, 1), 'retrieval-exact-term~s2~r1')
        self.assertNotIn(journey_name('calls', 0, 0), ('calls', 'journeys', 'run', 'manifest'))


class RunManifestTest(unittest.TestCase):
    def test_valid_manifest_and_comparability(self):
        parsed = RunManifest.from_dict(manifest())
        self.assertEqual(parsed.run_id, RUN)
        self.assertEqual(parsed.comparability(),
                         {'questions_digest': '4' * 64, 'driver_version': 'kmp.native_driver.v1',
                          'encoders': [('o200k_base', 'a' * 64)], 'bench_version': 'kmp.memory_bench.v1'})

    def test_key_coherence(self):
        edited = copy.deepcopy(manifest())
        edited['key']['mode'] = 'full'
        cases = [(edited, 'run_id is not the result key'),
                 (manifest(binary={'sha256': '9' * 64, 'version': None, 'provenance': None}), 'binary.sha256'),
                 (manifest(store={'key': '9' * 64}), 'store.key'),
                 (manifest(questions={'digest': '9' * 64}), 'questions.digest'),
                 (manifest(variant={'name': 'x', 'config_digest': '9' * 64}), 'config_digest'),
                 (manifest(private=None), 'required'), (manifest(private='no'), 'boolean'),
                 (manifest(max_calls=257), 'max_calls'), (manifest(encoders=[{'x': 1}]), 'encoders'),
                 (manifest(store={'key': '3' * 64, 'path': '/x'}), 'unknown field'),
                 (dict(manifest(), extra=1), 'unknown field')]
        missing = copy.deepcopy(manifest())
        del missing['key']['bench_version']
        cases.append((missing, 'bench_version'))
        for record, fragment in cases:
            with self.subTest(fragment=fragment), self.assertRaises(RunRecordInvalid) as caught:
                RunManifest.from_dict(record)
            self.assertIn(fragment, str(caught.exception))


if __name__ == '__main__':
    unittest.main()
