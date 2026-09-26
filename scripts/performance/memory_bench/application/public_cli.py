"""CLI of the public corpora (BT17): fetch, build, run, and the HippoRAG 2 full setup.

Run from the repository root:

  P='python3 -m scripts.performance.memory_bench.application.public_cli'

  $P fetch --dataset memoryagentbench-cr --dataset longmemeval-s --dataset musique \\
           --dataset 2wiki --dataset hipporag-question-ids
  $P fetch --dataset locomo --private-root ~/Documents/ai/artifacts/kmp-bench-private
  $P build --corpus factconsolidation --size 6k,32k
  $P run   --corpus factconsolidation --size 32k            # evidence recall, baseline variant
  $P run   --corpus longmemeval-s --per-type 10 --abstention 30
  $P run   --corpus musique --setup subset                  # 200 questions: internal deltas only
  $P run   --corpus 2wiki --setup subset
  $P run   --corpus locomo --private-root ~/Documents/ai/artifacts/kmp-bench-private

The HippoRAG 2 setup (the only one comparable with published MuSiQue/2Wiki figures:
1,000 questions over 11,656 and 6,119 passages) is the same command with
`--setup full`; `$P plan-full` prints it with a time estimate scaled from the
subset runs already cached.

`fetch` downloads only what `corpora/datasets.lock.json` pins and verifies size and
SHA-256; `build` loads a corpus through the release binary into a cached store
(`application/public_build.py`); `run` asks its questions on that store with the
variant (default `variants/baseline.toml`) and writes the recall summary next to the
cache (`<cache>/public/<corpus>/summary-<run>.json`; private corpora under the private
root). This module is wired into `memory_bench fetch` by the CLI owner; it stands on
its own meanwhile.
"""
import argparse
import json
import os
from pathlib import Path
import sys
import time

from ..corpora import fetch as fetching
from ..corpora import locomo, longmemeval, memoryagentbench, musique_2wiki
from ..corpora.public_errors import CorpusError
from ..domain.errors import BenchError
from ..runtime import binaries
from ..runtime.layout import PRIVATE_ROOT_ENV, private_layout, public_layout
from . import build as synth_build
from . import public_build, public_run

CORPORA = ('factconsolidation', 'longmemeval-s', 'musique', '2wiki', 'locomo')
FULL_PASSAGES = {'musique': 11656, '2wiki': 6119}


def _sizes(text):
    sizes = tuple(item.strip() for item in text.split(',') if item.strip())
    unknown = set(sizes) - set(memoryagentbench.SIZES)
    if not sizes or unknown:
        raise argparse.ArgumentTypeError(f'sizes are among {", ".join(memoryagentbench.SIZES)}')
    return sizes


def load_corpora(args, lock=None, log=sys.stderr):
    """[(PublicCorpus, LockedDataset)] the arguments name; files verified, never downloaded here."""
    lock = lock or fetching.load_lock()
    started = time.perf_counter()
    if args.corpus == 'factconsolidation':
        entry, folder = fetching.locate('memoryagentbench-cr', args.out, lock=lock)
        rows = memoryagentbench.load_rows(folder / memoryagentbench.PARQUET)
        found = [(memoryagentbench.build(rows, size, limit=args.limit), entry) for size in args.size]
    elif args.corpus == 'longmemeval-s':
        entry, folder = fetching.locate('longmemeval-s', args.out, lock=lock)
        items = longmemeval.load_items(folder / longmemeval.FILE)
        found = [(longmemeval.build(items, args.per_type, args.abstention), entry)]
    elif args.corpus in musique_2wiki.DATASETS:
        spec = musique_2wiki.DATASETS[args.corpus]
        entry, folder = fetching.locate(spec['lock'], args.out, lock=lock)
        _, ids_folder = fetching.locate('hipporag-question-ids', args.out, lock=lock)
        rows = musique_2wiki.read_archive_rows(folder / spec['archive'], spec['member'])
        selection = musique_2wiki.read_selection(ids_folder / spec['ids'], spec['id_field'])
        limit = None if args.setup == 'full' else args.limit or musique_2wiki.SUBSET
        found = [(musique_2wiki.build(args.corpus, rows, selection, limit), entry)]
    elif args.corpus == 'locomo':
        entry, folder = fetching.locate('locomo', private_root=args.private_root, lock=lock)
        found = [(locomo.build(locomo.load_samples(folder / locomo.FILE), args.adversarial), entry)]
    else:
        raise CorpusError(f'unknown corpus {args.corpus!r}')
    print(f'public: adapted {args.corpus} in {time.perf_counter() - started:.2f}s', file=log, flush=True)
    return found


def layout_for(corpus, args):
    return private_layout(args.private_root) if corpus.private else public_layout(args.out)


def summary_dir(layout, corpus):
    return layout.root / 'public' / corpus.name


def _corpus_row(corpus):
    return {'corpus': corpus.name, 'dataset': corpus.dataset, 'abouts': len(corpus.abouts),
            'entries': corpus.entry_count(), 'questions': len(corpus.questions),
            'questions_digest': corpus.questions_digest(), 'selection': corpus.selection
            if not corpus.private else '(private)', 'notes': corpus.notes}


def cmd_fetch(args):
    lock = fetching.load_lock()
    names = sorted(lock) if 'all' in args.dataset else args.dataset
    rows = []
    for name in names:
        entry = lock[name]
        if not entry.files:
            rows.append({'dataset': name, 'status': 'not fetched (no file locked)', 'licence': entry.spdx})
            continue
        if entry.storage == 'private' and not (args.private_root or os.environ.get(PRIVATE_ROOT_ENV)):
            rows.append({'dataset': name, 'status': 'skipped: private, no private root', 'licence': entry.spdx})
            continue
        folder = fetching.dataset_dir(entry, args.out, args.private_root)
        rows.extend(fetching.fetch(entry, folder, download=not args.verify_only))
    print(json.dumps(rows, indent=1))
    return 0


def _build(args, corpus, entry, writer, reader):
    layout = layout_for(corpus, args)
    started = time.perf_counter()
    built = public_build.build_public(corpus, entry, writer, layout, reader, args.batch_size,
                                      None if args.cpus == 'none' else args.cpus, args.timeout)
    row = {**_corpus_row(corpus), 'built': built['built'], 'elapsed_s': round(time.perf_counter() - started, 3),
           'timings': public_build.timings(built)}
    return layout, built, row


def cmd_build(args):
    writer = binaries.KmpBinary.locate(args.binary)
    reader = binaries.KmpBinary.locate(args.reader) if args.reader else writer
    rows = [_build(args, corpus, entry, writer, reader)[2] for corpus, entry in load_corpora(args)]
    print(json.dumps(rows, indent=1, sort_keys=True))
    return 0


def _journey_wall(run):
    walls = {}
    for call in run.calls:
        if call.sample == 0 and call.repeat == 0 and call.wall_ns is not None:
            walls[call.question_id] = walls.get(call.question_id, 0) + call.wall_ns
    values = sorted(walls.values())
    if not values:
        return None
    return {'n': len(values), 'mean_ms': round(sum(values) / len(values) / 1e6, 3),
            'p50_ms': round(values[len(values) // 2] / 1e6, 3), 'max_ms': round(values[-1] / 1e6, 3),
            'total_s': round(sum(values) / 1e9, 3)}


def cmd_run(args):
    writer = binaries.KmpBinary.locate(args.binary)
    out = []
    for corpus, entry in load_corpora(args):
        layout, built, row = _build(args, corpus, entry, writer, writer)
        started = time.perf_counter()
        result, run = public_run.run_public(corpus, built, layout, args.variant, writer.path, args.max_calls,
                                            None if args.run_cpus == 'none' else args.run_cpus,
                                            timeout_s=args.timeout)
        summary = public_run.recall_summary(corpus, run)
        report = {'schema': 'kmp.bench.public_recall.v1', 'corpus': _corpus_row(corpus),
                  'binary_sha256': writer.sha256, 'variant': str(args.variant), 'max_calls': args.max_calls,
                  'run': {'run_id': result.run_id, 'cached': result.cached, 'dir': str(result.run_dir),
                          'elapsed_s': round(time.perf_counter() - started, 3),
                          'journey_wall': _journey_wall(run),
                          'failures': result.manifest.get('failures') or []},
                  'load': row['timings'], 'load_elapsed_s': row['elapsed_s'], 'recall': summary,
                  'scope': 'evidence recall without a reader; QA accuracy is not measured here'}
        target = summary_dir(layout, corpus) / f'summary-{result.run_id[:16]}.json'
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(report, indent=1, sort_keys=True, ensure_ascii=False) + '\n', encoding='utf-8')
        out.append({'corpus': corpus.name, 'summary': str(target), 'run_id': result.run_id,
                    'headline': headline(summary), 'load_ms': row['timings'], 'run': report['run']})
    print(json.dumps(out, indent=1, sort_keys=True))
    return 0


HEADLINE = ('recall_at_1', 'recall_at_2', 'recall_at_5', 'recall_at_10', 'mrr', 'full_chain_at_2',
            'full_chain_at_5', 'full_chain_at_10', 'session_recall_at_5', 'stale_served',
            'stale_before_current', 'successor_rescued', 'abstained', 'evidence_hit', 'unknown')


def headline(summary):
    rows = {}
    for name, strata in summary['corpora'].items():
        overall = strata.get('all', {})
        rows[name] = {'questions': overall.get('questions')}
        for metric in HEADLINE:
            value = overall.get(metric)
            if isinstance(value, dict) and 'value' in value:
                rows[name][metric] = None if value['value'] is None else round(value['value'], 4)
            elif value is not None:
                rows[name][metric] = value
        for stratum in ('abstention', 'type:current_after_supersession'):
            if stratum in strata:
                rows[name][stratum] = {m: round(strata[stratum][m]['value'], 4)
                                       for m in ('abstained', 'stale_served', 'stale_before_current',
                                                 'recall_at_1', 'unknown')
                                       if isinstance(strata[stratum].get(m), dict)
                                       and strata[stratum][m].get('value') is not None}
    return rows


def cmd_plan_full(args):
    """The HippoRAG 2 full-setup commands, with a time estimate scaled from cached subset runs."""
    layout = public_layout(args.out)
    rows = []
    for name in ('musique', '2wiki'):
        found = sorted((layout.root / 'public' / f'{name}-hipporag{musique_2wiki.SUBSET}').glob('summary-*.json'))
        command = (f'python3 -m scripts.performance.memory_bench.application.public_cli run '
                   f'--corpus {name} --setup full')
        row = {'corpus': name, 'command': command, 'questions': musique_2wiki.FULL_QUESTIONS,
               'passages': FULL_PASSAGES[name]}
        if found:
            subset = json.loads(found[-1].read_text(encoding='utf-8'))
            wall, load = subset['run']['journey_wall'], subset['load']
            factor = FULL_PASSAGES[name] / load['entries']
            # Ask cost grows with the about (linear bound; BT14 measured O(N) re-reads) and with n.
            ask_s = wall['mean_ms'] / 1e3 * musique_2wiki.FULL_QUESTIONS * factor
            ingest_s = (load['ingest_ms'] or 0) / 1e3 * factor ** 2  # O(N^2/B) build, B=5000
            row.update(estimate_s={'ask_upper': round(ask_s), 'ask_if_flat': round(
                wall['mean_ms'] / 1e3 * musique_2wiki.FULL_QUESTIONS), 'ingest_upper': round(ingest_s, 1)},
                from_subset={'summary': str(found[-1]), 'entries': load['entries'],
                             'mean_ask_ms': wall['mean_ms'], 'ingest_ms': load['ingest_ms']})
        else:
            row['estimate_s'] = None
            row['absent'] = f'run the subset first: ... run --corpus {name} --setup subset'
        rows.append(row)
    print(json.dumps(rows, indent=1))
    return 0


def build_parser():
    parser = argparse.ArgumentParser(prog='public_cli', description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    fetch = commands.add_parser('fetch', help='download locked datasets and verify them')
    fetch.set_defaults(action=cmd_fetch)
    fetch.add_argument('--dataset', action='append', required=True, help="lock name, or 'all'")
    fetch.add_argument('--verify-only', action='store_true', help='check what is present; download nothing')
    for name, action, text in (('build', cmd_build, 'load a corpus into a cached store'),
                               ('run', cmd_run, 'build if needed, ask the questions, summarise recall')):
        command = commands.add_parser(name, help=text)
        command.set_defaults(action=action)
        command.add_argument('--corpus', choices=CORPORA, required=True)
        command.add_argument('--size', type=_sizes, default=('32k',), help='FactConsolidation sizes')
        command.add_argument('--limit', type=int, help='questions per FC variant / multi-hop subset size')
        command.add_argument('--per-type', type=int, default=10, help='LongMemEval questions per type')
        command.add_argument('--abstention', type=int, default=30, help='LongMemEval _abs items')
        command.add_argument('--setup', choices=('subset', 'full'), default='subset',
                             help='MuSiQue/2Wiki: 200-question subset or the HippoRAG 2 setup')
        command.add_argument('--adversarial', action='store_true', help='LoCoMo: keep category 5')
        command.add_argument('--binary', type=Path, help='kmp-mcp (default target/release/kmp-mcp)')
        command.add_argument('--batch-size', type=int, default=public_build.DEFAULT_BATCH)
        command.add_argument('--cpus', default=synth_build.BUILD_CPUS, help="writer cores; 'none'")
        command.add_argument('--timeout', type=float, default=synth_build.BUILD_TIMEOUT_S)
    commands.choices['build'].add_argument('--reader', type=Path, help='reader kmp-mcp for the import')
    run = commands.choices['run']
    run.add_argument('--variant', type=Path, default=public_run.DEFAULT_VARIANT)
    run.add_argument('--max-calls', type=int, default=1,
                     help='pages per journey (1 = the first answer, as the Rust runners read it)')
    run.add_argument('--run-cpus', default='none', help="cores of the asking process; 'none' (default)")
    plan = commands.add_parser('plan-full', help='HippoRAG 2 full-setup commands and time estimate')
    plan.set_defaults(action=cmd_plan_full)
    for command in commands.choices.values():
        command.add_argument('--out', type=Path, help='cache root (default <repo>/tmp/memory-bench)')
        command.add_argument('--private-root', type=Path, help='root of private corpora (LoCoMo)')
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    try:
        return args.action(args)
    except BenchError as error:
        print(str(error), file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
