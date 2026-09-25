"""Canonical digests and the section 8 cache keys."""
import hashlib
import unittest

from .. import BENCH_VERSION
from ..domain import cachekey
from ..domain.errors import CacheKeyInvalid

H = {name: str(digit) * 64 for digit, name in enumerate(
    ('binary_sha256', 'config_digest', 'store_key', 'questions_digest', 'reader'), start=1)}


def _result(**overrides):
    fields = {name: H[name] for name in ('binary_sha256', 'config_digest', 'store_key', 'questions_digest')}
    return cachekey.result_key(**{**fields, 'mode': 'quick-a', **overrides})


class CanonicalTest(unittest.TestCase):
    def test_canonical_json_is_sorted_compact_utf8(self):
        self.assertEqual(cachekey.canonical_json({'b': [1, 'ñ'], 'a': None, 'c': (True, 1.5)}),
                         '{"a":null,"b":[1,"ñ"],"c":[true,1.5]}')

    def test_digest_is_sha256_of_the_canonical_text(self):
        text = '{"a":1,"b":[1,2,"ñ"]}'
        self.assertEqual(cachekey.digest({'b': [1, 2, 'ñ'], 'a': 1}),
                         hashlib.sha256(text.encode('utf-8')).hexdigest())

    def test_pinned_vector(self):
        """Changing the canonical form silently would orphan every cache entry."""
        self.assertEqual(cachekey.digest({'a': 1, 'b': [1, 2, 'ñ']}),
                         '79c659a1c4e632fd30860256ab1b1bacfbd28a3f5a3b919d4191d4d1d7a935a8')

    def test_refuses_what_json_cannot_hold_exactly(self):
        for value in ({1: 'int key'}, {'x': float('nan')}, {'x': float('inf')}, {'x': {1, 2}}, {'x': b'x'}):
            with self.assertRaises(CacheKeyInvalid):
                cachekey.digest(value)

    def test_ordered_digest_is_order_free_and_refuses_duplicates(self):
        pairs = [('b', H['store_key']), ('a', H['config_digest'])]
        self.assertEqual(cachekey.ordered_digest(pairs), cachekey.ordered_digest(reversed(pairs)))
        with self.assertRaises(CacheKeyInvalid):
            cachekey.ordered_digest(pairs + [('a', H['reader'])])
        with self.assertRaises(CacheKeyInvalid):
            cachekey.ordered_digest([('a', 'not-hex')])


class KeysTest(unittest.TestCase):
    def test_result_key_pinned_vector(self):
        self.assertEqual(BENCH_VERSION, 'kmp.memory_bench.v1')
        self.assertEqual(_result(), 'd69566426316216abb930f49452b8c6d3fdb12cd4a70d82fcc8b3753d6c978e0')

    def test_every_component_changes_the_result_key(self):
        base = _result()
        variants = [_result(mode='full'), _result(bench_version='kmp.memory_bench.v2'),
                    _result(nonce='2026-09-26T08:00:00Z')]
        for name in ('binary_sha256', 'config_digest', 'store_key', 'questions_digest'):
            variants.append(_result(**{name: H['reader']}))
        self.assertEqual(len({base, *variants}), len(variants) + 1)

    def test_result_key_refuses_malformed_material(self):
        with self.assertRaises(CacheKeyInvalid):
            _result(binary_sha256='ABC')
        with self.assertRaises(CacheKeyInvalid):
            _result(mode='Quick A')

    def test_kinds_never_collide(self):
        source = cachekey.synth_source(generator='synth-v1', generator_version='1.0.0', seed=7,
                                       topology='mono', world_digest=H['reader'])
        store = cachekey.store_key(source=source, n=1000, bundle_format=3, reader_sha256=H['reader'],
                                   build='import')
        world = cachekey.world_key(generator='synth-v1', generator_version='1.0.0', seed=7,
                                   topology='mono', max_level=1000, block_size=1000)
        report = cachekey.report_key(result_keys=[_result()], options={})
        self.assertEqual(len({store, world, report, _result()}), 4)

    def test_store_key_covers_n_reader_build_batch_and_source(self):
        synth = cachekey.synth_source(generator='synth-v1', generator_version='1.0.0', seed=7,
                                      topology='mono', world_digest=H['reader'])
        real = cachekey.bundle_source(content_digest='sha256:' + H['reader'], label='real-2026-09-25')
        base = dict(source=synth, n=1000, bundle_format=3, reader_sha256=H['reader'], build='ingest',
                    batch_size=1000)
        keys = {cachekey.store_key(**base)}
        for change in (dict(n=10000), dict(reader_sha256=H['store_key']), dict(build='import'),
                       dict(batch_size=5000), dict(source=real), dict(store_mode='own'),
                       dict(writer_sha256=H['binary_sha256'])):
            keys.add(cachekey.store_key(**{**base, **change}))
        self.assertEqual(len(keys), 8)
        with self.assertRaises(CacheKeyInvalid):
            cachekey.store_key(**{**base, 'build': 'guess'})
        with self.assertRaises(CacheKeyInvalid):
            cachekey.store_key(**{**base, 'source': {'kind': 'other'}})
        with self.assertRaises(CacheKeyInvalid):
            cachekey.bundle_source(content_digest=H['reader'], label='missing prefix')

    def test_report_key_ignores_result_order(self):
        a, b = _result(), _result(mode='full')
        self.assertEqual(cachekey.report_key(result_keys=[a, b], options={'x': 1}),
                         cachekey.report_key(result_keys=[b, a], options={'x': 1}))

    def test_drift_names_the_fields_that_differ(self):
        first = {'questions_digest': 'q', 'driver_version': 'd', 'encoders': ['o200k_base'],
                 'bench_version': BENCH_VERSION, 'other': 1}
        self.assertEqual(cachekey.drift(first, dict(first, other=2)), ())
        self.assertEqual(cachekey.drift(first, dict(first, driver_version='d2', encoders=[])),
                         ('driver_version', 'encoders'))


if __name__ == '__main__':
    unittest.main()
