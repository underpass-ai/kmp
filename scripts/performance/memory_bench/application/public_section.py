"""The `public` section of a mode: evidence recall on the public benchmarks, per arm (BT17).

`modes.toml` names the corpora (`[modes.<mode>.public]`). For each one the section
adapts the fetched dataset (`public_cli.load_corpora`: files verified against the
lock, never downloaded here), has the baseline's binary write it once and each arm
import that bundle with its own binary (`public_build.build_public`, cached like a
synth store), asks the corpus questions with the arm's variant
(`public_run.run_public`) and keeps the `kmp.bench.public_recall.v1` summary of each
arm next to the cache (`public_cli.write_recall_report`).

A corpus whose dataset is not fetched is skipped with the fetch command as its
reason; the section is skipped only when every corpus was. The section is
descriptive: it carries each arm's headline and the candidate-minus-baseline Δ of
every figure both arms measured, and does not vote on the mode's verdict (no
pre-registered target or guard is measured here; evidence recall is not QA
accuracy). A private corpus (LoCoMo) is built and summarized in the private layout.
"""
from pathlib import Path
from types import SimpleNamespace
import time

from ..corpora.public_errors import FetchFailed, LicenceRefused
from ..domain.errors import BenchError
from ..domain.jsonl import REPO_ROOT
from ..runtime import binaries
from ..runtime.layout import private_layout
from . import build as synth_build
from . import public_build, public_cli, public_run, sections

NAME = 'public'
NOT_FETCHED = 'dataset not fetched: python3 -m scripts.performance.memory_bench fetch --dataset {lock}'
LOCK_NAMES = {'factconsolidation': 'memoryagentbench-cr', 'longmemeval-s': 'longmemeval-s',
              'musique': 'musique', '2wiki': '2wiki', 'locomo': 'locomo'}


def corpus_args(public, corpus, out, private_root):
    """The argument namespace `public_cli.load_corpora` reads, from the mode's settings."""
    return SimpleNamespace(corpus=corpus, size=tuple(public.sizes), limit=None, per_type=public.per_type,
                           abstention=public.abstention, setup=public.setup, adversarial=False,
                           out=out, private_root=private_root)


def _variant_path(spec):
    path = Path(spec.path)
    return path if path.is_absolute() else REPO_ROOT / path


def delta(base, cand):
    """Candidate minus baseline for every number both headlines hold (nested strata too)."""
    out = {}
    for name in sorted(set(base) & set(cand)):
        a, b = base[name], cand[name]
        if isinstance(a, dict) and isinstance(b, dict):
            inner = delta(a, b)
            if inner:
                out[name] = inner
        elif isinstance(a, (int, float)) and isinstance(b, (int, float)) \
                and not isinstance(a, bool) and not isinstance(b, bool):
            out[name] = round(b - a, 4)
    return out


def run_corpus(corpus, entry, specs, mode, layout, log=None):
    """{arm: {run_id, cached, headline}} of one adapted corpus."""
    writer = binaries.KmpBinary.locate(specs[0].binary.path)
    arms = {}
    for label, spec in zip(('baseline', 'candidate'), specs):
        reader = binaries.KmpBinary.locate(spec.binary.path)
        started = time.perf_counter()
        built = public_build.build_public(corpus, entry, writer, layout, reader, public_build.DEFAULT_BATCH,
                                          synth_build.BUILD_CPUS, synth_build.BUILD_TIMEOUT_S)
        load = {'timings': public_build.timings(built), 'elapsed_s': round(time.perf_counter() - started, 3)}
        started = time.perf_counter()
        result, run = public_run.run_public(corpus, built, layout, _variant_path(spec), spec.binary.path,
                                            mode.public.max_calls, mode.cpus, timeout_s=mode.timeout_s)
        report = public_cli.recall_report(corpus, reader.sha256, spec.path, mode.public.max_calls, result, run,
                                          load, time.perf_counter() - started)
        public_cli.write_recall_report(layout, corpus, report)
        arms[label] = {'run_id': result.run_id, 'cached': result.cached,
                       'headline': public_cli.headline(report['recall'])}
    return arms


def run(specs, mode, out_layout, private_root=None):
    """The section's SectionResult; one corpus failing fails the section, a missing one is skipped."""
    started = time.perf_counter()
    rows, private = [], False
    try:
        for name in mode.public.corpora:
            args = corpus_args(mode.public, name, out_layout.root, private_root)
            try:
                adapted = public_cli.load_corpora(args)
            except FetchFailed as missing:
                rows.append({'corpus': name, 'status': sections.SKIPPED,
                             'reason': NOT_FETCHED.format(lock=LOCK_NAMES[name]) + f' ({missing})'})
                continue
            except LicenceRefused as refused:  # a private dataset without a private root
                rows.append({'corpus': name, 'status': sections.SKIPPED, 'reason': str(refused)})
                continue
            for corpus, entry in adapted:
                layout = private_layout(private_root) if corpus.private else out_layout
                private = private or corpus.private
                arms = run_corpus(corpus, entry, specs, mode, layout)
                rows.append({'corpus': corpus.name, 'status': sections.RAN, 'reason': None,
                             'questions': len(corpus.questions), 'arms': arms,
                             'delta': delta(arms['baseline']['headline'], arms['candidate']['headline'])})
    except BenchError as error:
        return sections.failed(NAME, error, time.perf_counter() - started)
    ran = [row for row in rows if row['status'] == sections.RAN]
    seconds = time.perf_counter() - started
    if not ran:
        reasons = '; '.join(f'{row["corpus"]}: {row["reason"]}' for row in rows)
        result = sections.skipped(NAME, reasons or 'no public corpus configured')
        result.public, result.seconds = rows, seconds
        return result
    return sections.SectionResult(
        NAME, sections.RAN, layout='private' if private else 'public',
        runs={row['corpus']: {arm: {'run_id': v['run_id'], 'cached': v['cached']} for arm, v in row['arms'].items()}
              for row in ran},
        questions={row['corpus']: row['questions'] for row in ran}, verdict=None, applicable=False,
        limitations=['public benchmarks: evidence recall without a reader, not QA accuracy; MuSiQue/2Wiki '
                     'subsets give internal deltas only (the HippoRAG 2 setup is `public_cli run --setup full`)',
                     'public benchmarks: each arm runs with its variant\'s store files and environment but '
                     'no per-arm Jev fixture (a jev variant uses only the cassette its own environment names)'],
        seconds=seconds, public=rows)
