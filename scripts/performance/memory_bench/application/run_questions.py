"""Run a question set against one or two arms: ABAB blocks, warm-up, samples and repeats.

Each arm is one variant (binary, store files, environment) on one store. The
arms are interleaved in time, ABAB in blocks of `block` questions (BENCH_SPEC
section 10), so a drift of the machine lands on both arms alike. Per sample,
each arm gets one fresh process on a fresh copy of its store template, pinned
to the latency cores; `warmup` journeys (the first questions of the set) run
first and are never measured. Repeats of a question run back to back on the
same process (`phase = repeat`). A process whose call timed out or died is
replaced by a fresh one before the next journey.

Each arm's output is one run directory (SCHEMAS.md section 3): token_harness
traces sealed by token_harness' own `_seal`, plus run.json, calls.jsonl and
journeys.jsonl, so `token_harness verify|measure --run DIR` works on it. The
directory is assembled in the scratch area and moved to `runs/<run_id>` only
when complete; an existing run directory is a cache hit and is not rerun.
"""
from collections import Counter
from dataclasses import dataclass, field
from datetime import datetime, timezone
import itertools
import json
from pathlib import Path
import shutil

from ...token_harness.adapters.tiktoken_counter import PINNED_ASSETS
from ...token_harness.domain.errors import HarnessError
from ...token_harness.native.capture import _seal, sha256_file
from ...token_harness.native.driver import DRIVER_VERSION
from ...token_harness.native.transport import TIMEOUT_SECONDS
from .. import BENCH_VERSION
from ..domain import cachekey, jsonl
from ..domain.errors import BenchError
from ..domain.question import questions_digest
from ..domain.run_manifest import RunManifest
from ..domain.run_record import MAX_CALLS_LIMIT, RUN_SCHEMA
from ..runtime import probes
from ..runtime.driver import JourneyPlan, call_records, drive, journey_record, telemetry_bound
from ..runtime.session import BenchProcess, ProcessConfig

ORDER_SCHEME = 'ABAB'
DEFAULT_BLOCK = 10


class RunRefused(BenchError):
    code = 'RUN_REFUSED'


@dataclass(frozen=True)
class StoreRef:
    """A built store: its cache key, KMP content digest and template directory (BT14)."""
    key: str
    content_digest: str | None
    label: str
    template: Path | None  # the data directory copied into every process; None = empty store


@dataclass(frozen=True)
class Arm:
    variant: object  # domain.variant.Variant
    variant_path: str
    binary: Path
    store: StoreRef
    binary_version: str | None = None
    provenance: dict | None = None
    secrets: dict = field(default_factory=dict, repr=False)


@dataclass(frozen=True)
class RunPlan:
    questions: tuple
    mode: str
    samples: int = 1
    repeats: int = 1
    max_calls: int = MAX_CALLS_LIMIT
    timeout_s: float = float(TIMEOUT_SECONDS)
    max_bytes: int | None = None
    default_budget: dict | None = None
    block: int = DEFAULT_BLOCK
    warmup: int = 1
    cpus: str | None = probes.LATENCY_CPUS
    nonce: str | None = None
    probe_resources: bool = True

    def check(self):
        if not self.questions:
            raise RunRefused('no questions')
        for name in ('samples', 'repeats', 'block', 'max_calls'):
            value = getattr(self, name)
            if isinstance(value, bool) or not isinstance(value, int) or value < 1:
                raise RunRefused(f'{name} must be a positive integer')
        if self.max_calls > MAX_CALLS_LIMIT:
            raise RunRefused(f'max_calls above {MAX_CALLS_LIMIT}')
        if isinstance(self.warmup, bool) or not isinstance(self.warmup, int) or self.warmup < 0:
            raise RunRefused('warmup must be a non-negative integer')
        questions_digest(self.questions)  # refuses duplicate ids

    def blocks(self):
        return [self.questions[i:i + self.block] for i in range(0, len(self.questions), self.block)]


@dataclass(frozen=True)
class ArmResult:
    name: str
    run_id: str
    run_dir: Path
    cached: bool
    manifest: dict


def _now():
    return datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')


class _ArmRun:
    """Everything one arm accumulates until its run directory is sealed."""

    def __init__(self, arm, position, plan, layout, binary_sha):
        self.arm, self.position, self.plan, self.layout = arm, position, plan, layout
        self.binary_sha = binary_sha
        self.key = {'binary_sha256': binary_sha, 'config_digest': arm.variant.config_digest(),
                    'store_key': arm.store.key, 'questions_digest': questions_digest(plan.questions),
                    'mode': plan.mode, 'bench_version': BENCH_VERSION, 'nonce': plan.nonce}
        self.run_id = cachekey.result_key(**self.key)
        self.final = layout.run(self.run_id)
        self.staging = layout.require_inside(layout.scratch() / f'{self.run_id}.partial')
        self.identity = {'run_id': self.run_id, 'variant': arm.variant.name, 'binary_sha256': binary_sha}
        self.ids = itertools.count(1)
        self.process, self.processes = None, itertools.count(0)
        self.pending = []  # JourneyRun of the live process
        self.calls, self.journeys, self.rows, self.failures, self.reports = [], [], [], [], []
        self.isolation_env = None

    def config(self, pinning):
        return ProcessConfig(self.arm.binary, self.layout.scratch(), self.arm.variant.name,
                             env=self.arm.variant.env_map(), store_files=self.arm.variant.store_files,
                             template=self.arm.store.template, secrets=self.arm.secrets,
                             timeout_seconds=self.plan.timeout_s,
                             probe_resources=self.plan.probe_resources, pinning=pinning)

    def open(self, pinning, sample):
        index = next(self.processes)
        try:
            self.process = BenchProcess(self.config(pinning), self.staging, index, self.ids)
        except (HarnessError, OSError) as error:  # OSError: a child that died before the handshake
            self.process = None
            code = getattr(error, 'code', 'PROCESS_START_FAILED')
            detail = getattr(error, 'detail', None) or f'{type(error).__name__}: {error}'
            self.failures.append({'question_id': None, 'code': code,
                                  'detail': f'sample {sample} process {index}: {detail}'})
            return False
        self.isolation_env = self.isolation_env or self.process.store.allowlisted_env()
        self.sample = sample
        return True

    def close(self):
        if self.process is None:
            return
        report = self.process.close()
        self.reports.append({'sample': self.sample, **report.as_dict(),
                             'warmup_journeys': sum(1 for run in self.pending if run.warmup)})
        bound = telemetry_bound(report, self.pending)
        for run in self.pending:
            if run.warmup:
                continue
            self.calls.extend(call_records(run, report, bound, self.identity))
            self.journeys.append(journey_record(run, self.identity))
            self.rows.append({'lesson': run.plan.name, 'case_id': run.plan.question.id,
                              'budget': run.plan.max_bytes, 'exit_code': report.exit_code,
                              'driver_status': run.outcome.status})
        for ack in report.store_files:
            if not ack.applied:
                self.failures.append({'question_id': None, 'code': 'VARIANT_NOT_APPLIED',
                                      'detail': f'process {report.index}: {ack.name}: {ack.reason}'})
        self.process, self.pending = None, []

    def run(self, plan, warmup=False, pinning=None):
        if self.process is not None and not self.process.usable:
            self.close()
        if self.process is None and not self.open(pinning, plan.sample):
            if not warmup:
                self.failures.append({'question_id': plan.question.id, 'code': 'PROCESS_UNAVAILABLE',
                                      'detail': f'{plan.name} not run: the arm has no process'})
            return
        self.pending.append(drive(self.process, plan, warmup))


def _journey_plan(plan, question, sample, repeat):
    return JourneyPlan(question, sample, repeat, plan.max_calls, plan.max_bytes, plan.default_budget)


def _manifest(state, plan, machine, order, started, ended):
    arm, variant = state.arm, state.arm.variant
    applied = not any(f['code'] == 'VARIANT_NOT_APPLIED' for f in state.failures)
    return {'schema': RUN_SCHEMA, 'run_id': state.run_id, 'key': state.key,
            'variant': {'name': variant.name, 'path': arm.variant_path,
                        'preregistration_digest': variant.preregistration_digest(),
                        'config_digest': variant.config_digest()},
            'binary': {'sha256': state.binary_sha, 'version': arm.binary_version,
                       'provenance': arm.provenance},
            'store': {'key': arm.store.key, 'content_digest': arm.store.content_digest,
                      'label': arm.store.label},
            'questions': {'digest': state.key['questions_digest'], 'count': len(plan.questions),
                          'by_corpus': dict(sorted(Counter(q.corpus for q in plan.questions).items()))},
            'driver_version': DRIVER_VERSION,
            'encoders': [{'encoding': name, 'asset_sha256': sha} for name, sha in PINNED_ASSETS.items()],
            'samples': plan.samples, 'repeats': plan.repeats, 'max_calls': plan.max_calls,
            'timeout_s': float(plan.timeout_s), 'private': any(q.private for q in plan.questions),
            'max_bytes': None if plan.max_bytes is None else [plan.max_bytes],
            'machine': machine, 'order': {**order, 'position': state.position},
            'isolation': {'env': state.isolation_env, 'variant_applied': applied,
                          'store_files': {f.name: f.sha256 for f in variant.store_files},
                          'processes': state.reports},
            'started_at': started, 'ended_at': ended, 'failures': state.failures}


def _seal_arm(state, plan, machine, order, started):
    manifest = _manifest(state, plan, machine, order, started, _now())
    RunManifest.from_dict(manifest)
    staging = state.staging
    (staging / 'run.json').write_text(cachekey.canonical_json(manifest) + '\n', encoding='utf-8')
    (staging / 'calls.jsonl').write_text(jsonl.dump_lines(r.as_dict() for r in state.calls), encoding='utf-8')
    (staging / 'journeys.jsonl').write_text(jsonl.dump_lines(r.as_dict() for r in state.journeys),
                                            encoding='utf-8')
    _seal(staging, {'kind': 'memory bench run: native stdio journeys of bench questions',
                    'evidence_scope': 'native_replay', 'variant': state.arm.variant.name,
                    'binary_sha256': state.binary_sha, 'run_id': state.run_id,
                    'journeys': state.rows, 'model_calls': 0})
    state.final.parent.mkdir(parents=True, exist_ok=True)
    staging.rename(state.final)
    return ArmResult(state.arm.variant.name, state.run_id, state.final, False, manifest)


def _cached(state):
    raw = (state.final / 'manifest.json')
    if not raw.is_file():
        raise RunRefused(f'{state.final} exists without a manifest; remove it or rerun elsewhere')
    return ArmResult(state.arm.variant.name, state.run_id, state.final, True, json.loads(raw.read_text()))


def _check_arms(arms, plan, layout):
    if not 1 <= len(arms) <= 2:
        raise RunRefused('a run interleaves one or two arms')
    if len({arm.variant.name for arm in arms}) != len(arms):
        raise RunRefused('arms need distinct variant names')
    if any(q.private for q in plan.questions) and not layout.private:
        raise RunRefused('private questions need the private cache layout (outside the repository)')


def run_arms(arms, plan, layout):
    """Run `plan` on every arm, interleaved ABAB; returns one ArmResult per arm, in order."""
    plan.check()
    _check_arms(arms, plan, layout)
    states = [_ArmRun(arm, 'AB'[i], plan, layout, sha256_file(arm.binary)) for i, arm in enumerate(arms)]
    if len({s.run_id for s in states}) != len(states):
        raise RunRefused('both arms have the same result key: nothing would tell them apart')
    live = [s for s in states if not s.final.exists()]
    pinning = probes.resolve_pinning(plan.cpus)
    machine = probes.machine(pinning)
    order = {'scheme': ORDER_SCHEME if len(arms) == 2 else 'single', 'block': plan.block,
             'arms': [arm.variant.name for arm in arms], 'warmup_journeys': plan.warmup,
             'interleaved': len(live) == 2}
    started = _now()
    for state in live:
        shutil.rmtree(state.staging, ignore_errors=True)
        state.staging.mkdir(parents=True)
    try:
        for sample in range(plan.samples):
            for state in live:
                for question in plan.questions[:plan.warmup]:
                    state.run(_journey_plan(plan, question, sample, 0), True, pinning)
            for block in plan.blocks():
                for state in live:
                    for question in block:
                        for repeat in range(plan.repeats):
                            state.run(_journey_plan(plan, question, sample, repeat), False, pinning)
            for state in live:
                state.close()
        results = {id(s): _seal_arm(s, plan, machine, order, started) for s in live}
    finally:
        for state in live:
            if state.process is not None:
                state.process.close()
            shutil.rmtree(state.staging, ignore_errors=True)
    return [results.get(id(s)) or _cached(s) for s in states]
