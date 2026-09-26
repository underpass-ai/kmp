"""Cache layout containment and the CLI's stubs, validator and argument parsing."""
import argparse
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest

from .. import cli
from ..domain import cachekey
from ..domain.jsonl import REPO_ROOT
from ..runtime.layout import LayoutError, PRIVATE_ROOT_ENV, private_layout, public_layout

KEY = 'ab' * 32


class LayoutTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.folder = Path(folder.name)

    def test_public_layout_defaults_inside_the_git_ignored_tmp(self):
        layout = public_layout()
        self.assertEqual(layout.root, REPO_ROOT / 'tmp/memory-bench')
        self.assertEqual(layout.run(KEY), REPO_ROOT / 'tmp/memory-bench/runs' / KEY)
        self.assertFalse(layout.private)
        gitignore = (REPO_ROOT / '.gitignore').read_text(encoding='utf-8').splitlines()
        self.assertIn('/tmp/', gitignore)

    def test_entries_are_full_hex_keys_of_known_kinds(self):
        layout = public_layout(self.folder)
        for kind, method in (('worlds', layout.world), ('stores', layout.store), ('reports', layout.report)):
            self.assertEqual(method(KEY), self.folder / kind / KEY)
        with self.assertRaises(cachekey.CacheKeyInvalid):
            layout.run('../escape')
        with self.assertRaises(LayoutError):
            layout.entry('elsewhere', KEY)

    def test_writers_cannot_escape_the_root(self):
        layout = public_layout(self.folder / 'cache')
        self.assertTrue(layout.contains(layout.scratch() / 'x'))
        layout.require_inside(layout.run(KEY) / 'calls.jsonl.gz')
        with self.assertRaises(LayoutError):
            layout.require_inside(self.folder / 'cache/../outside')

    def test_private_layout_needs_an_explicit_root_outside_the_repository(self):
        with self.assertRaises(LayoutError):
            private_layout(env={})
        with self.assertRaises(Exception) as caught:
            private_layout(env={PRIVATE_ROOT_ENV: str(REPO_ROOT / 'tmp/private')})
        self.assertIn('PRIVATE_DATA_IN_REPO', str(caught.exception))
        layout = private_layout(env={PRIVATE_ROOT_ENV: str(self.folder / 'private')})
        self.assertEqual(layout.root, (self.folder / 'private/cache').resolve())
        self.assertTrue(layout.private)
        self.assertEqual(private_layout(root=self.folder / 'other', env={}).root,
                         (self.folder / 'other/cache').resolve())


class CliTest(unittest.TestCase):
    def run_cli(self, *argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = cli.main(list(argv))
        return code, out.getvalue(), err.getvalue()

    def test_every_spec_command_is_a_stub_naming_its_task(self):
        arguments = {'fetch': ['--dataset', 'musique'], 'world': ['--seed', '7'],
                     'quick-a': ['--candidate', 'v.toml'], 'full': ['--candidate', 'v.toml'],
                     'jev': ['--candidate', 'v.toml'], 'aa': ['--variant', 'v.toml'], 'cache': ['ls']}
        commands = cli.build_parser()._subparsers._group_actions[0].choices
        for command, owner in cli.OWNERS.items():
            if commands[command].get_default('action') is not cli.not_yet:
                continue  # landed: its own tests cover it
            with self.subTest(command=command):
                code, _, err = self.run_cli(command, *arguments.get(command, []))
                self.assertEqual(code, 2)
                self.assertIn(f'not implemented yet: {owner}', err)

    def test_levels_and_topologies(self):
        self.assertEqual(cli.parse_levels('1e3,1e4,1e5'), (1000, 10000, 100000))
        self.assertEqual(cli.parse_levels('1000'), (1000,))
        for bad in ('1e4,1e3', '1e3,1e3', '1.5', 'x', '0'):
            with self.assertRaises(argparse.ArgumentTypeError):
                cli.parse_levels(bad)
        self.assertEqual(cli.parse_topologies('mono,multi'), ('mono', 'multi'))
        for bad in ('mono,mono', 'star'):
            with self.assertRaises(argparse.ArgumentTypeError):
                cli.parse_topologies(bad)
        args = cli.build_parser().parse_args(['world', '--seed', '7', '--levels', '1e3', '--topology', 'multi'])
        self.assertEqual((args.seed, args.levels, args.topology), (7, (1000,), ('multi',)))

    def test_validate_reads_the_documented_fixtures(self):
        fixtures = Path(__file__).resolve().parent / 'fixtures'
        code, out, _ = self.run_cli('validate', '--questions', str(fixtures / 'questions.sample.jsonl'),
                                    '--variant', str(fixtures / 'variant.sample.toml'),
                                    '--calls', str(fixtures / 'calls.sample.jsonl'),
                                    '--journeys', str(fixtures / 'journeys.sample.jsonl'))
        self.assertEqual(code, 0)
        report = json.loads(out)
        self.assertEqual(report[str(fixtures / 'questions.sample.jsonl')]['questions'], 3)
        self.assertEqual(report[str(fixtures / 'calls.sample.jsonl')], {'calls': 2})

    def test_validate_fails_with_a_typed_error(self):
        with tempfile.NamedTemporaryFile('w', suffix='.jsonl', delete=False) as handle:
            handle.write('{"schema": "kmp.bench.question.v1"}\n')
        self.addCleanup(Path(handle.name).unlink)
        code, _, err = self.run_cli('validate', '--questions', handle.name)
        self.assertEqual(code, 2)
        self.assertIn('QUESTION_INVALID', err)


if __name__ == '__main__':
    unittest.main()
