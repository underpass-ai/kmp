"""BT17 wiring: the `public` section of `full`, the `ci` mode and the CLI mounts (fetch, mixed)."""
import contextlib
import io
from types import SimpleNamespace
import unittest
from unittest import mock

from .. import cli
from ..application import mixed_versions_cli, modes as modes_module, public_cli, public_section, sections
from ..corpora.public_errors import FetchFailed, LicenceRefused
from ..runtime.layout import public_layout

MODES = modes_module.load_modes()


class PublicModesTest(unittest.TestCase):
    def test_full_names_its_public_corpora_and_quick_modes_have_none(self):
        full = MODES.get('full')
        self.assertIn('public', full.sections)
        self.assertEqual(full.public.corpora, ('factconsolidation', 'longmemeval-s', 'musique', '2wiki'))
        self.assertEqual((full.public.setup, full.public.max_calls, full.public.sizes), ('subset', 1, ('32k',)))
        self.assertIsNone(MODES.get('quick-a').public)

    def test_ci_replicates_the_public_part_of_quick_a(self):
        ci, quick = MODES.get('ci'), MODES.get('quick-a')
        self.assertEqual((ci.replica_of, ci.run_mode), ('quick-public', 'quick-public'))
        self.assertEqual(ci.sections, ('synth', 'retrieval-judged', 'jev-judged'))
        self.assertNotIn('real-store', ci.sections)
        self.assertEqual(ci.synth, quick.synth)
        self.assertEqual(ci.judged, quick.judged)
        self.assertEqual((ci.max_calls, ci.max_bytes, ci.timeout_s), (quick.max_calls, quick.max_bytes,
                                                                      quick.timeout_s))
        self.assertIsNone(ci.cpus)

    def test_a_public_section_needs_its_corpora(self):
        text = modes_module.MODES_PATH.read_text()
        with self.assertRaisesRegex(modes_module.ModeInvalid, 'public'):
            modes_module.parse_modes(text.replace('[modes.full.public]', '[modes.full.elsewhere]'))
        with self.assertRaisesRegex(modes_module.ModeInvalid, 'corpora'):
            modes_module.parse_modes(text.replace('corpora = ["factconsolidation"', 'corpora = ["nope"'))
        with self.assertRaisesRegex(modes_module.ModeInvalid, 'setup'):
            modes_module.parse_modes(text.replace('setup = "subset"', 'setup = "half"'))


def spec(name, binary='target/release/kmp-mcp'):
    return SimpleNamespace(name=name, path=f'variants/{name}.toml', binary=SimpleNamespace(path=binary))


class PublicSectionTest(unittest.TestCase):
    specs = (spec('baseline'), spec('candidate'))

    def test_unfetched_corpora_skip_the_section_with_the_fetch_command(self):
        def missing(args):
            if args.corpus == 'locomo':
                raise LicenceRefused('locomo is private')
            raise FetchFailed(f'{args.corpus}/x: missing (run fetch)')
        mode = MODES.get('full')
        with mock.patch.object(public_cli, 'load_corpora', missing):
            result = public_section.run(self.specs, mode, public_layout())
        self.assertEqual(result.status, sections.SKIPPED)
        self.assertIn('fetch --dataset memoryagentbench-cr', result.reason)
        self.assertEqual([row['corpus'] for row in result.public], list(mode.public.corpora))
        self.assertTrue(all(row['status'] == sections.SKIPPED for row in result.public))
        self.assertIsNone(result.verdict)

    def test_a_ran_corpus_reports_both_arms_and_the_delta_without_voting(self):
        corpus = SimpleNamespace(name='musique-hipporag200', private=False, questions=[1, 2, 3])
        headline = {'musique': {'questions': 3, 'recall_at_5': 0.5, 'unknown': {'value': 0.1}}}
        better = {'musique': {'questions': 3, 'recall_at_5': 0.75, 'unknown': {'value': 0.1}}}
        seen = []

        def load(args):
            seen.append((args.corpus, args.setup, args.per_type, args.size))
            if args.corpus != 'musique':
                raise FetchFailed('missing')
            return [(corpus, 'entry')]

        def run_corpus(corpus_, entry, specs, mode, layout, log=None):
            return {'baseline': {'run_id': 'a' * 64, 'cached': True, 'headline': headline},
                    'candidate': {'run_id': 'b' * 64, 'cached': False, 'headline': better}}
        with mock.patch.object(public_cli, 'load_corpora', load), \
                mock.patch.object(public_section, 'run_corpus', run_corpus):
            result = public_section.run(self.specs, MODES.get('full'), public_layout())
        self.assertIn(('musique', 'subset', 10, ('32k',)), seen)
        self.assertEqual((result.status, result.verdict, result.applicable), (sections.RAN, None, False))
        self.assertEqual(result.questions, {'musique-hipporag200': 3})
        row = next(r for r in result.public if r['status'] == sections.RAN)
        self.assertEqual(row['delta'], {'musique': {'questions': 0, 'recall_at_5': 0.25, 'unknown': {'value': 0.0}}})
        self.assertEqual(result.runs['musique-hipporag200']['candidate'], {'run_id': 'b' * 64, 'cached': False})
        self.assertIn('public', result.as_dict())

    def test_a_broken_corpus_fails_only_this_section(self):
        def broken(*args, **kwargs):
            raise public_section.BenchError('boom')
        with mock.patch.object(public_cli, 'load_corpora', broken):
            result = public_section.run(self.specs, MODES.get('full'), public_layout())
        self.assertEqual((result.status, result.verdict), (sections.FAILED, 'captura_fallida'))


class PublicMarkdownTest(unittest.TestCase):
    def test_public_rows_render_one_table_per_question_corpus(self):
        from ..application import summary_markdown
        base = {'musique': {'questions': 3, 'recall_at_5': 0.5}}
        cand = {'musique': {'questions': 3, 'recall_at_5': 0.75}}
        summary = {'sections': [{'public': [
            {'corpus': 'musique-hipporag200', 'status': 'ran', 'reason': None, 'questions': 3,
             'arms': {'baseline': {'headline': base}, 'candidate': {'headline': cand}},
             'delta': public_section.delta(base, cand)},
            {'corpus': '2wiki', 'status': 'skipped', 'reason': 'dataset not fetched'}]}]}
        lines = summary_markdown._public(summary)
        self.assertIn('| recall_at_5 | 0.5000 | 0.7500 | 0.2500 |', lines)
        self.assertIn('- **2wiki** skipped: dataset not fetched', lines)


class CliMountsTest(unittest.TestCase):
    def test_fetch_is_public_cli_fetch(self):
        args = cli.build_parser().parse_args(['fetch', '--dataset', 'musique', '--verify-only'])
        self.assertEqual((args.dataset, args.verify_only), (['musique'], True))
        with mock.patch.object(public_cli, 'cmd_fetch', return_value=0) as fetch:
            self.assertEqual(cli.main(['fetch', '--dataset', 'musique', '--verify-only']), 0)
        self.assertEqual(fetch.call_args[0][0].dataset, ['musique'])

    def test_mixed_and_ci_are_mounted(self):
        args = cli.build_parser().parse_args(['mixed', '--reference', 'ref', '--scenario', 'write_then_read'])
        self.assertIs(args.action, mixed_versions_cli.run_command)
        self.assertEqual((args.reference, args.scenario), ('ref', ['write_then_read']))
        args = cli.build_parser().parse_args(['ci', '--variant', 'v.toml'])
        self.assertIs(args.action, cli.run_mode_command)
        self.assertFalse(hasattr(args, 'private_root'))
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            cli.build_parser().parse_args(['ci', '--variant', 'v.toml', '--private-root', '/x'])


if __name__ == '__main__':
    unittest.main()
