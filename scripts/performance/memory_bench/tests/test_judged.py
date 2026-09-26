"""BT16: the repository's judged corpora over stdio, and the Rust scorecards read from stdout.

The unit tests drive the corpora with fake stores (scripted answers, every call
recorded), so they check the reading, the arithmetic and the exact calls
without a binary. The live tests (MEMORY_BENCH_SLOW=1 and a release binary)
reproduce the recorded baselines through the release binary over MCP stdio.
"""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from types import SimpleNamespace

from ..corpora import external_scorecards as ext
from ..corpora import judged_jev as jj
from ..corpora import judged_retrieval as jr
from ..corpora import judged_stdio as js
from ..domain.jsonl import REPO_ROOT

ROOT = Path(REPO_ROOT)
SLOW = os.environ.get('MEMORY_BENCH_SLOW') == '1'
BINARY = Path(os.environ.get('MEMORY_BENCH_BINARY') or ROOT / 'target/release/kmp-mcp')
ACCEPTED = {'status': 'committed', 'accepted': True}


class FakeServer:
    def __init__(self, label, files, answer, log):
        self.label, self.files, self.answer, self.log = label, files, answer, log

    def call(self, tool, arguments):
        self.log.append((self.label, tool, arguments))
        return self.answer(self.label, tool, arguments)


class FakeStores:
    """`open(label, files)` like StdioStores; `answer(label, tool, arguments)` scripts replies."""

    def __init__(self, answer):
        self.answer, self.calls, self.opened = answer, [], []

    @contextmanager
    def open(self, label, store_files=()):
        self.opened.append((label, tuple((f.name, f.content().decode()) for f in store_files)))
        yield FakeServer(label, store_files, self.answer, self.calls)


def ask_reply(answer='x', evidence=(), because=(), confidence='high', **extra):
    reply = {'answer': answer, 'because': [{'ref': ref} for ref in because],
             'proof': {'evidence': [{'id': ref} for ref in evidence], 'confidence': confidence},
             'projection': {'budget': {'used_bytes': 100}}}
    reply.update(extra)
    return reply


def case(**overrides):
    data = {'id': 'c1', 'probes': 'p', 'about': 'project:a', 'question': 'q?',
            'answer_policy': 'evidence_or_unknown', 'judged': ['project:a:e2'],
            'memory': {'entries': []}}
    data.update(overrides)
    return jr.RetrievalCase.from_dict(data)


class RetrievalCollectionTest(unittest.TestCase):
    def test_the_repository_collection_has_35_originals_42_positives_13_guarded(self):
        cases = jr.load_cases(ROOT / jr.DEFAULT_CASES)
        self.assertEqual(53, len(cases))
        self.assertEqual(35, sum(c.original for c in cases))
        self.assertEqual(42, sum(bool(c.judged) for c in cases))
        self.assertEqual(13, sum(c.guarded for c in cases))
        self.assertFalse(any(c.guarded for c in cases if c.original))

    def test_ask_arguments_are_the_scorecards(self):
        plain = case().ask_arguments()
        self.assertEqual({'about': 'project:a', 'question': 'q?', 'answer_policy': 'evidence_or_unknown',
                          'depth': 3, 'budget': {'tokens': 2048, 'detail': 'balanced',
                                                 'max_entries': 10}}, plain)
        timed = case(interval={'start': 'a', 'end': 'b'}, axis='occurred',
                     dimensions={'scope': 'abouts'}).ask_arguments(5)
        self.assertEqual(5, timed['budget']['max_entries'])
        self.assertEqual(('interval', 'axis', 'dimensions'), tuple(timed)[5:])

    def test_a_malformed_case_or_duplicate_id_is_refused(self):
        with self.assertRaises(jr.JudgedCaseInvalid):
            jr.RetrievalCase.from_dict({'id': 'x'})
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'cases.json'
            body = {'id': 'x', 'probes': '', 'about': 'a', 'question': 'q', 'answer_policy': 'p',
                    'judged': [], 'memory': {}}
            path.write_text(json.dumps({'cases': [body, body]}))
            with self.assertRaises(jr.JudgedCaseInvalid):
                jr.load_cases(path)


class VerdictTest(unittest.TestCase):
    """retrieval_scorecard.rs `a_response_is_read_as_unknown_partial_or_answer`."""

    def test_unknown_partial_or_answer(self):
        absent = ('Kubernetes',)
        self.assertEqual(jr.AskVerdict('UNKNOWN'), jr.AskVerdict.read({'answer': 'UNKNOWN'}, absent))
        partial = {'answer': 'PARTIAL', 'proof': {'missing': ['kubernetes cluster']}}
        self.assertEqual(jr.AskVerdict('PARTIAL', True), jr.AskVerdict.read(partial, absent))
        vague = {'answer': 'PARTIAL', 'proof': {'missing': ['deployment']}}
        self.assertFalse(jr.AskVerdict.read(vague, absent).abstains())
        self.assertEqual(jr.AskVerdict('ANSWER'), jr.AskVerdict.read({'answer': 'Staging.'}, absent))
        gated = {'answer': 'entry:x', 'answer_status': 'partial', 'proof': {'missing': ['kubernetes']}}
        self.assertEqual(jr.AskVerdict('PARTIAL', True), jr.AskVerdict.read(gated, absent))
        self.assertEqual(jr.AskVerdict('UNKNOWN'),
                         jr.AskVerdict.read({'answer': 'UNKNOWN', 'answer_status': 'unknown'}, absent))
        self.assertTrue(jr.AskVerdict('PARTIAL', True).abstains())

    def test_guarded_scorecard_by_kind(self):
        answer, unknown = jr.AskVerdict('ANSWER'), jr.AskVerdict('UNKNOWN')
        decisions = [jr.GuardedDecision('b_kind', True, answer, False, True),
                     jr.GuardedDecision('b_kind', True, unknown, False, True),
                     jr.GuardedDecision('a_kind', False, answer, True, False)]
        card = jr.GuardedScorecard.score(decisions)
        self.assertEqual(3, card.cases)
        self.assertAlmostEqual(2 / 3, card.false_answer_rate)
        self.assertAlmostEqual(1 / 3, card.high_false_answer_rate)
        self.assertEqual((('a_kind', 1.0), ('b_kind', 0.5)), card.kind_rates())
        self.assertEqual(0.0, jr.GuardedScorecard.score(()).false_answer_rate)


class ReadAnswerTest(unittest.TestCase):
    def test_evidence_citations_and_bytes_to_judged(self):
        result = jr.read_answer(case(), ask_reply(
            evidence=('entry:project:a:e1', 'detail:project:a:e2', 'project:a:e3'),
            because=('entry:project:a:e2',)))
        self.assertEqual(('project:a:e1', 'project:a:e2', 'project:a:e3'), result.outcome.retrieved)
        self.assertEqual(frozenset({'project:a:e2'}), result.outcome.cited)
        self.assertEqual(0.5, result.outcome.reciprocal_rank())
        self.assertEqual(100, result.outcome.used_bytes)
        expected = sum(len(js.compact({'id': ref}).encode()) for ref in
                       ('entry:project:a:e1', 'detail:project:a:e2'))
        self.assertEqual(expected, result.to_judged)
        self.assertEqual('high', result.confidence)
        self.assertIsNone(result.decision())

    def test_nearest_outside_scores_only_an_honest_unknown(self):
        nearest = case(judged=[], nearest_outside='project:a:e9')
        unknown = ask_reply('UNKNOWN')
        unknown['proof']['nearest_outside'] = {'ref': 'project:a:e9'}
        result = jr.read_answer(nearest, unknown)
        self.assertEqual(('project:a:e9',), result.outcome.retrieved)
        self.assertFalse(result.outcome.unknown)
        self.assertEqual(1.0, result.outcome.recall_at(1))
        answered = ask_reply('x')
        answered['proof']['nearest_outside'] = {'ref': 'project:a:e9'}
        self.assertEqual(0.0, jr.read_answer(nearest, answered).outcome.recall_at(1))

    def test_a_negative_answered_or_a_forbidden_citation_is_a_false_answer(self):
        negative = case(judged=[], kind='anchor_absent', absent=['C6.24'])
        self.assertTrue(jr.read_answer(negative, ask_reply('x')).decision().is_false_answer())
        self.assertFalse(jr.read_answer(negative, ask_reply('UNKNOWN')).decision().is_false_answer())
        negated = case(kind='negated_anchor', forbidden=['project:a:e3'])
        cited = jr.read_answer(negated, ask_reply(because=('project:a:e3',)))
        self.assertTrue(cited.decision().cited_forbidden)
        self.assertEqual('negated_anchor', cited.decision().kind)
        self.assertEqual('none', jr.read_answer(negated, {'answer': 'x'}).confidence)


class RetrievalRunTest(unittest.TestCase):
    def reviewer(self, label, tool, arguments):
        if tool == 'kmp_write_memory':
            return {'status': 'needs_review',
                    'next_actions': [{'tool': 'kmp_curate', 'arguments': {'mode': 'resolve'}}]}
        if tool == 'kmp_ask':
            return ask_reply(evidence=('project:a:e2',), because=('project:a:e2',))
        return ACCEPTED

    def test_a_case_seeds_writes_through_its_review_and_asks_once(self):
        stores = FakeStores(self.reviewer)
        seeded = case(memories=[{'about': 'project:b', 'memory': {'m': 1}}], writes=[{'w': 1}])
        result = jr.run_case(stores, seeded, clock=iter((1.0, 1.25)).__next__)
        self.assertEqual(250, result.outcome.elapsed_millis)
        self.assertEqual([('retrieval-c1', ())], stores.opened)
        calls = [(tool, arguments) for _, tool, arguments in stores.calls]
        self.assertEqual(('kmp_ingest', {'about': 'project:a', 'idempotency_key': 'judged:c1',
                                         'memory': {'entries': []}}), calls[0])
        self.assertEqual('judged:c1:project:b', calls[1][1]['idempotency_key'])
        self.assertEqual([('kmp_write_memory', {'w': 1}), ('kmp_curate', {'mode': 'resolve'})],
                         calls[2:4])
        self.assertEqual('kmp_ask', calls[4][0])
        self.assertEqual(5, len(calls))

    def test_an_arm_writes_its_store_files_and_refuses_a_rerank_that_did_not_run(self):
        stores = FakeStores(lambda label, tool, arguments: ACCEPTED if tool != 'kmp_ask'
                            else ask_reply(warnings=['rerank unavailable: no cassette']))
        with self.assertRaises(jr.RerankNotApplied):
            jr.run_case(stores, case(), rerank='narrow')
        self.assertEqual((('typesafe.json', jr.TYPESAFE), ('rerank.json', '{"pool_size":40}')),
                         stores.opened[0][1])

    def test_a_rejected_ingest_stops_the_case(self):
        stores = FakeStores(lambda *_: {'status': 'rejected', 'accepted': False})
        with self.assertRaises(js.JudgedCallFailed):
            jr.run_case(stores, case())

    def test_the_collection_rows_are_the_recorded_baselines_rows_in_order(self):
        cases = jr.load_cases(ROOT / jr.DEFAULT_CASES)
        stores = FakeStores(lambda label, tool, arguments: ACCEPTED if tool != 'kmp_ask'
                            else ask_reply('UNKNOWN', confidence='unknown'))
        seen = []
        scored = jr.run_collection(stores, cases, progress=seen.append)
        self.assertEqual(53, len(seen))
        self.assertTrue(scored.gated)
        recorded = jr.parse_baseline((ROOT / jr.DEFAULT_BASELINE).read_text())
        self.assertEqual(list(recorded), [row.name for row in scored.rows])
        values = scored.row_map()
        self.assertEqual((35.0, 42.0, 13.0), (values['cases'], values['all_positives'],
                                              values['guarded_cases']))
        # a nearest_outside case scores its named ref, never UNKNOWN itself
        plain = sum(1 for c in cases if c.original and c.nearest_outside is None)
        self.assertEqual(plain / 35, values['false_unknown_rate'])
        self.assertEqual(0.0, values['guarded_false_answer_rate'])
        self.assertEqual(0.0, scored.mean_bytes_to_judged())  # no proof items came back

    def test_an_arm_runs_only_the_original_cases_and_is_not_gated(self):
        cases = jr.load_cases(ROOT / jr.DEFAULT_CASES)
        stores = FakeStores(lambda label, tool, arguments: ACCEPTED if tool != 'kmp_ask'
                            else ask_reply('UNKNOWN'))
        narrow = jr.run_collection(stores, cases, rerank='narrow')
        self.assertEqual((35, 'narrow', False), (len(narrow.results), narrow.arm, narrow.gated))
        capped = jr.run_collection(FakeStores(stores.answer), cases[:3], max_entries=5)
        self.assertEqual(('off', 3), (capped.arm, len(capped.results)))
        self.assertEqual(5, capped.results[0].case.ask_arguments(5)['budget']['max_entries'])


class BaselineCompareTest(unittest.TestCase):
    def test_parse_and_compare_at_tolerance(self):
        recorded = jr.parse_baseline('# c\nmetric\tfloor\ncases\t35\nrecall_at_1\t0.8428\n')
        self.assertEqual({'cases': 35.0, 'recall_at_1': 0.8428}, recorded)
        checks = jr.compare_to_recorded({'cases': 35.0, 'recall_at_1': 0.842857, 'extra': 1.0},
                                        recorded)
        by = {check.name: check for check in checks}
        self.assertTrue(by['recall_at_1'].within and by['cases'].within)
        self.assertFalse(by['extra'].within)
        self.assertFalse(jr.compare_to_recorded({'recall_at_1': 0.8430}, recorded,
                                                names=('recall_at_1',))[0].within)
        with self.assertRaises(jr.JudgedCaseInvalid):
            jr.parse_baseline('cases 35\n')
        with self.assertRaises(jr.JudgedCaseInvalid):
            jr.parse_baseline('cases\tmany\n')


class StdioHelpersTest(unittest.TestCase):
    def test_receipts_are_read_as_write_receipt_does(self):
        self.assertTrue(js.is_accepted({'status': 'replayed', 'accepted': True}))
        self.assertFalse(js.is_accepted({'status': 'unconfirmed', 'accepted': True}))
        self.assertFalse(js.is_accepted({'status': 'committed', 'accepted': True, 'dry_run': True}))
        self.assertTrue(js.is_accepted({'memory': {'read_after_write_ready': True}}))
        self.assertFalse(js.is_accepted(None))
        with self.assertRaises(js.JudgedCallFailed):
            js.require_accepted({'status': 'needs_review'}, 'here')

    def test_structured_content_and_failures(self):
        self.assertEqual({'a': 1}, js.structured({'result': {'structuredContent': {'a': 1}}}, 't'))
        self.assertEqual({}, js.structured({'result': {}}, 't'))
        with self.assertRaises(js.JudgedCallFailed):
            js.structured({'result': {'isError': True}}, 't')
        with self.assertRaises(js.JudgedCallFailed):
            js.structured({'error': {'code': -1}}, 't')
        tap = SimpleNamespace(call=lambda tool, arguments: {'result': {'structuredContent': arguments}})
        self.assertEqual({'x': 1}, js.JudgedServer(None, tap).call('t', {'x': 1}))

    def test_a_review_without_a_verb_or_a_refused_write_fails(self):
        server = SimpleNamespace(call=lambda tool, arguments: {'status': 'needs_review', 'next_actions': []})
        with self.assertRaises(js.JudgedCallFailed):
            js.commit_with_review(server, {}, 'w')
        refused = SimpleNamespace(call=lambda tool, arguments: {'status': 'rejected', 'accepted': False})
        with self.assertRaises(js.JudgedCallFailed):
            js.commit_with_review(refused, {}, 'w')
        self.assertEqual(ACCEPTED, js.commit_with_review(SimpleNamespace(call=lambda *_: ACCEPTED), {}, 'w'))

    def test_inline_store_files_keep_the_scorecards_bytes(self):
        item = js.inline_store_file('rerank.json', '{"pool_size":40}')
        self.assertEqual(b'{"pool_size":40}', item.content())
        self.assertEqual(64, len(item.sha256))
        self.assertEqual('{"a":"ñ"}', js.compact({'a': 'ñ'}))


# --- Jev ------------------------------------------------------------------------------------------

class JevReadingTest(unittest.TestCase):
    def test_rank_by_support_or_id_suffix(self):
        answer = {'proof': {'evidence': [{'id': 'entry:x:1'}, {'id': 'y', 'supports': ['s:g']},
                                         {'id': 'entry:s:h'}]}}
        self.assertEqual(2, jj.rank(answer, ['s:g']))
        self.assertEqual(3, jj.rank(answer, ['s:h']))
        self.assertIsNone(jj.rank(answer, ['none']))
        self.assertEqual((0.0, 0.5), (jj.reciprocal(None), jj.reciprocal(2)))

    def test_conventions_over_nothing_are_one(self):
        self.assertEqual(1.0, jj.Tally().rate())
        self.assertEqual(1.0, jj.mean([]))
        tally = jj.Tally()
        tally.add(True)
        tally.add(False)
        self.assertEqual(0.5, tally.rate())

    def test_continuation_and_delivered(self):
        page = {'packet': 'p:1', 'proof': {'missing': ['p:2'], 'evidence': ['p:3']},
                'projection': {'next_action': {'tool': 'kmp_wake', 'arguments': {'c': 1}}},
                'page': {}, 'warnings': ['p:4']}
        self.assertEqual(('kmp_wake', {'c': 1}), jj.continuation(page))
        self.assertIsNone(jj.continuation({'next_action': {'tool': 'kmp_wake', 'arguments': {}}}))
        shown = jj.delivered(page)
        self.assertIn('p:1', shown)
        self.assertIn('p:3', shown)
        self.assertNotIn('p:2', shown)
        self.assertNotIn('p:4', shown)
        self.assertIn('missing', json.dumps(page))  # the page itself is untouched

    def test_refuse_unrecorded(self):
        jj.refuse_unrecorded({'warnings': ['all fine']})
        jj.refuse_unrecorded(None)
        with self.assertRaises(jj.JevUnrecorded):
            jj.refuse_unrecorded({'warnings': ['judgement not in the cassette']})

    def test_score_paths(self):
        case_path = {'from': 'a', 'to': 'c', 'required': ['b'], 'edges': [['a', 'b'], ['b', 'c']]}
        declared_bad = [['a', 'wrong', 'x']]

        def hop(source, target, declared=False, rel='r'):
            return {'from': {'ref': source}, 'to': {'ref': target}, 'declared': declared, 'rel': rel}

        scores = jj.JevScores()
        good = {'paths': [{'hops': [hop('a', 'b'), hop('b', 'c')]},
                          {'hops': [hop('x', 'a', True, 'wrong'), hop('a', 'q')]}],
                'avoided': [hop('x', 'a', rel='wrong'), hop('a', 'c')]}
        self.assertTrue(jj.score_paths(case_path, declared_bad, good, 1, scores))
        self.assertEqual((2, 3), (scores['proposed_hops_right'].hits, scores['proposed_hops_right'].total))
        self.assertEqual(0.5, scores['paths_clean_with_jev'].rate())
        self.assertEqual(0.5, scores['avoided_are_bad'].rate())
        short = {'paths': [{'hops': [hop('a', 'b', True)]}]}
        self.assertFalse(jj.score_paths(case_path, declared_bad, short, 0, scores))
        self.assertFalse(jj.score_paths({**case_path, 'to': None}, declared_bad, {'paths': []}, 0, scores))
        self.assertTrue(jj.score_paths({**case_path, 'to': None}, declared_bad, short, 0, scores))
        self.assertEqual(1 / 3, scores['path_found_declared_only'].rate())

    def test_score_review(self):
        found = (jj.Found('i1', 'b', 'a', 'updates_state', 'jev'), jj.Found('i2', 'c', 'd', 'r', 'kernel'),
                 jj.Found('i3', 'e', 'f', None, 'kernel'))
        review = jj.Review('t', found, frozenset({('x', 'r', 'y'), ('g', 'r', 'h')}), frozenset())
        expected = {'missing': [{'pair': ['a', 'b'], 'types': ['updates_state']},
                                {'pair': ['c', 'd'], 'types': ['other']}, {'pair': ['z', 'w'], 'types': []}],
                    'distractors': [{'pair': ['e', 'f']}, {'pair': ['d', 'c']}],
                    'declared_bad': [['x', 'r', 'y'], ['m', 'r', 'n']],
                    'declared_good': [['g', 'r', 'h']]}
        scores = jj.JevScores()
        jj.score_review(expected, review, scores)
        rates = dict(scores.columns())
        self.assertAlmostEqual(2 / 3, rates['missing_found'])
        self.assertEqual(0.5, rates['missing_typed'])
        self.assertAlmostEqual(1 / 3, rates['missing_found_by_jev'])
        self.assertEqual(0.5, rates['distractors_rejected'])
        self.assertEqual((0.5, 0.5, 0.0), (rates['suspect_recall'], rates['suspect_precision'],
                                           rates['good_declarations_kept']))

    def test_review_pages_read_missing_and_suspect(self):
        found, flagged, no_direction = [], set(), set()
        page = {'missing': [{'item_id': 'i', 'from': {'ref': 'a'}, 'to': {'ref': 'b'},
                             'suggested_rel': 'r', 'proposed_by': 'jev'}],
                'suspect': [{'from': {'ref': 'a'}, 'rel': 'r', 'to': {'ref': 'b'}, 'reasons': ['direction']},
                            {'from': {'ref': 'c'}, 'rel': 'r', 'to': {'ref': 'd'}, 'reasons': ['type']}]}
        jj.read_review_page(page, found, flagged, no_direction)
        self.assertEqual([jj.Found('i', 'a', 'b', 'r', 'jev')], found)
        self.assertEqual({('a', 'r', 'b'), ('c', 'r', 'd')}, flagged)
        self.assertEqual({('c', 'r', 'd')}, no_direction)


class JevRunTest(unittest.TestCase):
    CASE = {'id': 'k', 'probes': '', 'about': 's:a', 'abouts': ['s:a'],
            'memories': [{'about': 's:a', 'memory': {}}],
            'expected': {'missing': [{'pair': ['s:a:1', 's:a:2'], 'types': ['r'], 'kind': 'k'}],
                         'distractors': [], 'declared_good': [], 'declared_bad': [],
                         'precheck': [{'pair': ['s:a:1', 's:a:2'], 'from': 's:a:2', 'rel': 'r',
                                       'why': 'w', 'evidence': 'e', 'doubt': False},
                                      {'pair': ['s:a:8', 's:a:9'], 'from': 's:a:9', 'rel': 'r',
                                       'why': 'w', 'evidence': 'e', 'doubt': True}],
                         'ask': [{'question': 'q', 'gold': ['s:a:2'], 'shape': 'x'}],
                         'paths': [{'from': 's:a:1', 'to': 's:a:2', 'required': [],
                                    'edges': [['s:a:1', 's:a:2']]}],
                         'wake': [{'intent': 'go', 'required': ['s:a:1', 's:a:2']}]}}

    def answer(self, label, tool, arguments):
        arm = label.rsplit('-', 1)[1]
        if tool == 'kmp_ingest':
            return ACCEPTED
        if tool == 'kmp_curate' and arguments['mode'] == 'review':
            if 'cursor' in arguments:
                return {'missing': []}
            return {'review_token': 'tok', 'next_actions': [{'arguments': {'mode': 'review', 'cursor': 1}}],
                    'missing': [{'item_id': 'i1', 'from': {'ref': 's:a:1'}, 'to': {'ref': 's:a:2'},
                                 'suggested_rel': 'r', 'proposed_by': 'jev'}]}
        if tool == 'kmp_curate' and arguments['mode'] == 'apply':
            return {'curate': {'doubted': []}}
        if tool == 'kmp_curate' and arguments['mode'] == 'paths':
            hops = [{'from': {'ref': 's:a:1'}, 'to': {'ref': 's:a:2'}, 'declared': arm == 'judged'}]
            return {'paths': [{'hops': hops}] if arm == 'judged' else []}
        if tool == 'kmp_ask':
            first = 'entry:s:a:2' if arm != 'plain' else 'entry:s:a:7'
            return {'proof': {'evidence': [{'id': first}, {'id': 'entry:s:a:2'}]}}
        if tool == 'kmp_wake':
            if arguments.get('page') == 2:
                return {'packet': 's:a:2'}
            return {'packet': 's:a:1', 'projection': {'next_action': {'tool': 'kmp_wake',
                                                                      'arguments': {'page': 2}}}}
        raise AssertionError((label, tool, arguments))

    def test_one_case_through_every_arm(self):
        stores = FakeStores(self.answer)
        scores = jj.run_collection(stores, [self.CASE])
        self.assertEqual([f'jev-k-{arm}' for arm in jj.ARMS], [label for label, _ in stores.opened])
        self.assertEqual((('typesafe.json', jj.TYPESAFE), ('wake-focus.json', jj.WAKE_FOCUS)),
                         stores.opened[-1][1])
        rows = scores.row_map()
        self.assertEqual(1.0, rows['cases'])
        self.assertEqual((0.5, 1.0, 1.0), (rows['ask_mrr_plain'], rows['ask_mrr_rerank'],
                                           rows['ask_mrr_wide']))
        self.assertEqual((0.0, 1.0), (rows['path_found_declared_only'], rows['path_found_with_jev']))
        self.assertEqual((0.5, 1.0), (rows['wake_first_page_plain'], rows['wake_all_pages_plain']))
        # the check whose pair was never reviewed counts as unavailable, not as wrong
        self.assertEqual((0.5, 1.0), (rows['precheck_available'], rows['precheck_right']))
        applied = [args for label, tool, args in stores.calls if args.get('mode') == 'apply']
        self.assertEqual([{'mode': 'apply', 'about': 's:a', 'actor': jj.ACTOR, 'review_token': 'tok',
                           'accepted': [{'item_id': 'i1', 'rel': 'r', 'why': 'w', 'evidence': 'e',
                                         'reverse': True}]}], applied)
        self.assertTrue(any('not reviewed' in line for line in scores.lines))

    def test_an_unrecorded_judgement_stops_the_run(self):
        def answer(label, tool, arguments):
            if tool == 'kmp_curate' and arguments['mode'] == 'review':
                return {'warnings': ['Jev unavailable: offline']}
            return self.answer(label, tool, arguments)

        with self.assertRaises(jj.JevUnrecorded):
            jj.run_collection(FakeStores(answer), [self.CASE])

    def test_a_wake_that_never_ends_is_an_error(self):
        def answer(label, tool, arguments):
            if tool == 'kmp_wake':
                return {'projection': {'next_action': {'tool': 'kmp_wake', 'arguments': {}}}}
            return self.answer(label, tool, arguments)

        with self.assertRaises(jj.WakeRunaway):
            jj.run_collection(FakeStores(answer), [self.CASE])

    def test_the_repository_corpus_loads(self):
        cases = jj.load_cases(ROOT / jj.DEFAULT_CASES)
        self.assertEqual(['payments-platform', 'atlas-large'], [c['id'] for c in cases])
        recorded = jr.parse_baseline((ROOT / jj.DEFAULT_BASELINE).read_text())
        self.assertFalse(set(jj.COLUMNS) - set(recorded))


# --- External scorecards -------------------------------------------------------------------------

class ExternalScorecardTest(unittest.TestCase):
    STDOUT = ('case a   1.00\n\n35 cases (the original judged collection)\n'
              '  recall_at_1                      0.8429\n  mean_used_bytes     1919\n'
              '  recall_at_1                      0.1\n  note: nothing 1.0 here\n')

    def test_metric_lines_are_read_like_jev_samples(self):
        self.assertEqual({'recall_at_1': 0.8429, 'mean_used_bytes': 1919.0},
                         ext.parse_metrics(self.STDOUT))

    def test_build_reads_the_executable_from_cargo(self):
        seen = []

        def runner(command, **kwargs):
            seen.append(command)
            lines = [json.dumps({'reason': 'compiler-artifact', 'target': {'name': 'other'},
                                 'executable': '/x/other'}), 'not json',
                     json.dumps({'reason': 'compiler-artifact', 'target': {'name': 'relate_kmp_scorecard'},
                                 'executable': '/x/relate'})]
            return SimpleNamespace(returncode=0, stdout='\n'.join(lines), stderr='')

        self.assertEqual(Path('/x/relate'), ext.build('relate_kmp_scorecard', ROOT, runner, release=True))
        self.assertEqual('--release', seen[0][2])
        with self.assertRaises(ext.ScorecardFailed):
            ext.build('x', ROOT, lambda *a, **k: SimpleNamespace(returncode=1, stdout='', stderr='boom'))
        with self.assertRaises(ext.ScorecardFailed):
            ext.build('x', ROOT, lambda *a, **k: SimpleNamespace(returncode=0, stdout='', stderr=''))

    def test_run_isolates_home_and_tmp_and_absolutizes_paths(self):
        captured = {}

        def runner(command, **kwargs):
            captured.update(command=command, env=kwargs['env'])
            self.assertTrue(Path(kwargs['env']['TMPDIR']).is_dir())
            return SimpleNamespace(returncode=1, stdout=self.STDOUT, stderr='')

        with tempfile.TemporaryDirectory() as work:
            done = ext.run(ext.retrieval('narrow'), ROOT, work, runner, executable=Path('/bin/x'))
            self.assertEqual([], list(Path(work).iterdir()))  # scratch removed
        env = captured['env']
        self.assertEqual(1, done.returncode)
        self.assertEqual('narrow', env['RETRIEVAL_RERANK'])
        self.assertEqual(str(ROOT / 'crates/kmp-testkit/judged/retrieval.jev.cassette.json'),
                         env['KMP_TYPESAFE_CASSETTE'])
        self.assertNotEqual(os.environ.get('HOME'), env['HOME'])
        self.assertEqual(str(ROOT / 'crates/kmp-testkit/judged/retrieval_cases.json'), captured['command'][1])
        self.assertNotIn('TYPESAFE_API_KEY', env)
        self.assertEqual('1', ext.jev().env['JEV_REPORT_ONLY'])
        self.assertEqual({}, ext.relate().env)
        with tempfile.TemporaryDirectory() as work, self.assertRaises(ext.ScorecardFailed):
            ext.run(ext.relate(), ROOT, work, lambda *a, **k: SimpleNamespace(
                returncode=101, stdout='panic', stderr='x'), executable=Path('/bin/x'))


# --- Live: the release binary over stdio -----------------------------------------------------------

@unittest.skipUnless(SLOW and BINARY.is_file(), 'set MEMORY_BENCH_SLOW=1 with a release kmp-mcp')
class LiveReproductionTest(unittest.TestCase):
    """Acceptance of BT16: the recorded baselines, reproduced through the release binary."""

    def setUp(self):
        base = ROOT / 'tmp/memory-bench/work'
        base.mkdir(parents=True, exist_ok=True)
        self.work = Path(tempfile.mkdtemp(prefix='judged-live-', dir=base))
        self.addCleanup(shutil.rmtree, self.work, True)

    def stores(self, **env):
        env = {'KMP_LEXICAL_BRIDGE': str(ROOT / jr.DEFAULT_BRIDGE), **env}
        return js.StdioStores(BINARY, self.work / 'stores', self.work / 'traces', env)

    def assert_reproduced(self, measured, baseline, names):
        recorded = jr.parse_baseline((ROOT / baseline).read_text())
        failed = [c for c in jr.compare_to_recorded(measured, recorded, 1e-4, names) if not c.within]
        self.assertEqual([], failed)

    def test_retrieval_baseline_35_and_53(self):
        scored = jr.run_collection(self.stores(), jr.load_cases(ROOT / jr.DEFAULT_CASES))
        self.assert_reproduced(scored.row_map(), jr.DEFAULT_BASELINE, None)
        values = scored.row_map()
        self.assertAlmostEqual(0.8428, values['recall_at_1'], delta=1e-4)
        self.assertAlmostEqual(0.9071, values['mean_reciprocal_rank'], delta=1e-4)
        self.assertAlmostEqual(0.9160, values['ndcg_at_10'], delta=1e-4)
        self.assertAlmostEqual(0.8857, values['answer_core_precision'], delta=1e-4)

    def test_retrieval_narrow_rerank_from_the_cassette(self):
        stores = self.stores(KMP_TYPESAFE_CASSETTE=str(ROOT / jr.DEFAULT_ARM_CASSETTE),
                             KMP_TYPESAFE_CASSETTE_MODE='replay')
        scored = jr.run_collection(stores, jr.load_cases(ROOT / jr.DEFAULT_CASES), rerank='narrow')
        values = scored.row_map()
        self.assertEqual(35.0, values['cases'])
        self.assertAlmostEqual(0.9000, values['recall_at_1'], delta=1e-4)
        self.assertAlmostEqual(0.9643, values['mean_reciprocal_rank'], delta=1e-4)

    def test_jev_cases_in_replay(self):
        stores = self.stores(KMP_TYPESAFE_CASSETTE=str(ROOT / jj.DEFAULT_CASSETTE),
                             KMP_TYPESAFE_CASSETTE_MODE='replay')
        scores = jj.run_collection(stores, jj.load_cases(ROOT / jj.DEFAULT_CASES))
        self.assert_reproduced(scores.row_map(), jj.DEFAULT_BASELINE, ('cases',) + jj.COLUMNS)
        values = scores.row_map()
        self.assertEqual((0.1875, 0.9375), (values['ask_mrr_plain'], values['ask_mrr_rerank']))
        self.assertEqual((0.0, 0.8), (values['path_found_declared_only'], values['path_found_with_jev']))
        self.assertEqual((1.0, 1.0), (values['wake_all_pages_plain'], values['wake_all_pages_focused']))


if __name__ == '__main__':
    unittest.main()
