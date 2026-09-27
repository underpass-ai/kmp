"""P11 far trace cut: pairs from the declared graph, trace_far questions, the scale mode and its report row."""
import dataclasses
import json
from types import SimpleNamespace
import unittest

from ..application import far_trace, markdown, modes as modes_module, synth_section
from ..application.score import _score_path
from ..domain.errors import QuestionInvalid
from ..domain.far_trace import (FarTraceInvalid, FarTraceSpec, declared_graph, distances, far_pairs,
                                histogram)
from ..domain.question import Question

A = 'synth:mono-a000'
B = 'synth:mono-a001'


def ref(about, n):
    return f'{about}:e{n:07d}'


def relation(start, end, block=0):
    return {'from': start, 'to': end, 'about': start.rsplit(':', 1)[0], 'block': block}


# A: 0>1>2>3 plus a diamond 0>4>5>3 (two shortest 3-hop routes 0..3) and 3>6 (0..6 is 4 hops).
# B: 0>1 (one hop only). A cross-about link and a self-loop are never hops.
RELATIONS = [relation(ref(A, 0), ref(A, 1)), relation(ref(A, 1), ref(A, 2)), relation(ref(A, 2), ref(A, 3)),
             relation(ref(A, 0), ref(A, 4)), relation(ref(A, 4), ref(A, 5)), relation(ref(A, 5), ref(A, 3)),
             relation(ref(A, 3), ref(A, 6)), relation(ref(B, 0), ref(B, 1)),
             relation(ref(A, 2), ref(B, 1)), relation(ref(A, 1), ref(A, 1))]
NODES = {ref(A, n) for n in range(7)} | {ref(B, n) for n in range(2)}


class DeclaredGraphTest(unittest.TestCase):
    def test_the_graph_is_directed_same_about_and_limited_to_the_rung(self):
        graph = declared_graph(RELATIONS, NODES)
        self.assertEqual(sorted(graph), [A, B])
        self.assertEqual(graph[A][ref(A, 1)], {ref(A, 2)})
        self.assertNotIn(ref(A, 0), distances(graph[A], ref(A, 3)))
        self.assertNotIn(ref(B, 1), graph[A][ref(A, 2)])
        self.assertNotIn(ref(A, 6), declared_graph(RELATIONS, NODES - {ref(A, 6)})[A])
        self.assertEqual(distances(graph[A], ref(A, 0))[ref(A, 6)], 4)

    def test_the_histogram_counts_ordered_reachable_pairs_by_hops(self):
        counts = histogram(declared_graph(RELATIONS, NODES))
        self.assertEqual(sum(counts.values()), 17 + 1)
        self.assertEqual(counts[4], 1)  # 0..6
        self.assertEqual(max(counts), 4)

    def test_far_pairs_keep_only_routes_of_at_least_k_hops_with_every_shortest_edge(self):
        graph = declared_graph(RELATIONS, NODES)
        pairs = far_pairs(graph, FarTraceSpec(3, 10), seed=7)
        self.assertTrue(pairs and all(p.hops >= 3 for p in pairs))
        self.assertTrue(all(p.about == A for p in pairs))
        self.assertEqual(sorted((p.start[-1], p.end[-1]) for p in pairs),
                         [('0', '3'), ('0', '6'), ('1', '6'), ('4', '6')])
        diamond = next(p for p in pairs if (p.start, p.end) == (ref(A, 0), ref(A, 3)))
        self.assertEqual(diamond.steps, (ref(A, 0), ref(A, 1), ref(A, 2), ref(A, 3)))
        self.assertEqual(len(diamond.accept_edges), 6)  # both 3-hop routes, never 3-6
        self.assertEqual(far_pairs(graph, FarTraceSpec(4, 10), seed=7)[0].hops, 4)
        self.assertEqual(far_pairs(graph, FarTraceSpec(5, 10), seed=7), ())
        self.assertEqual(far_pairs(graph, FarTraceSpec(3, 'all'), seed=7), pairs)

    def test_the_choice_is_fixed_by_content_and_seed(self):
        graph = declared_graph(RELATIONS, NODES)
        one = far_pairs(graph, FarTraceSpec(3, 2), seed=7)
        self.assertEqual(one, far_pairs(declared_graph(list(reversed(RELATIONS)), NODES), FarTraceSpec(3, 2), 7))
        self.assertEqual(one, far_pairs(graph, FarTraceSpec(3, 10), seed=7)[:2])
        self.assertEqual(len(one), 2)

    def test_the_spec_needs_k_of_two_or_more_and_some_pairs(self):
        for bad in ((1, 4), (3, 0), (True, 4), (3, '4'), (3, 'every')):
            with self.assertRaises(FarTraceInvalid):
                FarTraceSpec(*bad)


class Loaded:
    """The two sections of a World that far_trace reads (lines + blocks_of), two blocks of 4 refs."""

    def __init__(self):
        entries = [{'ref': ref(A, n), 'block': n // 4} for n in range(8)]
        relations = [relation(ref(A, 0), ref(A, 1)), relation(ref(A, 1), ref(A, 2)),
                     relation(ref(A, 2), ref(A, 3)), relation(ref(A, 0), ref(A, 3), block=1)]
        self.params = SimpleNamespace(block_size=4)
        self.lines = {'entries': [json.dumps(e) for e in entries], 'relations': [json.dumps(r) for r in relations]}
        self.blocks_of = {'entries': [e['block'] for e in entries], 'relations': [r['block'] for r in relations]}


def ladder(**changes):
    base = modes_module.SynthLadder(7, ('mono',), (4, 8), 5000, (), 0, (), FarTraceSpec(3, 4))
    return dataclasses.replace(base, **changes) if changes else base


class TraceFarQuestionTest(unittest.TestCase):
    def test_questions_use_the_lowest_rung_graph_and_validate_as_trace_far(self):
        questions = far_trace.questions(Loaded(), ladder(), 'mono')
        self.assertEqual(len(questions), 1)  # the block-1 shortcut 0-3 is not on the 4-entry rung
        question = questions[0]
        self.assertEqual((question.id, question.type, question.tool, question.min_level),
                         ('synth7-mono-far-01', 'trace_far', 'kmp_trace', 4))
        self.assertEqual(question.first_call(), ('kmp_trace', {'from': ref(A, 0), 'to': ref(A, 3), 'about': A}))
        self.assertEqual(question.tags, ('far_min_hops:3', 'hops:3'))
        self.assertEqual(question.source, {'kind': 'bench', 'rule': 'trace_far.v1', 'hops': 3})
        self.assertEqual(Question.from_dict(question.as_dict()), question)
        self.assertEqual(far_trace.questions(Loaded(), ladder(far_trace=None), 'mono'), ())

    def test_trace_far_is_a_trace_with_a_destination(self):
        record = far_trace.questions(Loaded(), ladder(), 'mono')[0].as_dict()
        with self.assertRaisesRegex(QuestionInvalid, 'kmp_trace'):
            Question.from_dict({**record, 'tool': 'kmp_curate', 'arguments': {'mode': 'paths', 'from': ref(A, 0)}})
        open_gold = {**record['gold'], 'path': {**record['gold']['path'], 'to': None}}
        with self.assertRaisesRegex(QuestionInvalid, 'destination'):
            Question.from_dict({**record, 'gold': open_gold, 'arguments': {'from': ref(A, 0), 'to': ref(A, 3)}})

    def test_any_shortest_declared_route_is_found_and_a_short_cut_is_not(self):
        question = far_trace.questions(Loaded(), ladder(), 'mono')[0]

        def page(*hops):
            return {'trace': [{'from': a, 'to': b} for a, b in hops]}
        route = [(ref(A, 0), ref(A, 1)), (ref(A, 1), ref(A, 2)), (ref(A, 2), ref(A, 3))]
        self.assertTrue(_score_path(question, [page(*route)])[0]['path_found'])
        self.assertFalse(_score_path(question, [page((ref(A, 0), ref(A, 3)))])[0]['path_found'])
        backwards = [(b, a) for a, b in route]
        self.assertFalse(_score_path(question, [page(*backwards)])[0]['path_found'])
        self.assertFalse(_score_path(question, [{'trace': []}])[0]['path_found'])


class ScaleModeTest(unittest.TestCase):
    def test_the_scale_mode_pins_the_verification_parameters(self):
        modes = modes_module.load_modes()
        scale = modes.get('scale')
        self.assertEqual((scale.timeout_s, scale.sections, scale.synth.levels, scale.synth.per_type),
                         (120.0, ('synth',), (1000, 10000, 100000), 2))
        self.assertEqual(scale.synth.exclude_types, ('wake_resume',))
        self.assertEqual(scale.synth.far_trace, FarTraceSpec(3, 'all'))
        self.assertEqual((modes.get('scale-aa').replica_of, modes.get('scale-aa').synth), ('scale', scale.synth))
        self.assertIsNone(modes.get('full').synth.far_trace)
        self.assertEqual(modes.get('quick-a').synth.exclude_types, ())

    def test_invalid_ladder_options_are_refused(self):
        text = modes_module.MODES_PATH.read_text()
        for old, new in (('exclude_types = ["wake_resume"]', 'exclude_types = ["no_such_type"]'),
                         ('exclude_types = ["wake_resume"]', 'exclude_types = ["wake_resume", "wake_resume"]'),
                         ('min_hops = 3\n', 'min_hops = 1\n'), ('pairs = "all"', 'pairs = 0'), ('pairs = "all"', 'pairs = "some"'),
                         ('pairs = "all"', 'pairs = "all"\nhops = 3')):
            with self.subTest(new=new), self.assertRaises(modes_module.ModeInvalid):
                modes_module.parse_modes(text.replace(old, new, 1))

    def test_excluded_types_are_never_selected(self):
        questions = [SimpleNamespace(id=f'q{i}', type=kind, min_level=4)
                     for i, kind in enumerate(('wake_resume', 'lookup_exact', 'wake_resume'))]
        chosen = synth_section.select(questions, ladder(exclude_types=('wake_resume',)))
        self.assertEqual([q.id for q in chosen], ['q1'])
        self.assertEqual(len(synth_section.select(questions, ladder())), 3)


def call(question_id, wall_ms, status='ok', repeat=0, censored=False, tool='kmp_trace'):
    return SimpleNamespace(question_id=question_id, repeat=repeat, tool=tool, status=status,
                           wall_ns=None if wall_ms is None else int(wall_ms * 1e6), censored=censored)


def score(question_id, found, kind='trace_far'):
    values = {} if found is None else {'path_found': found}
    return SimpleNamespace(question_id=question_id, type=kind, values=values,
                           tags=('far_min_hops:3', 'hops:3') if kind == 'trace_far' else ())


def item(level, calls):
    return SimpleNamespace(level=level, topology='mono', run=SimpleNamespace(run_id=f'r{level}', calls=calls))


class FarTraceRowTest(unittest.TestCase):
    def test_a_row_per_rung_with_found_rate_and_uncensored_wall_quantiles(self):
        calls = [call('f1', 10), call('f2', 20), call('f3', 40), call('f4', None, status='timeout', censored=True),
                 call('f1', 999, repeat=1), call('q1', 999, tool='kmp_ask')]
        scores = [score('f1', True), score('f2', True), score('f3', False), score('f4', None),
                  score('q1', None, kind='lookup_exact')]
        row = far_trace.row('baseline', item(100000, calls), scores)
        self.assertEqual((row['pairs'], row['min_hops'], row['hops'], row['calls'], row['censored']),
                         (4, 3, [3], 4, 1))
        self.assertEqual(row['path_found'], {'value': 2 / 3, 'n': 3})
        self.assertEqual((row['wall_ms_p50'], round(row['wall_ms_p95'], 3)), (20.0, 38.0))
        self.assertEqual(row['target'], {'statistic': 'wall_ms_p95', 'max_ms': 30.0, 'level': 100000})
        self.assertIs(row['within_target'], False)
        self.assertIsNone(far_trace.row('baseline', item(1000, calls), scores)['within_target'])
        fast = far_trace.row('baseline', item(100000, calls[:2]), scores[:2])
        self.assertIs(fast['within_target'], True)
        self.assertIsNone(far_trace.row('baseline', item(1000, calls), scores[-1:]))

    def test_rows_walk_every_arm_and_run_and_render_in_the_scale_section(self):
        runs = (item(1000, [call('f1', 3)]), item(100000, [call('f1', 25)]))
        state = SimpleNamespace(arm=SimpleNamespace(name='baseline', runs=runs),
                                scores={'r1000': (score('f1', True),), 'r100000': (score('f1', True),)})
        rows = far_trace.rows([state])
        self.assertEqual([r['level'] for r in rows], [1000, 100000])
        text = '\n'.join(markdown._scale({'exponents': [], 'calibration': None, 'far_trace': rows}))
        self.assertIn('### Far trace', text)
        self.assertIn('wall_ms_p95 <= 30 ms at 100000', text)
        self.assertIn('| baseline | mono | 100000 | 1 | 3 | 1.000 | 1 | 0 | 25.00 | 25.00 | yes |', text)
        self.assertNotIn('Far trace', '\n'.join(markdown._scale({'exponents': [], 'calibration': None,
                                                                   'far_trace': []})))


if __name__ == '__main__':
    unittest.main()
