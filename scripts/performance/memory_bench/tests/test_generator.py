"""synth-v1: determinism, the nested ladder, the invariants, calibration and the load calls.

The probe-backed checks (zero lexical overlap, calibration) need
`target/release/kmp_search_probe`; they are skipped with a reason when it is not built.
Generating 10^5 (the <60 s acceptance) runs only with MEMORY_BENCH_SLOW=1.
"""
import collections
import contextlib
import io
import json
import os
from pathlib import Path
import re
import tempfile
import time
import unittest

from .. import cli
from ..application import world as world_app
from ..domain.prng import SplitMix64
from ..generator import bridge, calibration, invariants, render, to_ingest
from ..generator import world as synth
from ..generator.blocks import COMPOSITION, QUESTION_BLOCKS, hub_degree
from ..generator.probe import DEFAULT_PATH, SearchProbe
from ..generator.topology import Topology

PINNED = Path(__file__).resolve().parents[1] / 'config' / 'world-digests.json'
SLOW = os.environ.get('MEMORY_BENCH_SLOW') == '1'
ANCHOR_SHAPE = re.compile(r'(?<![\w#.])(C\d+\.\d+|#\d{5,}|v0\.\d+\.\d+)(?![\w]|\.\w)')
_WORLDS = {}


def world(topology, levels=(1000,)):
    key = (topology, levels)
    if key not in _WORLDS:
        _WORLDS[key] = synth.generate(synth.WorldParams(7, topology, levels))
    return _WORLDS[key]


def probe_or_skip(test):
    if not DEFAULT_PATH.is_file():
        test.skipTest('kmp_search_probe not built (cargo build --release -p kmp-testkit '
                      '--bin kmp_search_probe)')
    return SearchProbe.locate()


class DeterminismTest(unittest.TestCase):
    def test_seed_7_matches_the_pinned_digests(self):
        pinned = json.loads(PINNED.read_text(encoding='utf-8'))
        self.assertEqual((pinned['generator'], pinned['generator_version'], pinned['seed']),
                         ('synth-v1', synth.GENERATOR_VERSION, 7))
        for topology in ('mono', 'multi'):
            with self.subTest(topology=topology):
                generated = world(topology, (1000, 10000))
                for level in ('1000', '10000'):
                    self.assertEqual(generated.level_digests[level], pinned['digests'][topology][level])

    def test_same_parameters_same_bytes_and_other_seeds_differ(self):
        again = synth.generate(synth.WorldParams(7, 'mono', (1000,)))
        self.assertEqual(again.lines, world('mono').lines)
        other = synth.generate(synth.WorldParams(11, 'mono', (1000,)))
        self.assertNotEqual(other.level_digests['1000'], world('mono').level_digests['1000'])

    def test_the_ladder_is_nested(self):
        for topology in ('mono', 'multi'):
            with self.subTest(topology=topology):
                big = world(topology, (1000, 10000))
                self.assertEqual(big.level_digests['1000'], world(topology).level_digests['1000'])
                small = world(topology).lines
                for section, lines in small.items():
                    self.assertEqual(big.lines[section][:len(lines)], lines, section)

    def test_parameters_are_checked(self):
        for levels in ((), (1000, 1000), (1500,), (2000, 1000)):
            with self.assertRaises(synth.WorldError):
                synth.WorldParams(7, 'mono', levels)
        with self.assertRaises(ValueError):
            synth.WorldParams(7, 'star', (1000,))

    @unittest.skipUnless(SLOW, 'set MEMORY_BENCH_SLOW=1 to generate 10^5')
    def test_ten_to_the_fifth_is_fast_and_pinned(self):
        pinned = json.loads(PINNED.read_text(encoding='utf-8'))
        for topology in ('mono', 'multi'):
            started = time.perf_counter()
            generated = synth.generate(synth.WorldParams(7, topology, (1000, 10000, 100000)))
            self.assertLess(time.perf_counter() - started, 60)
            self.assertEqual(generated.level_digests, pinned['digests'][topology])


class WorldShapeTest(unittest.TestCase):
    def test_records_follow_the_contract(self):
        generated = world('multi')
        entries = generated.records('entries')
        self.assertEqual(len(entries), 1000)
        self.assertEqual([e['ordinal'] for e in entries], list(range(1000)))
        for entry in entries:
            self.assertTrue(entry['ref'].startswith(entry['about'] + ':e'))
            coordinate = entry['coordinates'][0]
            self.assertEqual(coordinate['sequence'], entry['ordinal'] + 1)
            self.assertEqual(coordinate['occurred_at'], render.instant(entry['ordinal']))
            self.assertLessEqual(len(entry['text']), render.MAX_TEXT_CHARS)
            self.assertTrue({'subject', 'anchors', 'lang'} <= set(entry['truth']))
        self.assertEqual([e['ref'] for e in entries[:4]],
                         [f'synth:multi-a{i:03d}:e{i:07d}' for i in range(4)])  # the hubs
        abouts = generated.records('abouts')
        self.assertEqual([a['about'] for a in abouts], list(Topology('multi').abouts()))
        writes = generated.records('writes')
        self.assertEqual(len(writes), COMPOSITION.echoes)
        link = writes[0]['arguments']['memories'][0]['connect_to'][0]
        self.assertNotEqual(link['ref'].rsplit(':e', 1)[0], writes[0]['about'])
        self.assertEqual(world('mono').records('writes'), [])

    def test_manifest_and_files(self):
        with tempfile.TemporaryDirectory() as folder:
            record = synth.write_world(world('mono'), folder)
            for section in synth.SECTIONS:
                data = (Path(folder) / f'{section}.jsonl').read_bytes()
                self.assertEqual(record['files'][f'{section}.jsonl']['bytes'], len(data))
            manifest = json.loads((Path(folder) / 'manifest.json').read_text(encoding='utf-8'))
        self.assertEqual(manifest['schema'], 'kmp.bench.world.v1')
        self.assertEqual(manifest['world_digest'], world('mono').level_digests['1000'])
        self.assertEqual(manifest['world_key'], synth.WorldParams(7, 'mono', (1000,)).world_key())
        self.assertEqual(manifest['level_counts']['1000']['entries'], 1000)

    def test_composition_has_every_episode(self):
        episodes = collections.Counter(e['truth']['episode'] for e in world('multi').records('entries'))
        for name in ('family', 'chain', 'supersession-declared', 'supersession-valid_until',
                     'supersession-date_only', 'hub', 'hub_note', 'distractor', 'rare',
                     'paraphrase', 'crosslang', 'echo', 'filler'):
            self.assertIn(name, episodes)
        chains = collections.Counter()
        for entry in world('mono').records('entries'):
            if entry['truth']['chain']:
                chains[entry['truth']['chain']['id']] = entry['truth']['chain']['length']
        self.assertEqual(sorted(set(chains.values())), [2, 3, 4, 5, 6])

    def test_spanish_entries_exist(self):
        langs = collections.Counter(e['truth']['lang'] for e in world('mono').records('entries'))
        self.assertGreater(langs['es'], 0)

    def test_hub_degrees_grow_with_the_level(self):
        relations = world('mono', (1000, 10000)).records('relations')
        degree = collections.Counter(r['to'] for r in relations if r['to'].endswith(':e0000000'))
        self.assertEqual(degree['synth:mono-a000:e0000000'], hub_degree(0, 10))
        self.assertGreater(hub_degree(0, 100), 500)


class QuestionTest(unittest.TestCase):
    def test_question_types_and_rules(self):
        questions = synth.load_questions(world('multi'))
        kinds = collections.Counter(q.type for q in questions)
        for kind in ('enumerative_anchored', 'singular_anchored', 'anchor_neighbor_existing',
                     'anchor_absent', 'near_miss_attribute', 'negated_anchor',
                     'singular_anchored_twin', 'cross_about_anchor', 'lookup_exact',
                     'rare_identifier', 'paraphrase_zero_overlap', 'crosslang_es_en',
                     'crosslang_en_es', 'hard_distractor', 'multihop_why_k', 'path_between',
                     'path_open', 'current_after_supersession', 'as_of_historical',
                     'interval_scoped', 'hub_adjacent', 'multi_about', 'wake_resume',
                     'repeat_and_determinism'):
            self.assertGreaterEqual(kinds[kind], 4, kind)
        self.assertTrue(all(q.min_level == 1000 and q.corpus == 'synth-v1' for q in questions))
        mono = collections.Counter(q.type for q in synth.load_questions(world('mono')))
        self.assertNotIn('cross_about_anchor', mono)
        self.assertNotIn('multi_about', mono)

    def test_enumerative_questions_ask_eight_to_twelve_facets(self):
        for q in synth.load_questions(world('mono')):
            if q.type == 'enumerative_anchored':
                self.assertTrue(8 <= len(q.gold.facets) <= 12)
                self.assertTrue(any(f.answers for f in q.gold.facets))
                self.assertTrue(any(not f.answerable_facet for f in q.gold.facets))

    def test_chains_cover_two_to_five_refs_and_paths_two_to_five_hops(self):
        questions = synth.load_questions(world('mono', (1000, 10000)))
        ks = {len(q.gold.chain.refs) for q in questions if q.type == 'multihop_why_k'}
        hops = {len(q.gold.path.steps) - 1 for q in questions if q.type.startswith('path_')}
        self.assertEqual(ks, {2, 3, 4, 5})
        self.assertEqual(hops, {2, 3, 4, 5})
        undeclared = [q for q in questions if q.type == 'path_between' and q.source['undeclared_edges']]
        self.assertTrue(undeclared, 'some paths must need a proposed hop')

    def test_question_blocks_nest_by_min_level(self):
        questions = synth.load_questions(world('mono', (1000, 10000)))
        levels = collections.Counter(q.min_level for q in questions)
        self.assertEqual(set(levels), {(k + 1) * 1000 for k in QUESTION_BLOCKS if k < 10})


class InvariantTest(unittest.TestCase):
    def test_seed_7_passes_every_invariant(self):
        probe = SearchProbe.locate() if DEFAULT_PATH.is_file() else None
        for topology in ('mono', 'multi'):
            with self.subTest(topology=topology):
                generated = world(topology, (1000, 10000))
                report = invariants.check_world(
                    generated, synth.load_questions(generated), probe,
                    lambda level, t=topology: world(t).level_digests[str(level)])
                self.assertTrue(report['ok'], report['failures'])
                self.assertIn('nested_ladder', report['checked'])

    def _mutated(self, section, edit):
        generated = world('mono')
        copy = synth.World(generated.params, generated.world_key,
                           {name: list(lines) for name, lines in generated.lines.items()},
                           generated.blocks_of)
        copy.lines[section] = [json.dumps(edit(json.loads(line)) or json.loads(line))
                               for line in copy.lines[section]]
        return copy

    def test_an_absent_anchor_that_appears_is_caught(self):
        questions = synth.load_questions(world('mono'))
        absent = next(q for q in questions if q.type == 'anchor_absent').anchors[0]

        def plant(entry):
            if entry['ordinal'] == 500:
                entry['text'] += f' See {absent} for details.'
                return entry
        report = invariants.check_world(self._mutated('entries', plant), questions)
        self.assertIn('unknown_has_no_fact', report['failures'])

    def test_a_broken_valid_until_is_caught(self):
        def stretch(entry):
            coordinate = entry['coordinates'][0]
            if 'valid_until' in coordinate:
                coordinate['valid_until'] = coordinate['valid_from']
                return entry
        questions = synth.load_questions(world('mono'))
        report = invariants.check_world(self._mutated('entries', stretch), questions)
        self.assertIn('valid_until_coherent', report['failures'])

    def test_a_dropped_declared_hop_is_caught(self):
        questions = synth.load_questions(world('mono'))
        path = next(q for q in questions if q.type == 'path_between'
                    and len(q.source['undeclared_edges'].split(',')) < len(q.gold.path.steps) - 1)
        undeclared = set(path.source['undeclared_edges'].split(','))
        a, b = next((a, b) for a, b in zip(path.gold.path.steps, path.gold.path.steps[1:])
                    if f'{a}>{b}' not in undeclared)

        def drop(relation):
            if {relation['from'], relation['to']} == {a, b}:
                relation['to'] = relation['from']
                return relation
        report = invariants.check_world(self._mutated('relations', drop), questions)
        self.assertIn('declared_chains_exist', report['failures'])

    def test_zero_overlap_tables_hold_under_every_morphology(self):
        probe = probe_or_skip(self)
        subjects, predicates = render.paraphrase_table()
        stored = [row['stored'].replace('{V}', '') for row in subjects + predicates]
        asked = [row['asked'].replace('{S}', '') for row in subjects + predicates]
        lex_subjects, facts = render.lexicon_table()
        for morphology in ('english', 'spanish', 'none'):
            keys = probe.search_keys(stored + asked, morphology)
            self.assertFalse(frozenset().union(*keys[:len(stored)]) & frozenset().union(*keys[len(stored):]))
            for stored_lang, asked_lang in (('en', 'es'), ('es', 'en')):
                left = [row[stored_lang].replace('{V}', '') for row in lex_subjects + facts]
                right = [row[asked_lang] for row in lex_subjects] + \
                    [row[f'{asked_lang}_question'].replace('{S}', '') for row in facts]
                keys = probe.search_keys(left + right, morphology)
                self.assertFalse(frozenset().union(*keys[:len(left)]) & frozenset().union(*keys[len(left):]),
                                 (morphology, stored_lang))


class RenderRuleTest(unittest.TestCase):
    def test_filler_never_carries_a_facet_keyword_or_an_anchor(self):
        rng = SplitMix64(99)
        for _ in range(4000):
            sentence = render.filler_sentence(rng, 5000).lower()
            for keyword in render.FACET_KEYWORDS:
                self.assertIsNone(re.search(rf'\b{keyword}', sentence), (keyword, sentence))
            self.assertIsNone(ANCHOR_SHAPE.search(sentence), sentence)
            for number in re.findall(r'(?<![\w-])\d+(?![\w:-])', sentence):
                self.assertLess(int(number), 10000)

    def test_facet_sentences_carry_only_their_own_keyword(self):
        for facet in render.FACETS:
            for value in facet.values:
                text = render.facet_sentence(facet, 'C1.2', 'svc-001', value).lower()
                self.assertRegex(text, rf'\b{facet.keyword}')
                for other in render.FACETS:
                    if other is not facet:
                        self.assertIsNone(re.search(rf'\b{other.keyword}', text), (facet.name, other.name))

    def test_perturbed_anchors_are_never_generated(self):
        generated = {render.anchor(style, g, f) for style in render.ANCHOR_STYLES
                     for g in range(3000) for f in range(3)}
        for style in render.ANCHOR_STYLES:
            for g in range(3000):
                for f in range(3):
                    self.assertNotIn(render.absent_anchor(style, g, f), generated)
        self.assertEqual(render.absent_anchor('corte', 5, 1), 'C6.24')

    def test_anchors_live_only_in_family_entries(self):
        for entry in world('mono').records('entries'):
            if ANCHOR_SHAPE.search(entry['text']):
                self.assertEqual(entry['truth']['episode'], 'family', entry['text'][:80])

    def test_zipf_table_is_integer_and_monotone(self):
        table = render._ZIPF_CUMULATIVE
        self.assertEqual(len(table), render.PSEUDO_POOL)
        self.assertTrue(all(isinstance(value, int) for value in table))
        self.assertTrue(all(b > a for a, b in zip(table, table[1:])))
        self.assertLess(table[-1], 2 ** 64)
        self.assertEqual(render._iroot(32 * 10 ** 25, 5), 200000)


class IngestTest(unittest.TestCase):
    def test_batches_follow_the_contract(self):
        generated = world('multi')
        calls = to_ingest.ingest_calls(generated, 1000, 50)
        entries = {e['ref']: e for e in generated.records('entries')}
        seen_abouts, sent, relations = set(), [], 0
        for about, arguments in calls:
            memory = arguments['memory']
            self.assertEqual(arguments['about'], about)
            first = about not in seen_abouts
            seen_abouts.add(about)
            self.assertEqual(bool(memory['dimensions']), first)
            self.assertLessEqual(len(memory['entries']), 50)
            self.assertTrue(arguments['idempotency_key'].startswith(
                f'synth-v1:{generated.world_key[:16]}:{about}:B50:'))
            batch = {entry['id'] for entry in memory['entries']}
            for entry in memory['entries']:
                self.assertEqual(set(entry) - {'metadata'}, {'id', 'kind', 'text', 'coordinates'})
                self.assertTrue(entry['id'].startswith(about + ':'))
                sent.append(entry['id'])
            for relation in memory['relations']:
                later = max((relation['from'], relation['to']), key=lambda r: entries[r]['ordinal'])
                self.assertIn(later, batch)
                relations += 1
        self.assertEqual(sorted(sent), sorted(entries))
        self.assertEqual(relations, len(generated.lines['relations']))
        self.assertEqual(len(to_ingest.write_calls(generated, 1000)), len(generated.lines['writes']))
        self.assertEqual(to_ingest.ingest_calls(generated, 0, 50), [])

    def test_schema_example_shape(self):
        entry = world('mono').records('entries')[10]
        payload = to_ingest.entry_payload(entry)
        self.assertEqual(payload, {'id': entry['ref'], 'kind': entry['kind'], 'text': entry['text'],
                                   'coordinates': entry['coordinates']})


class CalibrationTest(unittest.TestCase):
    def test_ks_helpers(self):
        self.assertEqual(calibration.ks_against_percentiles(list(range(101)), list(range(101))), 0.0099)
        self.assertEqual(calibration.ks_against_ecdf([1, 1, 2, 4], ((1, 0.5), (2, 0.75), (4, 1.0))), 0.0)
        self.assertIsNone(calibration.ks_against_ecdf([], ((1, 0.5),)))

    def test_graph_profile(self):
        profile = calibration.graph_profile(['a', 'b', 'c', 'd'], [('a', 'b', 'follows'), ('x', 'a', 'answers')])
        self.assertEqual((profile['relations'], profile['isolated_fraction'], profile['max_degree']),
                         (1, 0.5, 1))

    def test_reference_is_aggregate_only(self):
        text = json.dumps(calibration.REFERENCE)
        self.assertNotIn('project:', text)
        self.assertEqual(len(calibration.REFERENCE['length_percentiles']), 101)

    def test_world_calibration_is_close_to_the_store(self):
        probe = probe_or_skip(self)
        report = calibration.world_calibration(world('multi'), probe)
        self.assertLess(report['ks_length'], 0.1)
        self.assertLess(report['ks_df'], 0.12)
        for name in ('identifier_density', 'anchor_chain_fraction', 'isolated_fraction',
                     'relations_per_entry'):
            pair = report[name]
            self.assertLess(abs(pair['synth'] - pair['real']) / pair['real'], 0.2, name)


class BridgeTest(unittest.TestCase):
    def test_shipped_table_and_fold(self):
        table = bridge.BridgeTable.load()
        self.assertAlmostEqual(table.similarity('valvula', 'valve'), 0.5069, places=3)
        self.assertIsNone(table.similarity('zzzqqq', 'valve'))
        self.assertEqual(bridge.fold('¿Dónde perdió el equipo la llave?'), ['perdio', 'equipo', 'llave'])
        state, bridged = bridge.coverage(table, 'Where did the payments team move the spare keys?',
                                         'El equipo de pagos trasladó sus llaves de repuesto a Oslo.')
        self.assertEqual(state, 'covered')
        self.assertIn('payments', bridged)


class CliTest(unittest.TestCase):
    def test_world_command_writes_a_checked_world(self):
        with tempfile.TemporaryDirectory() as folder:
            out, err = io.StringIO(), io.StringIO()
            flags = [] if DEFAULT_PATH.is_file() else ['--no-probe']
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = cli.main(['world', '--seed', '7', '--levels', '1e3', '--topology', 'mono',
                                 '--out', folder, *flags])
            self.assertEqual(code, 0, err.getvalue())
            summary = json.loads(out.getvalue())['mono']
            manifest = json.loads((Path(summary['directory']) / 'manifest.json').read_text())
            self.assertTrue(manifest['invariants']['ok'])
            self.assertEqual(manifest['world_digest'], world('mono').level_digests['1000'])

    def test_missing_probe_is_refused_unless_skipped(self):
        args = cli.build_parser().parse_args(['world', '--seed', '7', '--levels', '1e3',
                                              '--probe', '/nonexistent/probe'])
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            self.assertEqual(world_app.run_world(args), 2)
        self.assertIn('not found', err.getvalue())


if __name__ == '__main__':
    unittest.main()
