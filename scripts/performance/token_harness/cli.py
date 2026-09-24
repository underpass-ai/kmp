"""Token meter CLI: prepare (network), capture (local binary), verify, measure,
oracle, compare and render (offline).

Run from the repository root:
  python -m scripts.performance.token_harness prepare
  python -m scripts.performance.token_harness capture --binary BIN --variant baseline \
      --wake-contract kmp.wake_claim.v1 --out DIR
  python -m scripts.performance.token_harness verify --run DIR
  python -m scripts.performance.token_harness measure --run DIR --out FILE
  python -m scripts.performance.token_harness oracle --run DIR --out FILE
  python -m scripts.performance.token_harness compare --baseline-run DIR --baseline-metrics FILE \
      --baseline-oracle FILE --candidate-run DIR ... --out report.json
  python -m scripts.performance.token_harness render --report report.json --out report.md
"""
import argparse
import json
import os
from pathlib import Path
import sys

from .adapters import tiktoken_counter
from .application.measure import measure_journey
from .application.provenance import header
from .application.verify import verify
from .capture.manifest import DEFAULT_MAX_FILE_BYTES
from .domain.accounting import DEFAULT_MAX_UNIT_BYTES
from .domain.errors import HarnessError
from .domain.representation import RepresentationId

ROOT = Path(__file__).resolve().parents[3]
ENCODINGS = tuple(tiktoken_counter.PINNED_ASSETS)


def emit(value, out=None):
    text = json.dumps(value, ensure_ascii=False, indent=2) + '\n'
    if out is None:
        sys.stdout.write(text)
        return
    with Path(out).open('x', encoding='utf-8') as target:  # never overwrite evidence
        target.write(text)


def cache_dir(args):
    return args.cache_dir or os.environ.get('TIKTOKEN_CACHE_DIR') or ROOT / 'tmp/tiktoken-cache'


def prepare(args):
    directory = tiktoken_counter.configure_cache(cache_dir(args))
    directory.mkdir(parents=True, exist_ok=True)
    emit({'cache_dir': str(directory), 'library': tiktoken_counter.LIBRARY,
          'library_version': tiktoken_counter.PINNED_VERSION,
          'assets': tiktoken_counter.prepare_assets(args.encoding or ENCODINGS)}, args.out)
    return 0


def run_verify(args):
    _, _, report = verify(args.run, args.max_file_bytes)
    emit(report, args.out)
    return 0 if report['ok'] else 1


def run_measure(args):
    if Path(args.out).exists():
        raise SystemExit(f'Refusing to overwrite {args.out}')
    tiktoken_counter.configure_cache(cache_dir(args))
    counters = [tiktoken_counter.load_counter(name) for name in args.encoding or ENCODINGS]
    representations = list(RepresentationId)
    _, traces, verification = verify(args.run, args.max_file_bytes)
    journeys = [measure_journey(trace, counters, representations, args.max_unit_bytes)
                for trace in traces]
    emit({**header(counters, representations, verification),
          'failures': verification['failures'], 'journeys': journeys}, args.out)
    print(json.dumps({'journeys': len(journeys), 'failures': len(verification['failures'])}))
    return 0 if verification['ok'] else 1


def run_capture(args):
    from .native.capture import capture
    from .scenarios import BY_CASE, SCENARIOS
    scenarios = [BY_CASE[case] for case in args.case] if args.case else list(SCENARIOS)
    provenance = json.loads(args.build_provenance.read_text()) if args.build_provenance else None
    manifest = capture(args.binary, args.variant, args.wake_contract, args.out, args.work_dir,
                       scenarios, provenance)
    statuses = [row['driver_status'] for row in manifest['journeys']]
    print(json.dumps({'journeys': len(statuses), 'completed': statuses.count('completed')}))
    return 0


def run_oracle(args):
    from .oracle.evaluate import evaluate_run
    result = evaluate_run(args.run, args.max_file_bytes)
    emit(result, args.out)
    passed = sum(1 for j in result['journeys'] if j['quality_pass'])
    print(json.dumps({'journeys': len(result['journeys']), 'quality_pass': passed}))
    return 0


def run_compare(args):
    from .compare.report import compare_runs
    sides = {side: (getattr(args, side + '_run'), getattr(args, side + '_metrics'),
                    getattr(args, side + '_oracle')) for side in ('baseline', 'candidate')}
    report = compare_runs(sides['baseline'], sides['candidate'], args.max_file_bytes, args.label)
    emit(report, args.out)
    print(json.dumps(report['summary']['conclusions']))
    return 0


def run_render(args):
    from .compare.inputs import read_json
    from .compare.markdown import render
    report = read_json(args.report)
    controls = [(path.name, read_json(path)) for path in args.control or []]
    with Path(args.out).open('x', encoding='utf-8') as target:  # never overwrite
        target.write(render(report, args.report.name, controls))
    return 0


def _add_offline(commands):
    import tempfile
    command = commands.add_parser('capture', help='run the synthetic scenarios against one binary')
    command.set_defaults(action=run_capture)
    command.add_argument('--binary', type=Path, required=True)
    command.add_argument('--variant', choices=('baseline', 'candidate'), required=True)
    command.add_argument('--wake-contract', required=True,
                         choices=('kmp.wake_claim.v1', 'kmp.wake_claim.v2'))
    command.add_argument('--out', type=Path, required=True)
    command.add_argument('--work-dir', type=Path,
                         default=Path(tempfile.gettempdir()) / 'kmp-token-harness-stores')
    command.add_argument('--build-provenance', type=Path)
    command.add_argument('--case', action='append', help='repeatable; default: every scenario')
    command = commands.add_parser('oracle', help='judge a verified native capture')
    command.set_defaults(action=run_oracle)
    command.add_argument('--run', type=Path, required=True)
    command.add_argument('--out', type=Path)
    command.add_argument('--max-file-bytes', type=int, default=DEFAULT_MAX_FILE_BYTES)
    command = commands.add_parser('compare', help='paired baseline/candidate report')
    command.set_defaults(action=run_compare)
    for side in ('baseline', 'candidate'):
        for part in ('run', 'metrics', 'oracle'):
            command.add_argument(f'--{side}-{part}', type=Path, required=True)
    command.add_argument('--label', default='baseline-vs-candidate')
    command.add_argument('--out', type=Path)
    command.add_argument('--max-file-bytes', type=int, default=DEFAULT_MAX_FILE_BYTES)
    command = commands.add_parser('render', help='Markdown generated from report.json')
    command.set_defaults(action=run_render)
    command.add_argument('--report', type=Path, required=True)
    command.add_argument('--control', type=Path, action='append', help='A/A report(s) to summarize')
    command.add_argument('--out', type=Path, required=True)


def main(argv=None):
    parser = argparse.ArgumentParser(prog='token_harness', description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    for name, action in (('prepare', prepare), ('verify', run_verify), ('measure', run_measure)):
        command = commands.add_parser(name)
        command.set_defaults(action=action)
        if name != 'verify':
            command.add_argument('--cache-dir', type=Path,
                                 help='tiktoken cache; default $TIKTOKEN_CACHE_DIR or tmp/tiktoken-cache')
            command.add_argument('--encoding', action='append', choices=ENCODINGS,
                                 help='repeatable; default: all pinned encodings')
        if name != 'prepare':
            command.add_argument('--run', type=Path, required=True,
                                 help='journey directory written by acceptance_surface_journeys.py')
            command.add_argument('--max-file-bytes', type=int, default=DEFAULT_MAX_FILE_BYTES)
        if name == 'measure':
            command.add_argument('--out', type=Path, required=True)
            command.add_argument('--max-unit-bytes', type=int, default=DEFAULT_MAX_UNIT_BYTES)
        else:
            command.add_argument('--out', type=Path, help='write JSON here instead of stdout')
    _add_offline(commands)
    args = parser.parse_args(argv)
    try:
        return args.action(args)
    except HarnessError as error:
        print(str(error), file=sys.stderr)
        return 2
