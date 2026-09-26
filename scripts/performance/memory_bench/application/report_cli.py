"""`compare` and `render` commands (BENCH_SPEC section 13).

  $TT compare --questions Q --baseline RUN [--candidate RUN] [--baseline-variant TOML]
              [--candidate-variant TOML] [--baseline-extra RUN@LEVEL ...] [--determinism DIR]
  $B  render --report reports/<key>/report.json [--out report.md]

`compare` scores the runs, compares them and writes report.json and report.md under
`reports/<report_key>/` of the cache (the private cache when a corpus is private).
Run it through `$TT` (tiktoken 0.14.0) so tokens are counted; under plain `python3`
every token figure is null with its reason. `render` rebuilds report.md from a
report.json and nothing else.
"""
import argparse
import json
from pathlib import Path
import sys

from ..domain.jsonl import REPO_ROOT
from ..domain.question import load_questions
from ..domain.variant import load_variant
from ..runtime.layout import private_layout, public_layout
from . import markdown, report as reports, tokens
from .run_data import load_run


def _extra(value):
    """RUN or RUN@LEVEL."""
    path, sep, level = value.rpartition('@')
    if not sep:
        return Path(value), None
    try:
        return Path(path), int(float(level))
    except ValueError:
        raise argparse.ArgumentTypeError(f'{value!r}: expected RUN@LEVEL') from None


def add_arguments(commands):
    compare = commands.add_parser('compare', help='score runs and compare them into report.json')
    compare.set_defaults(action=run_compare)
    compare.add_argument('--questions', type=Path, required=True)
    compare.add_argument('--baseline', type=Path, required=True, help='run directory')
    compare.add_argument('--candidate', type=Path, help='run directory (omit for one arm)')
    for arm in ('baseline', 'candidate'):
        compare.add_argument(f'--{arm}-variant', type=Path, help='variant TOML (pre-registration)')
        compare.add_argument(f'--{arm}-extra', type=_extra, action='append', default=[],
                             metavar='RUN[@LEVEL]', help='same variant: budget sweep or another level')
    compare.add_argument('--level', type=int, help='ladder level of the primary runs')
    compare.add_argument('--topology', choices=('mono', 'multi'))
    compare.add_argument('--mode', default='adhoc')
    compare.add_argument('--determinism', type=Path, help='determinism.py report or its directory')
    compare.add_argument('--world-manifest', type=Path, help='synth-v1 manifest.json (calibration)')
    compare.add_argument('--bootstrap-b', type=int, default=10_000)
    compare.add_argument('--latency-b', type=int)
    compare.add_argument('--out', type=Path, help='cache root (default: public or private layout)')
    compare.add_argument('--private-root', type=Path)
    render = commands.add_parser('render', help='report.md from report.json alone')
    render.set_defaults(action=run_render)
    render.add_argument('--report', type=Path, required=True)
    render.add_argument('--out', type=Path, help='default: report.md beside the report')
    return compare, render


def _arm(name, primary, extras, variant_path, level, topology):
    variant = load_variant(variant_path, REPO_ROOT) if variant_path else None
    runs = [reports.ArmRun(load_run(primary), level, topology)]
    runs += [reports.ArmRun(load_run(path), extra_level if extra_level is not None else level, topology)
             for path, extra_level in extras]
    return reports.Arm(name, tuple(runs), variant)


def run_compare(args):
    questions = load_questions(args.questions)
    topology = args.topology or reports.infer_topology(questions)
    baseline = _arm('baseline', args.baseline, args.baseline_extra, args.baseline_variant, args.level, topology)
    candidate = None
    if args.candidate:
        candidate = _arm('candidate', args.candidate, args.candidate_extra, args.candidate_variant,
                         args.level, topology)
    calibration = None
    if args.world_manifest:
        calibration = json.loads(args.world_manifest.read_text(encoding='utf-8')).get('calibration')
    counters, reason = tokens.try_load_counters()
    options = reports.ReportOptions(mode=args.mode, bootstrap_b=args.bootstrap_b, latency_b=args.latency_b,
                                    determinism=args.determinism, calibration=calibration)
    report = reports.build_report(baseline, candidate, questions, counters, options, reason)
    private = reports.mentions_private(report)
    layout = private_layout(args.private_root) if private else public_layout(args.out)
    paths = reports.write_report(report, layout, markdown.render(report))
    print(json.dumps({'report': str(paths[0]), 'verdict': report['verdict']['value'],
                      'reasons': report['verdict']['reasons']}, ensure_ascii=False))
    return 0


def run_render(args):
    report = json.loads(args.report.read_text(encoding='utf-8'))
    out = args.out or args.report.with_name('report.md')
    text = markdown.render(report)
    if out.exists() and out.read_text(encoding='utf-8') != text:
        print(f'refusing to overwrite {out}: it differs from what the report renders to', file=sys.stderr)
        return 2
    out.write_text(text, encoding='utf-8')
    print(str(out))
    return 0
