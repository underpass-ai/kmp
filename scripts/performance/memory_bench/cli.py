"""memory_bench CLI (BENCH_SPEC section 13). Run from the repository root:

  B='python3 -m scripts.performance.memory_bench'
  TT='uv run --no-project --with tiktoken==0.14.0 python -m scripts.performance.memory_bench'

  $TT prepare                       verify the pinned tiktoken assets (offline)
  $B  fetch --dataset NAME          download a public dataset with sha and licence lock
  $B  world --generator synth-v1 --seed 7 --levels 1e3,1e4,1e5 --topology mono,multi
  $B  build | materialize           build and cache stores
  $TT quick-a --candidate variants/X.toml [--baseline variants/baseline.toml] [--freeze DIR]
  $TT full --candidate variants/X.toml
  $TT jev --candidate variants/X.toml --samples 3 [--record]
  $TT aa --variant variants/baseline.toml
  $TT scale --candidate variants/X.toml | scale-aa --variant variants/baseline.toml
  $B  ci --variant variants/baseline.toml   A/A on quick-public (scripts/ci/memory-bench-quick.sh)
  $B  mixed --reference PATH [--candidate PATH] [--old PATH]   mixed versions, multiprocess (BT18)
  $B  jev-tail [--self-check] [--concurrency 2,3,4]   Jev tail through a local fault proxy (BT19)
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

REPLICA_COMMANDS = ('aa', 'ci', 'scale-aa')  # a variant against a fresh replica of itself
DEFAULT_BASELINE = REPO_ROOT / 'scripts/performance/memory_bench/variants/baseline.toml'
# Command -> task that implements it (bench_tasks.json).
OWNERS = {'prepare': 'BT12', 'fetch': 'BT17', 'build': 'BT14',
          'materialize': 'BT14', 'quick-a': 'BT12', 'full': 'BT12', 'jev': 'BT11', 'aa': 'BT12',
          'scale': 'P11', 'scale-aa': 'P11',
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


def run_store_command(args):
    from .application import build
    return {'build': build.run_build, 'materialize': build.run_materialize,
            'cache': build.run_cache}[args.command](args)


def run_prepare(args):
    from .application import tokens
    counters, reason = tokens.try_load_counters(args.cache_dir)
    report = {'cache_dir': str(args.cache_dir or tokens.default_cache_dir()),
              'encoders': [{'encoding': c.identity.encoding, 'asset_sha256': c.identity.asset_sha256}
                           for c in counters], 'ready': bool(counters), 'reason': reason}
    if not counters:
        report['fix'] = ('python3 -m scripts.performance.token_harness prepare --cache-dir '
                         'tmp/memory-bench/tiktoken-cache (downloads the pinned assets once, checks their sha256)')
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if counters else 1


def run_fetch(args):
    from .application.public_cli import cmd_fetch
    return cmd_fetch(args)


def run_mode_command(args):
    from .application.mode_run import ModeRequest, run_mode
    if args.command == 'jev' and args.samples is not None:
        from .application.modes import ModeInvalid, load_modes
        fixed = load_modes().get('jev').samples
        if args.samples != fixed:
            raise ModeInvalid(f'modes.toml fixes jev samples at {fixed}; edit it to change them')
    replica = args.command in REPLICA_COMMANDS
    request = ModeRequest(mode=args.command, baseline=args.variant if replica else args.baseline,
                          candidate=None if replica else args.candidate,
                          freeze=getattr(args, 'freeze', None), private_root=getattr(args, 'private_root', None),
                          record=getattr(args, 'record', False), out=args.out)
    summary, paths, _ = run_mode(request)
    print(json.dumps({'summary': str(paths[0]), 'markdown': str(paths[1]),
                      'verdict': summary['verdict']['value'], 'total_s': summary['timings']['total_s'],
                      'within_budget': summary['timings']['within_budget'],
                      'sections': {s['name']: {'status': s['status'], 'verdict': s['verdict'],
                                               'layout': s['layout'], 'report_key': s['report_key']}
                                   for s in summary['sections']}}, indent=1, ensure_ascii=False))
    return 0 if summary['verdict']['value'] not in ('captura_fallida',) else 1


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
    prepare = commands.add_parser('prepare', help='verify the pinned tiktoken assets (offline)')
    prepare.set_defaults(action=run_prepare)
    prepare.add_argument('--cache-dir', type=Path, help='default: TIKTOKEN_CACHE_DIR or tmp/memory-bench/tiktoken-cache')
    fetch = commands.add_parser('fetch', help='download locked public datasets and verify them (BT17)')
    fetch.set_defaults(action=run_fetch)
    fetch.add_argument('--dataset', action='append', required=True, help="lock name, or 'all'")
    fetch.add_argument('--verify-only', action='store_true', help='check what is present; download nothing')
    fetch.add_argument('--out', type=Path, help='cache root (default <repo>/tmp/memory-bench)')
    fetch.add_argument('--private-root', type=Path, help='root of private datasets (LoCoMo)')
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
    build = commands.add_parser('build', help='build synth-v1 stores and cache them')
    build.set_defaults(action=run_store_command)
    materialize = commands.add_parser('materialize', help='copy a cached store template into work/')
    materialize.set_defaults(action=run_store_command)
    for name, help_text in (('quick-a', 'phase A quick run (<= 10 min with the base cached)'),
                            ('full', 'every private question, the synth ladder, the judged corpora'),
                            ('scale', 'synth ladder 10^3-10^5 with the far trace cut (P11 objective)'),
                            ('scale-aa', 'A/A of scale: one binary against a fresh replica of itself'),
                            ('jev', 'Jev arms: samples with a fresh cassette or book each'),
                            ('aa', 'A/A control: a variant against a fresh replica of itself'),
                            ('ci', 'informative CI run: A/A on the public part of quick-a, no network, no key')):
        command = commands.add_parser(name, help=help_text)
        command.set_defaults(action=run_mode_command)
        if name in REPLICA_COMMANDS:
            command.add_argument('--variant', type=Path, required=True)
        else:
            command.add_argument('--candidate', type=Path, required=True)
            command.add_argument('--baseline', type=Path, default=DEFAULT_BASELINE)
        if name != 'ci':  # ci runs no private section
            command.add_argument('--freeze', type=Path, help='B-real freeze (default: latest under the private root)')
            command.add_argument('--private-root', type=Path, help='default: $MEMORY_BENCH_PRIVATE_ROOT')
        command.add_argument('--out', type=Path, help='public cache root (default <repo>/tmp/memory-bench)')
        if name == 'jev':
            command.add_argument('--samples', type=int, help='must equal modes.toml (3)')
            command.add_argument('--record', action='store_true',
                                 help='jev = "record" variants ask the real provider with TYPESAFE_API_KEY')
    from .application import report_cli
    report_cli.add_arguments(commands)  # compare, render (BT10)
    real = commands.add_parser('real', help='B-real private corpus: freeze, blind labeling, kappa, run (BT07)')
    real.set_defaults(action=lambda args: args.real_action(args))
    from .labeling.cli import add_commands as add_real_commands
    add_real_commands(real.add_subparsers(dest='real_command', required=True))
    cache = commands.add_parser('cache', help='list cached stores or remove abandoned staging')
    cache.set_defaults(action=run_store_command)
    cache.add_argument('operation', choices=('ls', 'gc'))
    from .application.build import add_arguments
    add_arguments(build, materialize, cache)
    mixed = commands.add_parser('mixed', help='mixed versions and multiprocess scenarios (BT18)')
    from .application.mixed_versions_cli import add_arguments as add_mixed_arguments
    add_mixed_arguments(mixed)
    jev_tail = commands.add_parser('jev-tail', help='Jev tail test through a local fault proxy (BT19)')
    from .application.jev_tail import add_arguments as add_tail_arguments
    add_tail_arguments(jev_tail)
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
