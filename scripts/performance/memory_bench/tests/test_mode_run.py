"""BT12: modes.toml, the sections of a mode, git_ref builds, judged corpora and the summary.

The run-level tests drive the fake kmp-mcp of test_runtime (a Python script behind
the real token_harness transport); nothing here needs a release binary, a private
store or the network. The live quick-a is exercised by hand (docs/development/
memory-bench.md) because it needs the private freeze and minutes of real asks.
"""
import contextlib
from contextlib import contextmanager
import dataclasses
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from ..application import judged_section, mode_run, modes as modes_module, real_section, sections
from ..application import summary_markdown, synth_section
from ..application.arms import ArmSpec, arm_on, fixture_for
from ..application.report import jev_samples
from ..application.run_data import load_run
from ..application.run_questions import RunPlan, StoreRef, run_arms
from ..corpora import negative_rules
from ..corpora.judged_stdio import StoreFileNotApplied
from ..domain.errors import BenchError
from ..runtime import git_build, jev_fixture, server_log
from ..runtime.layout import public_layout
from .test_runtime import FakeBinaryCase, question, variant

MODES = modes_module.load_modes()


def fake_question(identifier, kind='lookup_exact', tags=(), rules=None, corpus='hard-negatives'):
    """A valid question re-labelled: selection code reads only id, type, tags and source."""
    source = None if rules is None else {'kind': 'generator', 'rule': 'r', 'rules_sha256': rules, 'seed': 7}
    return dataclasses.replace(question(identifier, 'q'), type=kind, tags=tuple(sorted(tags)), source=source,
                               corpus=corpus)


class ModesTest(unittest.TestCase):
    def test_the_repository_modes_fix_every_run_parameter(self):
        quick = MODES.get('quick-a')
        self.assertEqual((quick.max_calls, quick.samples, quick.repeats, quick.max_bytes, quick.timeout_s),
                         (256, 1, 1, 4096, 60.0))
        self.assertEqual(quick.budget_s, 600.0)
        self.assertEqual(quick.sections, ('real-store', 'synth', 'retrieval-judged', 'jev-judged'))
        self.assertEqual(MODES.get('jev').samples, 3)
        aa = MODES.get('aa')
        self.assertEqual((aa.replica_of, aa.run_mode, aa.sections, aa.real), ('quick-a', 'quick-a',
                                                                             quick.sections, quick.real))
        self.assertEqual(MODES.get('full').run_mode, 'full')
        with self.assertRaises(modes_module.ModeInvalid):
            MODES.get('nope')

    def test_a_cached_run_measured_otherwise_is_refused(self):
        quick = MODES.get('quick-a')
        manifest = {'run_id': 'x', 'samples': 1, 'repeats': 1, 'max_calls': 256, 'timeout_s': 60.0,
                    'max_bytes': [4096]}
        quick.check_run(manifest)
        with self.assertRaisesRegex(modes_module.ModeInvalid, 'repeats'):
            quick.check_run({**manifest, 'repeats': 2})
        quick.check_run({**manifest, 'max_bytes': [2048]}, max_bytes=2048)

    def test_invalid_mode_files_are_refused(self):
        text = (modes_module.MODES_PATH.read_text())
        with self.assertRaises(modes_module.ModeInvalid):
            modes_module.parse_modes(text.replace('"neutral",\n]', ']'))
        with self.assertRaises(modes_module.ModeInvalid):
            modes_module.parse_modes(text.replace('sections = ["real-store", "synth", "retrieval-judged", '
                                                  '"jev-judged"]', 'sections = ["elsewhere"]', 1))
        with self.assertRaises(modes_module.ModeInvalid):
            modes_module.parse_modes(text + '\n[modes.bb]\nreplica_of = "aa"\n')
        with self.assertRaises(modes_module.ModeInvalid):
            modes_module.parse_modes('schema = "other"')

    def test_verdicts_combine_worst_first_over_the_sections_that_vote(self):
        def result(name, verdict, applicable=True, status='ran'):
            return sections.SectionResult(name, status, verdict=verdict, reasons=[f'{name} said so'],
                                          applicable=applicable)
        value, reasons = mode_run.combine(MODES, [result('a', 'mejora'), result('b', 'neutral'),
                                                  result('c', 'regresion', applicable=True),
                                                  result('d', 'captura_fallida', status='skipped')])
        self.assertEqual((value, reasons), ('regresion', ['c: c said so']))
        value, _ = mode_run.combine(MODES, [result('a', 'mejora'), result('b', 'indecidible', False)])
        self.assertEqual(value, 'mejora')
        self.assertEqual(mode_run.combine(MODES, [result('a', 'neutral', False)])[0], 'indecidible')

    def test_a_section_votes_when_it_measured_a_target_or_something_went_wrong(self):
        def verdict(value, n=None, parity=None):
            targets = [{'metric': 'useful_rate', 'baseline': {'n': n}}] if n is not None else []
            if parity is not None:
                targets.append({'metric': 'parity_rate', 'decidable': parity})
            return {'value': value, 'targets': targets, 'guards': [{'metric': 'core_precision', 'worsening': 0.0}]}
        self.assertFalse(sections.applicable(verdict('indecidible', n=0)))
        self.assertTrue(sections.applicable(verdict('indecidible', n=12)))
        self.assertTrue(sections.applicable(verdict('neutral', parity=True)))
        self.assertFalse(sections.applicable(verdict('neutral', parity=False)))
        self.assertTrue(sections.applicable(verdict('regresion', n=0)))


class RealSectionTest(unittest.TestCase):
    def test_strata_take_the_first_questions_by_id(self):
        questions = [fake_question(f'hn-{i:02d}', 'anchor_absent' if i % 2 else 'near_miss_attribute',
                                   ['form:short' if i < 6 else 'form:long']) for i in range(12)]
        chosen = real_section.stratified(reversed(questions), 2, real_section.NEGATIVE_STRATA)
        self.assertEqual([q.id for q in chosen], ['hn-07', 'hn-09', 'hn-01', 'hn-03',
                                                  'hn-06', 'hn-08', 'hn-00', 'hn-02'])
        self.assertEqual(len(real_section.stratified(questions, 0, ())), 12)

    def test_negatives_run_only_under_the_pinned_and_registered_rules(self):
        pinned = negative_rules.registered_sha256()
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaisesRegex(BenchError, 'config pins'):
                real_section.require_rules([fake_question('a', rules='0' * 64)], root)
            with self.assertRaises(BenchError):  # no registry yet
                real_section.require_rules([fake_question('a', rules=pinned)], root)
            Path(root, negative_rules.REGISTRY_NAME).write_text(json.dumps({'rules_sha256': pinned}) + '\n')
            self.assertEqual(real_section.require_rules([fake_question('a', rules=pinned)], root), pinned)

    def test_the_freeze_is_the_latest_dated_one_under_the_private_root(self):
        with tempfile.TemporaryDirectory() as root:
            self.assertIsNone(real_section.find_freeze(env={}))
            self.assertIsNone(real_section.find_freeze(private_root=root))
            for name in ('2026-09-01', '2026-09-26', '2026-10-01'):
                Path(root, name).mkdir()
            for name in ('2026-09-01', '2026-09-26'):
                Path(root, name, 'freeze.json').write_text('{}')
            found = real_section.find_freeze(env={'MEMORY_BENCH_PRIVATE_ROOT': root})
            self.assertEqual(found.root.name, '2026-09-26')

    def test_an_undated_freeze_is_never_the_default(self):
        # A named freeze (`breal-ext`) sorts after every date; picking it by default
        # silently swapped the real-store questions (170 B-real, no negatives).
        with tempfile.TemporaryDirectory() as root:
            for name in ('2026-09-26', 'breal-ext', 'cache'):
                Path(root, name).mkdir()
                Path(root, name, 'freeze.json').write_text('{}')
            found = real_section.find_freeze(private_root=root)
            self.assertEqual(found.root.name, '2026-09-26')
            explicit = real_section.find_freeze(freeze=Path(root, 'breal-ext'))
            self.assertEqual(explicit.root.name, 'breal-ext')
        skipped = real_section.run((None, None), MODES.get('quick-a'), None, None)
        if os.environ.get('MEMORY_BENCH_PRIVATE_ROOT') is None:
            self.assertEqual(skipped.status, 'skipped')


class FakeRunner:
    """git, cargo and the built binary, as subprocess.run sees them."""

    def __init__(self, root):
        self.root, self.calls = Path(root), []

    def __call__(self, argv, cwd=None, env=None, **kwargs):
        self.calls.append(list(argv))
        out = ''
        if argv[:2] == ['git', 'rev-parse']:
            out = 'c' * 40 if 'HEAD' in argv[-1] or argv[-1].startswith('main') else ''
            if not out:
                return subprocess.CompletedProcess(argv, 128, '', 'unknown revision')
        elif argv[:3] == ['git', 'worktree', 'add']:
            Path(argv[4]).mkdir(parents=True)
        elif argv[:3] == ['git', 'worktree', 'remove']:
            import shutil
            shutil.rmtree(argv[-1])
        elif argv[:2] == ['cargo', 'build']:
            built = Path(env['CARGO_TARGET_DIR']) / 'release' / 'kmp-mcp'
            built.parent.mkdir(parents=True, exist_ok=True)
            built.write_text('#!/bin/sh\necho "kmp-mcp 9.9.9 (fake)"\n')
            built.chmod(0o755)
        elif argv[1:] == ['--version']:
            out = {'cargo': 'cargo 1.0', 'rustc': 'rustc 1.0'}.get(argv[0], 'kmp-mcp 9.9.9 (fake)')
        return subprocess.CompletedProcess(argv, 0, out + '\n', '')


class GitBuildTest(unittest.TestCase):
    def test_a_git_ref_is_built_once_in_a_removed_worktree_with_provenance(self):
        with tempfile.TemporaryDirectory() as folder:
            layout = public_layout(Path(folder) / 'cache')
            runner = FakeRunner(folder)
            built = git_build.build_ref('HEAD', layout, repo_root=folder, runner=runner, log=io.StringIO())
            self.assertEqual(built.path.parent, layout.entry('bin', built.sha256))
            provenance = json.loads((built.path.parent / 'provenance.json').read_text())
            self.assertEqual((provenance['git_commit'], provenance['git_ref'], provenance['build']),
                             ('c' * 40, 'HEAD', 'cargo build --release --locked -p kmp-mcp'))
            self.assertEqual(provenance['version'], 'kmp-mcp 9.9.9 (fake)')
            self.assertFalse(any(p.name.startswith('build-') for p in layout.scratch().iterdir()))
            builds = sum(1 for call in runner.calls if call[:2] == ['cargo', 'build'])
            again = git_build.build_ref('HEAD', layout, repo_root=folder, runner=runner)
            self.assertEqual((again.sha256, builds),
                             (built.sha256, sum(1 for c in runner.calls if c[:2] == ['cargo', 'build'])))
            with self.assertRaises(git_build.GitBuildFailed):
                git_build.build_ref('nope', layout, repo_root=folder, runner=runner)

    def test_a_path_binary_outside_the_checkout_has_source_path_only(self):
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'kmp-mcp'
            binary.write_text('#!/bin/sh\necho "kmp-mcp 0.0.1"\n')
            binary.chmod(0o755)
            resolved = git_build.resolve_path(binary)
            self.assertEqual((resolved.version, resolved.provenance), ('kmp-mcp 0.0.1', {'source': 'path'}))
            with self.assertRaises(git_build.GitBuildFailed):
                git_build.resolve_path(Path(folder) / 'missing')


class FakeInner:
    """StdioStores stand-in: every store closes refusing its files for `reason`."""

    def __init__(self, reason):
        self.reason, self.reports = reason, []

    @contextmanager
    def open(self, label, files=()):
        yield label
        self.reports.append({'label': label, 'store_files': [
            {'name': f.name, 'sha256': f.sha256, 'status': 'not_applied', 'reason': self.reason} for f in files]})
        if files:
            raise StoreFileNotApplied(f'{label}: refused')


class JudgedSectionTest(unittest.TestCase):
    def test_rows_bind_as_the_recorded_files_say(self):
        self.assertEqual([judged_section.bound(n) for n in ('cases', 'recall_at_1', 'false_unknown_rate',
                                                             'false_answer_rate_anchor_absent')],
                         ['exact', 'floor', 'ceiling', 'ceiling'])
        self.assertTrue(judged_section.holds('recall_at_1', 0.84285, 0.8428))
        self.assertFalse(judged_section.holds('recall_at_1', 0.8420, 0.8428))
        self.assertFalse(judged_section.holds('false_unknown_rate', 0.1, 0.05))

    def test_a_candidate_that_breaks_a_row_the_baseline_holds_regresses(self):
        from ..corpora import judged_retrieval
        recorded = judged_retrieval.parse_baseline(
            (judged_section.REPO_ROOT / judged_retrieval.DEFAULT_BASELINE).read_text())
        arm = judged_section.CorpusArm('retrieval', 'plain')
        base = {'rows': dict(recorded), 'unverified_stores': 0, 'cached': True}
        cand = {'rows': {**recorded, 'recall_at_1': recorded['recall_at_1'] - 0.05}, 'unverified_stores': 3,
                'cached': False}
        row = judged_section.compare(arm, base, cand, judged_section.REPO_ROOT)
        self.assertTrue(row['recorded']['baseline']['holds'])
        self.assertEqual((row['breaks_recorded'], row['worse']), (['recall_at_1'], ['recall_at_1']))
        verdict, reasons = judged_section._verdict([row], 3)
        self.assertEqual(verdict, 'regresion')
        same = judged_section.compare(arm, base, base, judged_section.REPO_ROOT)
        self.assertEqual(judged_section._verdict([same], 0), ('neutral', ['every row equal on both arms']))

    def test_only_a_binary_without_store_config_telemetry_is_tolerated(self):
        files = variant('v', store_files={'rerank.json': {'json': {'pool_size': 40}}}).store_files
        stores = judged_section.AckStores(FakeInner(server_log.PREDATES_STORE_CONFIG), files)
        with stores.open('case-1') as label:
            self.assertEqual(label, 'case-1')
        self.assertEqual(stores.unverified, ['case-1'])
        strict = judged_section.AckStores(FakeInner('not read by this kmp-mcp version'), files)
        with self.assertRaises(StoreFileNotApplied):
            with strict.open('case-2'):
                pass


def spec(name, binary, **extra):
    resolved = git_build.ResolvedBinary(Path(binary), git_build.sha256_file(binary), 'fake', {'source': 'path'})
    return ArmSpec(variant(name, **extra), f'variants/{name}.toml', resolved)


class JevHookTest(FakeBinaryCase):
    def run_jev(self, bt03):
        cassette = self.root / 'named.cassette.json'
        jev_fixture.empty_cassette(cassette, jev_fixture.DEFAULT_MODEL)
        arm_spec = spec('jevarm', self.binary, jev='replay', env={'KMP_TYPESAFE_CASSETTE': str(cassette)})
        store = StoreRef('3' * 64, None, 'fake', self.template('t', bt03))
        questions = (question('q0', 'judge plain'), question('q1', 'plain'))
        arm = arm_on(arm_spec, store, self.layout, questions)
        self.assertEqual(arm.jev_fixture.key, fixture_for(arm_spec, self.layout, store.key, questions).key)
        plan = RunPlan(questions, 'test', samples=2, warmup=0, cpus=self.cpu, timeout_s=5.0)
        [result] = run_arms([arm], plan, self.layout)
        return result

    def test_every_sample_gets_its_own_fixture_and_its_seal_reaches_the_manifest(self):
        result = self.run_jev(True)
        samples = jev_samples(load_run(result.run_dir))
        self.assertEqual([(s['sample'], s['status']) for s in samples], [(0, 'complete'), (1, 'complete')])
        self.assertEqual(samples[0]['by_source'], {'cassette_hit': 1})
        self.assertEqual(result.manifest['failures'], [])

    def test_a_replay_without_telemetry_is_unverified_not_a_failure(self):
        result = self.run_jev(False)
        samples = result.manifest['isolation']['jev']['samples']
        self.assertEqual({s['status'] for s in samples}, {'unverified'})
        self.assertEqual(result.manifest['failures'], [])

    def test_record_needs_the_key_and_the_jev_mode(self):
        arm_spec = spec('rec', self.binary, jev='record')
        with self.assertRaises(BenchError):
            arm_on(arm_spec, StoreRef('3' * 64, None, 'fake', None), self.layout, (question('q', 'x'),))


class SectionsTest(FakeBinaryCase):
    def test_a_section_measures_both_arms_and_writes_its_report(self):
        mode = dataclasses.replace(MODES.get('quick-a'), cpus=self.cpu, warmup=0, bootstrap_b=20,
                                   timeout_s=5.0)
        specs = (spec('base', self.binary), spec('cand', self.binary, store_files={
            'rerank.json': {'json': {'pool_size': 40}}}))
        stores = [StoreRef('3' * 64, None, 'fake', self.template(f't{i}', True)) for i in range(2)]
        questions = (question('q0', 'plain'), question('q1', 'judge plain'))
        result = sections.run_section('fake', specs, stores, questions, mode, self.layout)
        self.assertEqual(result.status, 'ran', result.reason)
        self.assertTrue((result.report_dir / 'report.json').is_file())
        self.assertTrue((result.report_dir / 'report.md').is_file())
        self.assertEqual(result.layout, 'public')
        self.assertEqual(result.parity['calls'], 2)
        again = sections.run_section('fake', specs, stores, questions, mode, self.layout)
        self.assertEqual(again.status, 'ran', again.reason)
        self.assertEqual({r['cached'] for r in again.runs.values()}, {True})
        stricter = dataclasses.replace(mode, repeats=2)
        refused = sections.run_section('fake', specs, stores, questions, stricter, self.layout)
        self.assertEqual((refused.status, refused.verdict), ('failed', 'captura_fallida'))
        self.assertIn('repeats', refused.reason)

    def test_an_aa_section_replays_the_baseline_under_a_nonce(self):
        mode = dataclasses.replace(MODES.get('aa'), cpus=self.cpu, warmup=0, bootstrap_b=20, timeout_s=5.0)
        base = spec('base', self.binary)
        store = StoreRef('3' * 64, None, 'fake', self.template('t', True))
        questions = (question('q0', 'plain'), question('q1', 'plain'))
        result = sections.run_section('aa', (base, base), [store, store], questions, mode, self.layout,
                                      replica_nonce='aa-1')
        self.assertEqual(result.status, 'ran', result.reason)
        self.assertNotEqual(result.runs['baseline']['run_id'], result.runs['candidate']['run_id'])
        self.assertTrue(result.aa['holds'])
        self.assertEqual(result.verdict, 'neutral')


class CliTest(FakeBinaryCase):
    def run_cli(self, *argv):
        from .. import cli
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = cli.main(list(argv))
        return code, out.getvalue(), err.getvalue()

    def test_mode_commands_refuse_a_candidate_that_is_the_baseline_and_other_samples(self):
        toml = f'name = "{{}}"\nclaim = "parity"\ntargets = ["parity_rate"]\nn = 1\nstore = "shared"\n' \
               f'jev = "off"\n[binary]\npath = "{self.binary}"\n'
        for name in ('one', 'two'):
            (self.root / f'{name}.toml').write_text(toml.format(name))
        code, _, err = self.run_cli('quick-a', '--baseline', str(self.root / 'one.toml'),
                                    '--candidate', str(self.root / 'two.toml'), '--out', str(self.root / 'c'))
        self.assertEqual(code, 2)
        self.assertIn('MODE_REFUSED', err)
        code, _, err = self.run_cli('jev', '--candidate', str(self.root / 'two.toml'), '--samples', '2')
        self.assertEqual((code, 'MODE_INVALID' in err), (2, True))
        from ...token_harness.adapters import tiktoken_counter
        with mock.patch.dict(os.environ, {'TIKTOKEN_CACHE_DIR': str(self.root / 'no-assets')}), \
                mock.patch.object(tiktoken_counter, '_cache_dir', None):
            code, out, _ = self.run_cli('prepare', '--cache-dir', str(self.root / 'no-assets'))
        self.assertEqual((code, json.loads(out)['ready']), (1, False))


class SynthSelectTest(unittest.TestCase):
    def test_per_type_keeps_the_first_questions_of_each_type_at_the_lowest_level(self):
        ladder = modes_module.SynthLadder(7, ('mono',), (1000,), 5000, (), 1)
        questions = [dataclasses.replace(fake_question(f's{i}', kind, corpus='synth-v1'), min_level=level)
                     for i, (kind, level) in enumerate([('a', 1000), ('a', 1000), ('b', 10000), ('b', 1000)])]
        self.assertEqual([q.id for q in synth_section.select(questions, ladder)], ['s0', 's3'])
        everything = dataclasses.replace(ladder, per_type=0)
        self.assertEqual(len(synth_section.select(questions, everything)), 3)


class SummaryTest(unittest.TestCase):
    def summary(self):
        section = sections.SectionResult('synth-mono', 'ran', layout='public', report_key='f' * 64,
                                         verdict='neutral', reasons=['same'],
                                         headline={'baseline': {'useful_rate': 0.5}, 'candidate': {'useful_rate': 0.5}},
                                         parity={'calls': 2, 'different': 0, 'parity_rate': 1.0},
                                         aa={'quality_delta_zero': True, 'token_delta_zero': False,
                                             'token_delta_zero_normalized': True, 'moved': [],
                                             'raw_moved': ['tokens_journey']})
        judged = sections.SectionResult('retrieval-judged', 'ran', layout='public', verdict='neutral', judged=[{
            'arm': 'retrieval-plain', 'baseline': {'recall_at_1': 0.8}, 'candidate': {'recall_at_1': 0.8},
            'deltas': {'recall_at_1': 0.0}, 'worse': [], 'breaks_recorded': [],
            'recorded': {'baseline': {'holds': True, 'failing': [], 'not_measured': 2, 'rows': 9,
                                      'file': 'docs/x.tsv'}, 'candidate': None},
            'unverified_stores': {'baseline': 0, 'candidate': 1}, 'cached': {'baseline': True, 'candidate': False}}])
        return {'summary_key': 'a' * 64, 'bench_version': 'kmp.memory_bench.v3', 'modes_sha256': 'b' * 64,
                'mode': 'quick-a', 'generated_by': {'code_sha256': 'c' * 64},
                'timings': {'started_at': '2026-09-26T00:00:00Z', 'total_s': 12.5, 'budget_s': 600.0,
                            'within_budget': True, 'by_section': {}},
                'verdict': {'value': 'neutral', 'reasons': ['synth-mono: same']},
                'arms': {'baseline': {'variant': 'baseline', 'claim': 'parity', 'jev': 'off',
                                      'binary_sha256': 'd' * 64, 'binary_version': 'kmp-mcp 0.23.0',
                                      'binary_provenance': {'source': 'git_ref', 'git_ref': 'HEAD',
                                                            'git_commit': 'e' * 40},
                                      'config_digest': '1' * 64}, 'candidate': None},
                'parameters': {'samples': 1}, 'sections': [section.as_dict(), judged.as_dict(),
                                                           sections.skipped('public', 'BT17').as_dict()]}

    def test_the_markdown_is_deterministic_and_names_verdict_sections_and_rows(self):
        text = summary_markdown.render(self.summary())
        self.assertEqual(text, summary_markdown.render(self.summary()))
        for needle in ('**Verdict: `neutral`**', 'within budget', '| synth-mono | ran | neutral |',
                       '0.500 → 0.500', '1.0000 (0 differ)', '### retrieval-plain', 'replica of the baseline',
                       '**public** skipped: BT17', 'git_ref `HEAD`', '9 rows of `docs/x.tsv`, 2 not ported',
                       'A/A: quality deltas 0: yes; token deltas 0: no raw, yes with handles normalized; '
                       'moved: none'):
            self.assertIn(needle, text)


if __name__ == '__main__':
    unittest.main()
