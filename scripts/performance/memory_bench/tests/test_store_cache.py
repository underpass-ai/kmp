"""BT14: store keys, the store cache, the CLI verbs around kmp-mcp and the build itself.

The end-to-end case builds the 10^3 mono store with the release binary
(`target/release/kmp-mcp`) into a temporary cache root, imports it, and asks a few
questions on both templates; it is skipped with a reason when the binary is not built.
"""
import contextlib
import io
import json
import os
from pathlib import Path
import stat
import tempfile
import textwrap
import unittest

from .. import cli
from ..application import build
from ..domain import cachekey, ingest_growth
from ..generator import world as synth
from ..runtime import binaries
from ..runtime.layout import public_layout
from ..runtime.store_cache import (StoreCache, StoreCacheError, StoreRecord, copy_template,
                                   template_summary)
from ...token_harness.native.isolation import create_store

HEX = 'ab' * 32
DIGEST = 'sha256:' + 'cd' * 32
_WORLD = {}


def small_world():
    if 'mono' not in _WORLD:
        _WORLD['mono'] = synth.generate(synth.WorldParams(7, 'mono', (1000,)))
    return _WORLD['mono']


def material(**overrides):
    base = {'source': cachekey.synth_source(generator='synth-v1', generator_version='1.1.0', seed=7,
                                            topology='mono', world_digest=HEX),
            'n': 1000, 'bundle_format': {'bundle_format': 3, 'event_format': 2},
            'reader_sha256': HEX, 'build': 'import', 'writer_sha256': HEX, 'batch_size': 1000,
            'store_mode': 'shared'}
    base.update(overrides)
    return base


def record(staging, **overrides):
    values = {'material': material(), 'label': 'test', 'content_digest': DIGEST, 'event_count': 1,
              'bundle': {'sha256': cachekey.sha256_hex((staging / 'bundle.jsonl').read_bytes()),
                         'bytes': 3, 'header': {}},
              'template': template_summary(staging / 'store'),
              'timings_ms': {'ingest': None, 'export': 1.0, 'import': 2.0, 'copy': 0.5},
              'absent': {'ingest': 'imported'}, 'ingest': None, 'checks': {}, 'created_at': '2026-09-26T00:00:00Z'}
    values.update(overrides)
    values['store_key'] = cachekey.store_key(**values['material'])
    return StoreRecord(**values)


def stage_entry(cache, key='x' * 16):
    staging = cache.stage(key)
    (staging / 'store' / 'store').mkdir(parents=True)
    (staging / 'store' / 'FORMAT_VERSION').write_text('1\n')
    (staging / 'bundle.jsonl').write_text('{}\n')
    return staging


def _json_chunks(text):
    decoder, index, chunks = json.JSONDecoder(), 0, []
    while index < len(text):
        if text[index].isspace():
            index += 1
            continue
        value, end = decoder.raw_decode(text, index)
        chunks.append(json.dumps(value))
        index = end
    return chunks


class StoreKeyTest(unittest.TestCase):
    def test_every_named_part_changes_the_key(self):
        base = cachekey.store_key(**material())
        source = material()['source']
        variants = {
            'generator': dict(source, generator='synth-v2'),
            'generator_version': dict(source, generator_version='1.0.0'),
            'seed': dict(source, seed=11), 'topology': dict(source, topology='multi'),
            'world_digest': dict(source, world_digest='ef' * 32)}
        for name, changed in variants.items():
            with self.subTest(part=name):
                self.assertNotEqual(cachekey.store_key(**material(source=changed)), base)
        for name, value in (('n', 10000), ('bundle_format', {'bundle_format': 4, 'event_format': 2}),
                            ('reader_sha256', 'ef' * 32), ('writer_sha256', 'ef' * 32),
                            ('batch_size', 5000), ('build', 'ingest')):
            with self.subTest(part=name):
                self.assertNotEqual(cachekey.store_key(**material(**{name: value})), base)

    def test_ingest_and_import_materials(self):
        world = small_world()
        request = build.BuildRequest(7, 'mono', 1000, 1000, None)
        writer = binaries.KmpBinary(Path('/w'), 'aa' * 32)
        reader = binaries.KmpBinary(Path('/r'), 'bb' * 32)
        header = {'bundle_format': 3, 'event_format': 2}
        ingest = build.ingest_material(request, world, writer)
        self.assertEqual((ingest['build'], ingest['bundle_format'], ingest['reader_sha256']),
                         ('ingest', build.INGEST_FORMAT, writer.sha256))
        self.assertEqual(ingest['source']['world_digest'], world.level_digests['1000'])
        imported = build.import_material(request, world, writer, reader, header)
        self.assertEqual((imported['reader_sha256'], imported['writer_sha256'], imported['bundle_format']),
                         (reader.sha256, writer.sha256, header))
        bigger = build.BuildRequest(7, 'mono', 1000, 5000, None)
        self.assertNotEqual(cachekey.store_key(**build.ingest_material(bigger, world, writer)),
                            cachekey.store_key(**ingest))


class StoreCacheTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.layout = public_layout(self.folder.name)
        self.cache = StoreCache(self.layout)

    def tearDown(self):
        self.folder.cleanup()

    def test_commit_get_materialize_and_never_overwrite(self):
        staging = stage_entry(self.cache)
        entry = self.cache.commit(staging, record(staging))
        self.assertFalse(staging.exists())
        again = self.cache.get(entry.record.store_key)
        self.assertEqual(again.record, entry.record)
        self.assertTrue(again.template.is_dir() and again.bundle.is_file())
        path, copy_ms = self.cache.materialize(entry.record.store_key, self.layout.scratch() / 'copy')
        self.assertTrue((path / 'FORMAT_VERSION').is_file())
        self.assertGreaterEqual(copy_ms, 0)
        second = stage_entry(self.cache)
        with self.assertRaises(StoreCacheError):
            self.cache.commit(second, record(second))
        with self.assertRaises(Exception):
            self.cache.materialize(entry.record.store_key, Path(self.folder.name).parent / 'outside')
        listed = self.cache.entries()
        self.assertEqual([(d.name, e is not None, error) for d, e, error in listed],
                         [(entry.record.store_key, True, None)])
        self.assertEqual(self.cache.gc(), [second])
        self.assertIsNone(self.cache.get('ef' * 32))

    def test_cli_lists_materializes_and_collects(self):
        staging = stage_entry(self.cache)
        key = self.cache.commit(staging, record(staging)).record.store_key
        abandoned = stage_entry(self.cache)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(cli.main(['cache', 'ls', '--out', self.folder.name]), 0)
            self.assertEqual(cli.main(['materialize', '--store-key', key, '--out', self.folder.name]), 0)
            self.assertEqual(cli.main(['cache', 'gc', '--out', self.folder.name]), 0)
        listed, materialized, collected = [json.loads(chunk) for chunk in _json_chunks(out.getvalue())]
        self.assertEqual([row['store_key'] for row in listed], [key])
        self.assertTrue(Path(materialized['data_dir']).is_relative_to(self.layout.scratch()))
        self.assertEqual(collected['removed'], [str(abandoned)])
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(cli.main(['materialize', '--store-key', 'ef' * 32, '--out', self.folder.name]), 2)

    def test_commit_refuses_a_bundle_that_is_not_the_recorded_one(self):
        staging = stage_entry(self.cache)
        stale = record(staging)
        (staging / 'bundle.jsonl').write_text('{"changed": true}\n')
        with self.assertRaises(StoreCacheError):
            self.cache.commit(staging, stale)

    def test_record_rules(self):
        staging = stage_entry(self.cache)
        good = record(staging)
        self.assertEqual(StoreRecord.from_dict(json.loads(json.dumps(good.as_dict()))), good)
        with self.assertRaises(StoreCacheError):  # material does not reproduce the key
            StoreRecord(**{**{k: getattr(good, k) for k in good.__dataclass_fields__},
                           'store_key': 'ef' * 32})
        with self.assertRaises(StoreCacheError):  # a null timing needs its reason
            record(staging, absent={})
        with self.assertRaises(StoreCacheError):  # a reason for a measured timing
            record(staging, absent={'ingest': 'x', 'copy': 'y'})
        with self.assertRaises(StoreCacheError):
            StoreRecord.from_dict({**good.as_dict(), 'extra': 1})
        broken = self.layout.store(good.store_key)
        broken.mkdir(parents=True)
        (broken / 'store.json').write_text('{')
        with self.assertRaises(StoreCacheError):
            self.cache.get(good.store_key)

    def test_template_copy_leaves_the_journals_behind(self):
        source = Path(self.folder.name) / 'data'
        (source / 'logs').mkdir(parents=True)
        (source / 'logs' / 'kmp-mcp.log.2026-09-26').write_text('x\n')
        (source / 'store').mkdir()
        (source / 'store' / 'logs').mkdir()  # only the top-level journal folder is excluded
        (source / 'FORMAT_VERSION').write_text('1\n')
        copy_template(source, Path(self.folder.name) / 'copy')
        copied = sorted(p.relative_to(Path(self.folder.name) / 'copy').as_posix()
                        for p in (Path(self.folder.name) / 'copy').rglob('*'))
        self.assertEqual(copied, ['FORMAT_VERSION', 'store', 'store/logs'])


class GrowthTest(unittest.TestCase):
    @staticmethod
    def rows(cost, batch_sizes=(1000, 5000), level=10000):
        return [[0, held, batch, 0, cost(batch, held), None]
                for batch in batch_sizes for held in range(0, level, batch)]

    def test_each_term_is_recovered_and_named(self):
        cases = {'B': lambda b, h: 0.08 * b, 'H': lambda b, h: 0.08 * b + 0.5 * h,
                 'B*H': lambda b, h: 0.08 * b + 1e-4 * b * h, 'B^2': lambda b, h: 0.08 * b + 1e-4 * b * b}
        for term, cost in cases.items():
            with self.subTest(term=term):
                fit = ingest_growth.fit(self.rows(cost))
                coefficients = dict(zip(fit.terms, fit.coefficients))
                self.assertEqual(fit.terms, ingest_growth.TERMS)
                self.assertAlmostEqual(coefficients['B'], 0.08, places=6)
                for other in ('H', 'B*H', 'B^2'):
                    expected = {'H': 0.5, 'B*H': 1e-4, 'B^2': 1e-4}[other] if other == term else 0.0
                    self.assertAlmostEqual(coefficients[other], expected, places=6)
                self.assertLess(fit.residual_ms, 1e-6)
                self.assertEqual(fit.as_dict()['largest_batch'], {'B': 5000, 'H': 5000})

    def test_estimate_sums_the_batches_of_a_bigger_build(self):
        fit = ingest_growth.fit(self.rows(lambda b, h: 0.08 * b + 0.5 * h))
        n, batch = 100000, 5000
        expected = 0.08 * n + 0.5 * sum(range(0, n, batch))
        self.assertAlmostEqual(fit.estimate_ms([n], batch), expected, places=3)
        self.assertAlmostEqual(fit.estimate_ms([1500], 1000), 0.08 * 1500 + 0.5 * 1000, places=6)
        self.assertAlmostEqual(fit.components_ms()['H'], 2500.0, places=6)

    def test_terms_one_batch_size_cannot_separate_are_dropped(self):
        rows = self.rows(lambda b, h: 0.08 * b + 0.5 * h, batch_sizes=(1000,))
        self.assertEqual(ingest_growth.fit(rows).terms, ('B', 'H'))
        self.assertIsNone(ingest_growth.fit(rows[:1]))
        as_dicts = [dict(zip(ingest_growth.COLUMNS, row)) for row in rows]
        self.assertEqual(ingest_growth.fit(as_dicts).coefficients, ingest_growth.fit(rows).coefficients)


FAKE_KMP = textwrap.dedent('''\
    #!/usr/bin/env python3
    import json, os, sys
    verb, path = sys.argv[1], sys.argv[2]
    data = os.environ['KMP_MCP_DATA_DIR']
    if verb == 'export':
        header = {"bundle_format": 3, "event_format": 2, "event_count": 1,
                  "content_digest": "sha256:" + "cd" * 32}
        open(path, 'w').write(json.dumps(header) + "\\n{}\\n")
        where = os.environ.get('FAKE_DATA_DIR', data)
        print(json.dumps({"data_dir": where, "content_digest": header["content_digest"], "event_count": 1}))
    elif verb == 'import':
        print(json.dumps({"events_imported": 1, "mutations_applied": 2}))
    else:
        print('boom', file=sys.stderr); sys.exit(2)
    ''')


class BinariesTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        root = Path(self.folder.name)
        self.path = root / 'kmp-mcp'
        self.path.write_text(FAKE_KMP)
        self.path.chmod(self.path.stat().st_mode | stat.S_IXUSR)
        self.binary = binaries.KmpBinary.locate(self.path)
        self.store = create_store(root / 'work', 'fake')

    def tearDown(self):
        self.folder.cleanup()

    def test_export_import_and_header(self):
        bundle = self.store.root / 'b.jsonl'
        summary, run = binaries.export_bundle(self.binary, self.store, bundle)
        self.assertEqual(summary['event_count'], 1)
        self.assertGreater(run.wall_ms, 0)
        header = binaries.bundle_header(bundle)
        self.assertEqual(binaries.bundle_format(header), {'bundle_format': 3, 'event_format': 2})
        report, _ = binaries.import_bundle(self.binary, self.store, bundle)
        self.assertEqual(report['events_imported'], 1)

    def test_refusals(self):
        with self.assertRaises(binaries.BinaryFailed):
            binaries.run_cli(self.binary, self.store, 'nope', ['x'])
        with self.assertRaises(binaries.BinaryFailed):
            binaries.KmpBinary.locate(Path(self.folder.name) / 'missing')
        other = create_store(Path(self.folder.name) / 'work', 'other')
        env = dict(self.store.env, FAKE_DATA_DIR=str(other.data_dir))
        moved = type(self.store)(self.store.root, self.store.data_dir, env)
        with self.assertRaises(binaries.BinaryFailed):  # it read another store
            binaries.export_bundle(self.binary, moved, self.store.root / 'b.jsonl')
        (self.store.root / 'bad.jsonl').write_text('{"bundle_format": 3}\n')
        with self.assertRaises(binaries.BinaryFailed):
            binaries.bundle_header(self.store.root / 'bad.jsonl')
        self.assertRaises(binaries.BinaryFailed, binaries.CliRun('export', 0, '', '', 1.0).json_line)
        self.assertRaises(binaries.BinaryFailed, binaries.CliRun('export', 0, 'no', '', 1.0).json_line)


class FakeSession:
    def __init__(self, answers):
        self.answers, self.calls = list(answers), []

    def call(self, tool, arguments):
        self.calls.append((tool, arguments))
        return {'result': {'structuredContent': self.answers.pop(0)}}


class WorldAndWriteTest(unittest.TestCase):
    def test_load_world_round_trip_and_tamper(self):
        world = small_world()
        with tempfile.TemporaryDirectory() as folder:
            layout = public_layout(folder)
            directory = layout.world(world.world_key)
            synth.write_world(world, directory)
            self.assertEqual(build.find_world(layout, 7, 'mono', 1000), directory)
            loaded = build.load_world(directory, 1000)
            self.assertEqual(loaded.lines, world.lines)
            self.assertEqual(loaded.blocks_of, world.blocks_of)
            with self.assertRaises(build.BuildFailed):
                build.find_world(layout, 7, 'multi', 1000)
            (directory / 'writes.jsonl').write_text('{}\n')
            with self.assertRaises(build.BuildFailed):
                build.load_world(directory, 1000)

    def test_write_resolves_its_review_verbatim(self):
        review = {'status': 'needs_review',
                  'next_actions': [{'tool': 'kmp_write_memory', 'arguments': {'continuation': 'read_' + '0' * 32}}]}
        committed = {'accepted': True, 'status': 'committed', 'dry_run': False}
        session = FakeSession([review, committed])
        log = io.StringIO()
        self.assertEqual(build._write(session, {'about': 'a'}, 'w', log), 1)
        self.assertEqual(session.calls[1], ('kmp_write_memory', {'continuation': 'read_' + '0' * 32}))
        self.assertIn('verbatim', log.getvalue())
        with self.assertRaises(build.BuildFailed):
            build._write(FakeSession([{'accepted': False, 'status': 'rejected'}]), {}, 'w', log)
        with self.assertRaises(build.BuildFailed):
            build._write(FakeSession([review] * 5), {}, 'w', log)
        with self.assertRaises(build.BuildFailed):
            build._structured({'result': {'isError': True, 'structuredContent': {'error': 'x'}}}, 'w')


@unittest.skipUnless(binaries.DEFAULT_BINARY.is_file(), 'target/release/kmp-mcp not built')
class ReleaseBinaryBuildTest(unittest.TestCase):
    """10^3 mono through the real binary: ingest, export, import, identical answers."""

    def test_ingest_and_import_answer_byte_for_byte(self):
        world = small_world()
        with tempfile.TemporaryDirectory() as folder:
            layout = public_layout(folder)
            synth.write_world(world, layout.world(world.world_key))
            writer = binaries.KmpBinary.locate()
            request = build.BuildRequest(7, 'mono', 1000, 1000, None, 120.0)
            built = build.build_stores(request, writer, layout=layout, log=io.StringIO())
            ingest, imported = built['ingest'].record, built['import'].record
            self.assertEqual(ingest.content_digest, imported.content_digest)
            self.assertEqual(ingest.ingest['entries'], 1000)
            self.assertEqual(imported.checks['reexport_content_digest'], 'equal')
            for entry in (ingest, imported):
                self.assertEqual({k for k, v in entry.timings_ms.items() if v is None}, set(entry.absent))
            again = build.build_stores(request, writer, layout=layout, log=io.StringIO())
            self.assertEqual(again['import'].record, imported)  # cached, not rebuilt
            questions = [q for q in synth.load_questions(world) if q.min_level <= 1000][:6]
            report = build.compare_answers(writer, built['ingest'].template, built['import'].template,
                                           questions, Path(folder) / 'work' / 'answers')
            self.assertTrue(report['stores_agree'], report)
            self.assertEqual(report['different'], 0)
            self.assertEqual(report['control_aa']['different'], 0)
            self.assertEqual(sorted(os.listdir(folder)), ['stores', 'work', 'worlds'])


if __name__ == '__main__':
    unittest.main()
