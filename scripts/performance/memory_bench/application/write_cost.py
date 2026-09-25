"""write_cost: milliseconds to write one entry, as p50/p95 over 20 single-entry writes.

BENCH_SPEC section 7 ("ms de escritura de 1 entrada"): on one pinned process
over a store (a copy of a template, or empty), `warmup` writes run first and
are not measured, then `writes` single-entry `kmp_ingest` calls follow, each
its own journey through token_harness' driver. Every write must be accepted
(receipt.is_accepted); a refused or timed-out write is reported, never
averaged in. Entries are deterministic, so two binaries write the same bytes.

Output: `kmp.bench.write_cost.v1` (a dict), and, when an output directory is
given, `write-cost.json` plus the process and per-write traces beside it.

  python3 -m scripts.performance.memory_bench.application.write_cost --binary target/release/kmp-mcp
"""
import argparse
from dataclasses import dataclass
import itertools
import json
from pathlib import Path
import shutil
import sys
import tempfile

from ...token_harness.domain.errors import HarnessError
from ...token_harness.native.capture import sha256_file
from ...token_harness.native.driver import run_journey
from ...token_harness.native.transport import TIMEOUT_SECONDS
from ..domain import cachekey
from ..domain.errors import BenchError
from ..domain.receipt import is_accepted
from ..domain.stats import quantile
from ..runtime import probes
from ..runtime.driver import CallSpec
from ..runtime.layout import public_layout
from ..runtime.session import BenchProcess, ProcessConfig

SCHEMA = 'kmp.bench.write_cost.v1'
ABOUT = 'bench:write-cost'
DIMENSION = {'id': 'work:main', 'kind': 'work'}
TIME_ORIGIN_HOUR = '2026-01-01T{:02d}:{:02d}:00Z'
WRITE_CALL_CAP = 4  # an ingest answers in one call; a review loop would show up as more


class WriteCostFailed(BenchError):
    code = 'WRITE_COST_FAILED'


@dataclass(frozen=True)
class WritePlan:
    writes: int = 20
    warmup: int = 1
    about: str = ABOUT
    timeout_s: float = float(TIMEOUT_SECONDS)
    cpus: str | None = probes.LATENCY_CPUS

    def check(self):
        if isinstance(self.writes, bool) or not isinstance(self.writes, int) or self.writes < 2:
            raise WriteCostFailed('writes must be an integer >= 2 (p95 needs a spread)')
        if isinstance(self.warmup, bool) or not isinstance(self.warmup, int) or self.warmup < 0:
            raise WriteCostFailed('warmup must be a non-negative integer')


def ingest_arguments(about, index):
    """One deterministic single-entry kmp_ingest; the first write declares the dimension."""
    minute = index % (24 * 60)
    when = TIME_ORIGIN_HOUR.format(minute // 60, minute % 60)
    entry = {'id': f'{about}:w{index:05d}', 'kind': 'observation',
             'text': f'Write-cost probe {index}: pressure of valve V-{index:05d} read at shift change.',
             'coordinates': [{'dimension': 'work', 'scope_id': 'work:main', 'occurred_at': when,
                              'valid_from': when, 'sequence': index + 1}]}
    return {'about': about, 'idempotency_key': f'write-cost:{about}:{index}',
            'memory': {'dimensions': [DIMENSION] if index == 0 else [], 'entries': [entry],
                       'relations': []}}


def _summary(values):
    if not values:
        return None
    return {'n': len(values), 'p50': quantile(values, 0.5), 'p95': quantile(values, 0.95),
            'min': min(values), 'max': max(values)}


def _sample(index, run, report):
    attempt = run['attempts'][-1] if run['attempts'] else None
    observation = attempt.observation if attempt else None
    found, reason = (report.telemetry.for_call(attempt.process_call_index, attempt.tool)
                     if attempt else (None, 'no call'))
    return {'write': index, 'status': run['outcome'].status, 'accepted': run['accepted'],
            'calls': len(run['attempts']),
            'wall_ns': observation.wall_ns if observation else None,
            'censored': bool(observation and observation.censored),
            'server_ms': found.tool.duration_ms if found else None,
            'server_us': found.tool.duration_us if found else None,
            'cpu_ns': observation.resources.get('cpu_ns') if observation else None,
            'rss_peak_kb': observation.resources.get('rss_peak_kb') if observation else None,
            'server_absent': reason}


def _write(process, about, index, name):
    with process.journey(name) as tap:
        outcome = run_journey(tap, CallSpec('kmp_ingest', ingest_arguments(about, index)), None,
                              WRITE_CALL_CAP)
    accepted = outcome.status == 'completed' and bool(outcome.results) and is_accepted(outcome.results[-1])
    return {'outcome': outcome, 'attempts': tap.attempts, 'accepted': accepted}


def measure_write_cost(binary, work_dir, plan=None, *, env=None, template=None, out_dir=None):
    """Run the plan on a fresh pinned process; returns the kmp.bench.write_cost.v1 dict."""
    plan = plan or WritePlan()
    plan.check()
    binary = Path(binary).resolve()
    pinning = probes.resolve_pinning(plan.cpus)
    machine = probes.machine(pinning)
    Path(work_dir).mkdir(parents=True, exist_ok=True)
    traces = Path(out_dir) if out_dir else Path(tempfile.mkdtemp(prefix='write-cost-', dir=work_dir))
    traces.mkdir(parents=True, exist_ok=True)
    config = ProcessConfig(binary, Path(work_dir), 'write-cost', env=dict(env or {}), template=template,
                           timeout_seconds=plan.timeout_s, pinning=pinning)
    process = BenchProcess(config, traces, 0, itertools.count(1))
    runs = []
    try:
        for index in range(plan.warmup + plan.writes):
            if not process.usable:
                break
            warm = index < plan.warmup
            runs.append(_write(process, plan.about, index,
                               None if warm else f'write-cost~w{index - plan.warmup:03d}'))
    finally:
        report = process.close()
    measured = [_sample(i, run, report) for i, run in enumerate(runs[plan.warmup:])]
    good = [s for s in measured if s['accepted'] and not s['censored']]
    result = {'schema': SCHEMA, 'binary_sha256': sha256_file(binary), 'tool': 'kmp_ingest',
              'about': plan.about, 'entries_per_write': 1, 'writes': plan.writes, 'warmup': plan.warmup,
              'accepted': sum(1 for s in measured if s['accepted']),
              'censored': sum(1 for s in measured if s['censored']),
              'wall_ns': _summary([s['wall_ns'] for s in good]),
              'server_ms': _summary([s['server_ms'] for s in good if s['server_ms'] is not None]),
              'server_us': _summary([s['server_us'] for s in good if s['server_us'] is not None]),
              'cpu_ns': _summary([s['cpu_ns'] for s in good if s['cpu_ns'] is not None]),
              'rss_peak_kb': _summary([s['rss_peak_kb'] for s in good if s['rss_peak_kb'] is not None]),
              'startup_ns': report.startup_ns, 'cpus_allowed': report.cpus_allowed,
              'machine': machine, 'samples': measured}
    if out_dir:
        (traces / 'write-cost.json').write_text(cachekey.canonical_json(result) + '\n', encoding='utf-8')
    else:
        shutil.rmtree(traces, ignore_errors=True)
    if result['accepted'] != plan.writes:
        raise WriteCostFailed(f'{plan.writes - result["accepted"]} of {plan.writes} writes not accepted')
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--writes', type=int, default=20)
    parser.add_argument('--warmup', type=int, default=1)
    parser.add_argument('--cpus', default=probes.LATENCY_CPUS, help="taskset-style list; 'none' to not pin")
    parser.add_argument('--out', type=Path, help='keep write-cost.json and the traces here')
    args = parser.parse_args(argv)
    layout = public_layout()
    plan = WritePlan(args.writes, args.warmup, cpus=None if args.cpus == 'none' else args.cpus)
    try:
        result = measure_write_cost(args.binary, layout.scratch(), plan, out_dir=args.out)
    except HarnessError as error:
        print(str(error), file=sys.stderr)
        return 2
    print(json.dumps({key: result[key] for key in ('binary_sha256', 'accepted', 'wall_ns', 'server_us',
                                                   'cpu_ns', 'rss_peak_kb', 'cpus_allowed')}, indent=2))
    return 0


if __name__ == '__main__':
    sys.exit(main())
