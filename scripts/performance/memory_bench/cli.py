"""memory_bench CLI (BENCH_SPEC section 13). Run from the repository root:

  B='python3 -m scripts.performance.memory_bench'
  TT='uv run --no-project --with tiktoken==0.14.0 python -m scripts.performance.memory_bench'

  $TT prepare                       verify the pinned tiktoken assets
  $B  fetch --dataset NAME          download a public dataset with sha and licence lock
  $B  world --generator synth-v1 --seed 7 --levels 1e3,1e4,1e5 --topology mono,multi
  $B  build | materialize           build and cache stores
  $TT quick-a --candidate variants/X.toml
  $TT full --candidate variants/X.toml
  $TT jev --candidate variants/X.toml --samples 3
  $TT aa --variant variants/baseline.toml
  $B  compare | render | real | cache ls|gc
  $B  validate --questions F --variant F --calls F   check files against SCHEMAS.md

Commands whose task has not landed say which task owns them and exit 2.
"""
import argparse
import json
from pathlib import Path
import sys

from ..token_harness.domain.errors import HarnessError
from .domain.errors import NotImplementedYet
from .domain.jsonl import REPO_ROOT

# Command -> task that implements it (bench_tasks.json).
OWNERS = {'prepare': 'BT12', 'fetch': 'BT17', 'build': 'BT14',
          'materialize': 'BT14', 'quick-a': 'BT12', 'full': 'BT12', 'jev': 'BT11', 'aa': 'BT12',
          'compare': 'BT10', 'render': 'BT10', 'real': 'BT07', 'cache': 'BT14'}


def parse_levels(text):
    """'1e3,1e4,1e5' -> (1000, 10000, 100000); integers only, strictly increasing."""
    levels = []
    for item in text.split(','):
        try:
            value = float(item)
        except ValueError:
            raise argparse.ArgumentTypeError(f'{item!r} is not a number') from None
        if value != int(value) or value < 1:
            raise argparse.ArgumentTypeError(f'{item!r} is not a positive integer')
        levels.append(int(value))
    if levels != sorted(set(levels)):
        raise argparse.ArgumentTypeError('levels must be strictly increasing')
    return tuple(levels)


def parse_topologies(text):
    topologies = tuple(item.strip() for item in text.split(','))
    if not topologies or set(topologies) - {'mono', 'multi'} or len(set(topologies)) != len(topologies):
        raise argparse.ArgumentTypeError('topologies are mono and/or multi')
    return topologies


def not_yet(args):
    raise NotImplementedYet(f'not implemented yet: {OWNERS[args.command]}')


def run_world(args):
    from .application.world import run_world as run
    return run(args)


def run_validate(args):
    from .domain.question import load_questions, questions_digest
    from .domain.run_record import CallRecord, JourneyRecord
    from .domain.jsonl import parse_lines
    from .domain.errors import RunRecordInvalid
    from .domain.variant import load_variant
    report = {}
    for path in args.questions or []:
        questions = load_questions(path)
        report[str(path)] = {'questions': len(questions), 'digest': questions_digest(questions)}
    for path in args.variant or []:
        variant = load_variant(path, REPO_ROOT)
        report[str(path)] = {'variant': variant.name, 'config_digest': variant.config_digest(),
                             'preregistration_digest': variant.preregistration_digest()}
    for kind, paths, record in (('calls', args.calls, CallRecord), ('journeys', args.journeys, JourneyRecord)):
        for path in paths or []:
            rows = parse_lines(Path(path).read_text(encoding='utf-8'), str(path), RunRecordInvalid)
            for number, row in rows:
                record.from_dict(row, f'{path}:{number}')
            report[str(path)] = {kind: len(rows)}
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


def _stub(commands, name, help_text):
    command = commands.add_parser(name, help=f'{help_text} [{OWNERS[name]}]')
    command.set_defaults(action=not_yet)
    return command


def build_parser():
    parser = argparse.ArgumentParser(prog='memory_bench', description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    _stub(commands, 'prepare', 'verify the pinned tiktoken assets')
    _stub(commands, 'fetch', 'download a public dataset').add_argument(
        '--dataset', action='append', required=True)
    world = commands.add_parser('world', help='generate, check and calibrate synth-v1 worlds')
    world.set_defaults(action=run_world)
    world.add_argument('--generator', choices=('synth-v1',), default='synth-v1')
    world.add_argument('--seed', type=int, required=True)
    world.add_argument('--levels', type=parse_levels, default=(1000, 10000, 100000))
    world.add_argument('--topology', type=parse_topologies, default=('mono', 'multi'))
    world.add_argument('--probe', type=Path, help='kmp_search_probe (default target/release)')
    world.add_argument('--no-probe', action='store_true',
                       help='skip the probe oracles and the calibration (they are then reported skipped)')
    world.add_argument('--no-nested-check', action='store_true',
                       help='skip regenerating lower levels to check the nesting invariant')
    world.add_argument('--out', type=Path, help='cache root (default <repo>/tmp/memory-bench)')
    _stub(commands, 'build', 'build and cache stores')
    _stub(commands, 'materialize', 'materialize a cached store for a run')
    for name, help_text in (('quick-a', 'phase A quick run'), ('full', 'ladder and public corpora')):
        _stub(commands, name, help_text).add_argument('--candidate', type=Path, required=True)
    jev = _stub(commands, 'jev', 'Jev arms: samples with a fresh book each')
    jev.add_argument('--candidate', type=Path, required=True)
    jev.add_argument('--samples', type=int, default=3)
    _stub(commands, 'aa', 'A/A control of one variant').add_argument('--variant', type=Path, required=True)
    _stub(commands, 'compare', 'paired comparison into report.json')
    _stub(commands, 'render', 'report.md from report.json')
    _stub(commands, 'real', 'B-real private corpus (local release gate)')
    _stub(commands, 'cache', 'list or collect the cache').add_argument('operation', choices=('ls', 'gc'))
    validate = commands.add_parser('validate', help='check files against SCHEMAS.md contracts')
    validate.set_defaults(action=run_validate)
    for flag in ('questions', 'variant', 'calls', 'journeys'):
        validate.add_argument('--' + flag, type=Path, action='append')
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    try:
        return args.action(args)
    except HarnessError as error:
        print(str(error), file=sys.stderr)
        return 2
