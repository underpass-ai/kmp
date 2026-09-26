"""The repository's judged corpora in a mode: retrieval (35 + 18 cases) and Jev (replay).

Each arm runs the ported scorecards over MCP stdio on its own binary
(`corpora/judged_retrieval.py`, `corpora/judged_jev.py`), one fresh store per case
and arm, and gets the scorecard's rows. Rows are cached per (binary, variant
configuration, corpus, arm, fixture files) under `reports/<key>/judged.json` of
the public cache, so a cached baseline costs nothing.

What each arm runs with:

- `plain` retrieval: the variant's store files and environment on top of the
  judged lexical bridge (`KMP_LEXICAL_BRIDGE`, unless the variant names its own).
  This is where a candidate's own configuration (an ask gate, a reranker) shows.
- `narrow` / `wide` retrieval and the Jev corpus: the corpus's own store files and
  cassette, in replay; the variant's store files are left out (the corpus pins
  them), its binary and remaining environment are kept.

Store files must be acknowledged by the binary (`kmp_store_config`). A binary that
predates that telemetry (v0.23.0) cannot acknowledge anything: its stores are
counted as `unverified` (a declared limitation) and the corpus's own behavioural
checks still apply (a rerank that did not run, a judgement missing from the
cassette, fail the corpus). Any other refusal fails the corpus.

The comparison is row by row: the candidate's value against the baseline's, and
each arm against the recorded baseline TSV (floors may rise, `false_*` ceilings may
fall, counts are exact). A row the baseline holds and the candidate breaks is a
regression of the section.
"""
from contextlib import contextmanager
from dataclasses import dataclass, field
from datetime import datetime, timezone
import json
from pathlib import Path
import shutil
import time

from .. import BENCH_VERSION
from ..corpora import judged_jev, judged_retrieval
from ..corpora.judged_stdio import StdioStores, StoreFileNotApplied
from ..domain import cachekey
from ..domain.errors import BenchError
from ..domain.jsonl import REPO_ROOT
from ..runtime.jev_fixture import JEV_ENV
from ..runtime.server_log import PREDATES_STORE_CONFIG
from . import sections

SCHEMA = 'kmp.bench.judged.v1'
FILE = 'judged.json'
TOLERANCE = 1e-4 + 1e-9  # the recorded TSVs keep 4 decimals (judged_retrieval.compare_to_recorded)
EXACT = ('cases', 'all_positives', 'guarded_cases')
RECORDED = {('retrieval', 'plain'): judged_retrieval.DEFAULT_BASELINE,
            ('jev', 'replay'): judged_jev.DEFAULT_BASELINE}


def bound(name):
    """How a recorded row binds: counts are exact, `false_*` rows are ceilings, the rest floors."""
    if name in EXACT:
        return 'exact'
    return 'ceiling' if 'false_' in name else 'floor'


def holds(name, value, recorded):
    kind = bound(name)
    if kind == 'exact':
        return abs(value - recorded) <= TOLERANCE
    return value <= recorded + TOLERANCE if kind == 'ceiling' else value >= recorded - TOLERANCE


@dataclass
class AckStores:
    """StdioStores that adds the variant's files and tolerates a binary that cannot acknowledge."""
    inner: StdioStores
    extra_files: tuple = ()
    unverified: list = field(default_factory=list)

    @contextmanager
    def open(self, label, store_files=()):
        names = {item.name for item in store_files}
        files = tuple(store_files) + tuple(f for f in self.extra_files if f.name not in names)
        try:
            with self.inner.open(label, files) as server:
                yield server
        except StoreFileNotApplied:
            acks = self.inner.reports[-1]['store_files']
            reasons = {ack['reason'] for ack in acks if ack['status'] != 'applied'}
            if reasons != {PREDATES_STORE_CONFIG}:
                raise
            self.unverified.append(label)


@dataclass(frozen=True)
class CorpusArm:
    corpus: str  # retrieval | jev
    arm: str  # plain | narrow | wide | replay

    @property
    def name(self):
        return f'{self.corpus}-{self.arm}'


def _sha(path):
    return cachekey.sha256_hex(Path(path).read_bytes())


def _env(spec, arm, repo_root):
    env = {'KMP_LEXICAL_BRIDGE': str(Path(repo_root) / judged_retrieval.DEFAULT_BRIDGE)}
    variant_env = spec.variant.env_map()
    if arm.arm == 'plain':
        env.update(variant_env)
        if 'KMP_TYPESAFE_CASSETTE' in variant_env:
            env['KMP_TYPESAFE_CASSETTE_MODE'] = 'replay'
        return env
    env.update({k: v for k, v in variant_env.items() if k not in JEV_ENV})
    cassette = judged_jev.DEFAULT_CASSETTE if arm.corpus == 'jev' else judged_retrieval.DEFAULT_ARM_CASSETTE
    env.update({'KMP_TYPESAFE_CASSETTE': str(Path(repo_root) / cassette), 'KMP_TYPESAFE_CASSETTE_MODE': 'replay'})
    return env


def _cases_path(arm, repo_root):
    return Path(repo_root) / (judged_jev.DEFAULT_CASES if arm.corpus == 'jev' else judged_retrieval.DEFAULT_CASES)


def cache_key(spec, arm, env, extra_files, repo_root):
    """Binary, the corpus fixture files and every input that changes what the arm measures."""
    material = {'binary_sha256': spec.binary.sha256, 'corpus': arm.corpus, 'arm': arm.arm,
                'cases_sha256': _sha(_cases_path(arm, repo_root)),
                'env': {k: (_sha(v) if k in ('KMP_LEXICAL_BRIDGE', 'KMP_TYPESAFE_CASSETTE') and Path(v).is_file()
                            else v) for k, v in sorted(env.items())},
                'store_files': {f.name: f.sha256 for f in extra_files}}
    return cachekey.digest({'kind': 'judged', 'bench_version': BENCH_VERSION, 'material': material})


def _measure(spec, arm, env, extra_files, work, repo_root):
    stores = AckStores(StdioStores(spec.binary.path, work / 'stores', work / 'traces', env), extra_files)
    if arm.corpus == 'jev':
        scores = judged_jev.run_collection(stores, judged_jev.load_cases(_cases_path(arm, repo_root)))
        rows = scores.row_map()
    else:
        rerank = None if arm.arm == 'plain' else arm.arm
        scored = judged_retrieval.run_collection(stores, judged_retrieval.load_cases(_cases_path(arm, repo_root)),
                                                 rerank=rerank)
        rows = scored.row_map()
    return rows, len(stores.unverified), len(stores.inner.reports)


def measure_arm(spec, arm, layout, repo_root=REPO_ROOT, use_cache=True):
    """{rows, unverified_stores, stores, cached, seconds}: the arm's scorecard rows.

    Cached under `reports/<key>/judged.json`; `use_cache=False` (an A/A replica)
    measures now and writes nothing."""
    env = _env(spec, arm, repo_root)
    extra_files = tuple(spec.variant.store_files) if arm.arm == 'plain' else ()
    key = cache_key(spec, arm, env, extra_files, repo_root)
    target = layout.report(key) / FILE
    if use_cache and target.is_file():
        return {**json.loads(target.read_text(encoding='utf-8')), 'cached': True}
    work = layout.require_inside(layout.scratch() / f'judged-{key[:16]}{"" if use_cache else "-replica"}')
    started = time.perf_counter()
    try:
        rows, unverified, stores = _measure(spec, arm, env, extra_files, work, repo_root)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    record = {'schema': SCHEMA, 'key': key, 'corpus': arm.corpus, 'arm': arm.arm,
              'variant': spec.name, 'binary_sha256': spec.binary.sha256,
              'config_digest': spec.variant.config_digest(), 'rows': rows,
              'unverified_stores': unverified, 'stores': stores,
              'seconds': round(time.perf_counter() - started, 3),
              'created_at': datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')}
    if use_cache:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(cachekey.canonical_json(record) + '\n', encoding='utf-8')
    return {**record, 'cached': False}


def recorded_check(arm, rows, repo_root):
    path = RECORDED.get((arm.corpus, arm.arm))
    if path is None:
        return None
    recorded = judged_retrieval.parse_baseline((Path(repo_root) / path).read_text(encoding='utf-8'))
    failing = sorted(name for name, value in recorded.items()
                     if name in rows and not holds(name, rows[name], value))
    # Rows only the Rust scorecard computes (jev_kmp_scorecard's write, label and summary
    # columns) are not ported: they are counted, not failed (corpora/judged_jev.py).
    not_measured = sorted(name for name in recorded if name not in rows)
    return {'file': path, 'rows': len(recorded) - len(not_measured), 'failing': failing,
            'not_measured': len(not_measured), 'holds': not failing}


def compare(arm, base, cand, repo_root):
    names = sorted(set(base['rows']) & set(cand['rows']))
    deltas = {name: cand['rows'][name] - base['rows'][name] for name in names}
    worse = sorted(name for name in names if bound(name) == 'floor' and deltas[name] < -TOLERANCE
                   or bound(name) == 'ceiling' and deltas[name] > TOLERANCE
                   or bound(name) == 'exact' and abs(deltas[name]) > TOLERANCE)
    checks = {'baseline': recorded_check(arm, base['rows'], repo_root),
              'candidate': recorded_check(arm, cand['rows'], repo_root)}
    broken = []
    if checks['baseline'] and checks['candidate']:
        broken = sorted(set(checks['candidate']['failing']) - set(checks['baseline']['failing']))
    return {'arm': arm.name, 'baseline': base['rows'], 'candidate': cand['rows'], 'deltas': deltas,
            'worse': worse, 'recorded': checks, 'breaks_recorded': broken,
            'unverified_stores': {'baseline': base['unverified_stores'], 'candidate': cand['unverified_stores']},
            'cached': {'baseline': base['cached'], 'candidate': cand['cached']}}


def _verdict(rows, unverified):
    broken = [f'{row["arm"]}: {name}' for row in rows for name in row['breaks_recorded']]
    if broken:
        return 'regresion', ['candidate breaks a recorded row the baseline holds: ' + ', '.join(broken)]
    worse = [f'{row["arm"]}: {name}' for row in rows for name in row['worse']]
    moved = [f'{row["arm"]}: {name}' for row in rows for name, value in row['deltas'].items()
             if abs(value) > TOLERANCE]
    reasons = []
    if worse:
        reasons.append('rows worse than the baseline (inside recorded bounds): ' + ', '.join(worse))
    if unverified:
        reasons.append(f'{unverified} store(s) whose files the binary could not acknowledge (predates telemetry)')
    if not moved:
        return 'neutral', reasons + ['every row equal on both arms']
    return 'neutral', reasons + ['rows moved: ' + ', '.join(moved)]


def run_corpus(name, specs, arms, layout, repo_root=REPO_ROOT, replica=False):
    """One judged corpus: its arms on both variants, compared (a replica re-measures, uncached)."""
    started = time.perf_counter()
    rows = []
    try:
        for arm in arms:
            base = measure_arm(specs[0], arm, layout, repo_root)
            cand = measure_arm(specs[1], arm, layout, repo_root, use_cache=not replica)
            rows.append(compare(arm, base, cand, repo_root))
    except BenchError as error:
        return sections.failed(name, error, time.perf_counter() - started)
    unverified = sum(sum(row['unverified_stores'].values()) for row in rows)
    verdict, reasons = _verdict(rows, unverified)
    result = sections.SectionResult(name, sections.RAN, layout='public', verdict=verdict, reasons=reasons,
                                    applicable=verdict == 'regresion',
                                    limitations=[r for r in reasons if 'acknowledge' in r], judged=rows)
    result.seconds = time.perf_counter() - started
    return result


CORPORA = ('retrieval-judged', 'jev-judged')


def run(name, specs, mode, layout, repo_root=REPO_ROOT, replica=False):
    """The judged corpus `name` of the mode (skipped when the mode names no arm of it)."""
    if name == 'retrieval-judged':
        arms = [CorpusArm('retrieval', arm) for arm in mode.judged.retrieval_arms]
    else:
        arms = [CorpusArm('jev', 'replay')] if mode.judged.jev else []
    if not arms:
        return sections.skipped(name, 'the mode runs no arm of this corpus')
    return run_corpus(name, specs, arms, layout, repo_root, replica)
