"""Jev tail test: 429 with Retry-After, timeouts, 2-4 concurrent asks and deadlines (BENCH_SPEC 9, BT19).

  B='python3 -m scripts.performance.memory_bench'
  $B jev-tail                     real mode: needs TYPESAFE_API_KEY, else skipped with a reason
  $B jev-tail --self-check        the same measurement against a loopback stand-in (no network, no key)

For each concurrency level (2, 3, 4 by default) the test sends the same numbered
requests twice through `runtime/jev_fault_proxy.FaultProxy`: a clean pass (every
request forwarded) and a faulted pass (the `FaultPlan`, by default 429 with
Retry-After, a delay and a forced timeout among plain requests). Request `i`
goes to site `sites[i % len(sites)]` in both passes, so the added latency of a
request is its faulted wall time minus its clean one. Per level and per site
the report gives the maximum and the p99 of that added latency (p99 only with
n >= 100, BENCH_SPEC 11; otherwise null with the reason), the share of requests
that finished within the site's deadline, and the degraded requests with the
warning the binary would log for them.

What is measured, exactly. kmp-mcp pins its endpoint to https://api.typesafe.ai
and builds its HTTP client with `no_proxy()` (typesafe_config.rs,
typesafe_judgement.rs), so no proxy can sit between the binary and the
provider. The client here mirrors the binary's policy instead: up to
`MAX_RETRIES` retries of a 429, each after `Retry-After` seconds (1 when absent,
at most `MAX_RETRY_WAIT_SECS`), one per-request timeout, no retry of a timeout,
and the binary's error message for each way of giving up. A test reads those
constants from the Rust source, so the mirror cannot drift silently.

The key. Real mode reads `TYPESAFE_API_KEY` from the environment of this
process only; it travels as the `Authorization` header through the loopback
proxy and nowhere else. It is never written: the report records only that a key
was present. Without it the mode is skipped with its reason and exits 0; the
key file the binary could read is not consulted here.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import socket
import sys
import time
import urllib.error
import urllib.request

from .. import BENCH_VERSION
from ..domain.errors import BenchError
from ..runtime.jev_fault_proxy import (PROVIDER_HOST, SITE_HEADER, FaultPlan, FaultProxy, LocalUpstream,
                                       check_upstream, is_provider)
from ..runtime.layout import public_layout

SCHEMA = 'kmp.bench.jev_tail.v1'
API_KEY_ENV = 'TYPESAFE_API_KEY'
PROVIDER_URL = f'https://{PROVIDER_HOST}/v1/systemone'
MODEL = 'jev-1.13.0'
# Mirrors crates/kmp-mcp/src/serving/adapters/typesafe_{judgement,transport}.rs (checked by test_jev_tail).
MAX_RETRIES = 2
MAX_RETRY_WAIT_SECS = 5
DEFAULT_RETRY_WAIT_SECS = 1
TIMEOUT_MS = 20_000  # timeout_ms of the judged stores' typesafe.json
SITES = ('rerank', 'wake_focus', 'paths', 'labels')
DEFAULT_PLAN = 'pass,429:1,pass,delay:1500,pass,timeout:25,pass,429,pass,pass'
# --self-check: the same shapes, scaled to a 1 s timeout so it runs in seconds.
SELF_CHECK_PLAN = 'pass,429:1,pass,delay:300,pass,timeout:1.5,pass,429,pass,pass'
SELF_CHECK_TIMEOUT_MS = 1000
P99_MIN_N = 100
NO_KEY = (f'{API_KEY_ENV} is not set: the Jev tail test runs only in real mode (BENCH_SPEC 9); '
          'run it with the key exported, or `--self-check` against the loopback stand-in')
LIMITATION = ('kmp-mcp pins https://api.typesafe.ai and builds its client with no_proxy(), so the proxy '
              'cannot sit in front of the binary: the client mirrors its retry policy (2 retries of a 429, '
              'Retry-After capped at 5 s, 1 s when absent, one per-request timeout, no retry of a timeout)')
MESSAGES = {'timed_out': 'TypeSafe timed out', 'unavailable': 'TypeSafe unavailable',
            'rate_limited': 'TypeSafe rate limit persisted after retries',
            'rejected': 'TypeSafe rejected the API key', 'http': 'TypeSafe returned HTTP {code}'}


class TailRefused(BenchError):
    code = 'JEV_TAIL_REFUSED'


@dataclass(frozen=True)
class RetryPolicy:
    max_retries: int = MAX_RETRIES
    max_wait_s: int = MAX_RETRY_WAIT_SECS
    default_wait_s: int = DEFAULT_RETRY_WAIT_SECS
    timeout_s: float = TIMEOUT_MS / 1e3

    def wait_for(self, retry_after):
        try:
            wait = int(str(retry_after).strip()) if retry_after is not None else self.default_wait_s
        except ValueError:
            wait = self.default_wait_s
        return min(max(wait, 0), self.max_wait_s)


@dataclass(frozen=True)
class TailSettings:
    concurrency: tuple = (2, 3, 4)
    requests: int = 40  # per level and pass
    sites: tuple = SITES
    deadlines_ms: dict = field(default_factory=lambda: {site: TIMEOUT_MS for site in SITES})
    plan: FaultPlan = field(default_factory=lambda: FaultPlan.parse(DEFAULT_PLAN))
    policy: RetryPolicy = RetryPolicy()

    def check(self):
        if not self.concurrency or any(level < 2 or level > 4 for level in self.concurrency):
            raise TailRefused('concurrency levels are 2 to 4 (BENCH_SPEC 9)')
        if self.requests < 1 or not self.sites:
            raise TailRefused('at least one request and one site')
        missing = sorted(set(self.sites) - set(self.deadlines_ms))
        if missing:
            raise TailRefused(f'no deadline for {", ".join(missing)}')

    def as_dict(self):
        return {'concurrency': list(self.concurrency), 'requests': self.requests, 'sites': list(self.sites),
                'deadlines_ms': dict(sorted(self.deadlines_ms.items())), 'plan': self.plan.label(),
                'policy': {'max_retries': self.policy.max_retries, 'max_wait_s': self.policy.max_wait_s,
                           'default_wait_s': self.policy.default_wait_s, 'timeout_s': self.policy.timeout_s}}


@dataclass(frozen=True)
class Outcome:
    index: int
    site: str
    status: str  # ok | timed_out | unavailable | rate_limited | rejected | http
    code: int | None
    attempts: int
    waited_s: float
    elapsed_ms: float

    @property
    def degraded(self):
        return self.status != 'ok'

    def warning(self):
        if not self.degraded:
            return None
        return MESSAGES[self.status].format(code=self.code)


def body_for(site, index, model=MODEL):
    """A small, deterministic provider body per (site, request): one yes/no question."""
    return json.dumps({'state': f'memory bench tail probe: site {site}, request {index}', 'model': model,
                       'questions': {'c0': {'type': 'noul', 'instructions': 'Is this text a probe?'}}},
                      separators=(',', ':')).encode()


def _timed_out(error):
    reason = getattr(error, 'reason', error)
    return isinstance(reason, (socket.timeout, TimeoutError)) or 'timed out' in str(reason)


def send(url, body, key, site, policy, index=0, opener=None, sleep=time.sleep):
    """One evaluation as the binary sends it: retries on 429, gives up on anything else."""
    opener = opener or urllib.request.build_opener(urllib.request.ProxyHandler({}))
    headers = {'Content-Type': 'application/json', SITE_HEADER: site, 'Authorization': f'Bearer {key}'}
    started = time.perf_counter()
    attempts, waited = 0, 0.0

    def done(status, code=None):
        return Outcome(index, site, status, code, attempts, round(waited, 3), (time.perf_counter() - started) * 1e3)

    while True:
        attempts += 1
        request = urllib.request.Request(url, data=body, headers=headers, method='POST')
        try:
            with opener.open(request, timeout=policy.timeout_s) as response:
                response.read()
                return done('ok', response.status)
        except urllib.error.HTTPError as error:
            code = error.code
            if code == 429 and attempts <= policy.max_retries:
                wait = policy.wait_for(error.headers.get('Retry-After'))
                sleep(wait)
                waited += wait
                continue
            if code == 429:
                return done('rate_limited', code)
            return done('rejected' if code in (401, 403) else 'http', code)
        except (urllib.error.URLError, OSError) as error:
            return done('timed_out' if _timed_out(error) else 'unavailable')


def run_pass(url, key, settings, concurrency):
    """Outcomes of `settings.requests` requests sent by `concurrency` workers, by index."""
    jobs = [(index, settings.sites[index % len(settings.sites)]) for index in range(settings.requests)]
    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        futures = [pool.submit(send, url, body_for(site, index), key, site, settings.policy, index)
                   for index, site in jobs]
        return tuple(future.result() for future in futures)


def quantile(values, q):
    """Nearest-rank quantile (the bench reports p99 only with n >= 100)."""
    ordered = sorted(values)
    return ordered[max(0, math.ceil(q * len(ordered)) - 1)]


def tail_row(clean, faulted, deadline_ms):
    """Added latency (faulted - clean, paired by request), deadline and degradation of some requests."""
    by_index = {o.index: o for o in clean}
    added = [o.elapsed_ms - by_index[o.index].elapsed_ms for o in faulted if o.index in by_index]
    n = len(added)
    warnings = {}
    for outcome in faulted:
        if outcome.degraded:
            warnings[outcome.warning()] = warnings.get(outcome.warning(), 0) + 1
    on_time = sum(1 for o in faulted if o.elapsed_ms <= deadline_ms)
    return {'n': n, 'added_ms': {
        'max': round(max(added), 3) if added else None,
        'p99': round(quantile(added, 0.99), 3) if n >= P99_MIN_N else None,
        'p99_absent_reason': None if n >= P99_MIN_N else f'p99 needs n >= {P99_MIN_N} (BENCH_SPEC 11); n = {n}',
        'mean': round(sum(added) / n, 3) if added else None},
        'deadline_ms': deadline_ms, 'within_deadline': on_time, 'within_deadline_rate': on_time / n if n else None,
        'retries': sum(o.attempts - 1 for o in faulted), 'waited_s': round(sum(o.waited_s for o in faulted), 3),
        'degraded': sum(1 for o in faulted if o.degraded), 'warned': sum(warnings.values()),
        'warnings': dict(sorted(warnings.items())),
        'clean_errors': sum(1 for o in clean if o.degraded)}


def measure(upstream, key, settings, proxy_factory=FaultProxy):
    """Every concurrency level: clean pass, faulted pass, rows overall and per site."""
    settings.check()
    levels = []
    for concurrency in settings.concurrency:
        passes = {}
        for name, plan in (('clean', FaultPlan.clean()), ('faulted', settings.plan)):
            with proxy_factory(upstream, plan) as proxy:
                outcomes = run_pass(proxy.url, key, settings, concurrency)
            passes[name] = (outcomes, proxy.events())
        clean, faulted = passes['clean'][0], passes['faulted'][0]
        deadline = min(settings.deadlines_ms[s] for s in settings.sites)
        by_site = {site: tail_row([o for o in clean if o.site == site], [o for o in faulted if o.site == site],
                                  settings.deadlines_ms[site]) for site in settings.sites}
        faults = {}
        for event in passes['faulted'][1]:
            faults[event.fault] = faults.get(event.fault, 0) + 1
        levels.append({'concurrency': concurrency, 'all': tail_row(clean, faulted, deadline), 'by_site': by_site,
                       'proxy': {'faulted_requests': len(passes['faulted'][1]),
                                 'clean_requests': len(passes['clean'][1]), 'faults': dict(sorted(faults.items()))}})
    return levels


def skipped(reason, settings):
    return {'schema': SCHEMA, 'bench_version': BENCH_VERSION, 'status': 'skipped', 'reason': reason,
            'settings': settings.as_dict(), 'levels': [], 'limitations': [LIMITATION]}


def run_tail(settings, upstream=PROVIDER_URL, env=None, key=None, proxy_factory=FaultProxy):
    """The tail report. Real mode (the provider upstream) takes the key from `env` and is
    skipped without it; a loopback upstream takes `key` as given (a stand-in value)."""
    check_upstream(upstream)
    real = is_provider(upstream)
    if real:
        env = os.environ if env is None else env
        key = (env.get(API_KEY_ENV) or '').strip()
        if not key:
            return skipped(NO_KEY, settings)
    elif not key:
        raise TailRefused('a loopback upstream needs a stand-in key value')
    started = time.perf_counter()
    levels = measure(upstream, key, settings, proxy_factory)
    return {'schema': SCHEMA, 'bench_version': BENCH_VERSION, 'status': 'ran',
            'mode': 'real' if real else 'self_check', 'upstream': PROVIDER_URL if real else 'loopback stand-in',
            'key': {'source': f'env {API_KEY_ENV}' if real else 'stand-in', 'recorded': False},
            'settings': settings.as_dict(), 'levels': levels, 'elapsed_s': round(time.perf_counter() - started, 3),
            'limitations': [LIMITATION] + ([] if real else [
                'self-check: the upstream is a loopback stand-in, not Jev; its latency is not the provider\'s'])}


def _levels(text):
    try:
        levels = tuple(int(item) for item in text.split(','))
    except ValueError:
        raise argparse.ArgumentTypeError('levels are integers such as 2,3,4') from None
    return levels


def add_arguments(parser):
    parser.add_argument('--concurrency', type=_levels, default=(2, 3, 4), help='levels, 2 to 4 (default 2,3,4)')
    parser.add_argument('--requests', type=int, default=TailSettings.requests, help='per level and pass')
    parser.add_argument('--plan', help='faults cycled by arrival: pass, 429[:s], delay:<ms>, timeout:<s> '
                        f'(default {DEFAULT_PLAN}; with --self-check {SELF_CHECK_PLAN})')
    parser.add_argument('--timeout-ms', type=int, help=f'per-request timeout and deadline (default {TIMEOUT_MS}; '
                        f'with --self-check {SELF_CHECK_TIMEOUT_MS})')
    parser.add_argument('--self-check', action='store_true',
                        help='run against a loopback stand-in upstream (no network, no key)')
    parser.add_argument('--out', help='report directory (default tmp/memory-bench/jev-tail/<utc time>)')
    parser.set_defaults(action=run_command)
    return parser


def settings_from(args):
    timeout_ms = args.timeout_ms or (SELF_CHECK_TIMEOUT_MS if args.self_check else TIMEOUT_MS)
    plan = args.plan or (SELF_CHECK_PLAN if args.self_check else DEFAULT_PLAN)
    return TailSettings(concurrency=args.concurrency, requests=args.requests,
                        deadlines_ms={site: timeout_ms for site in SITES}, plan=FaultPlan.parse(plan),
                        policy=RetryPolicy(timeout_s=timeout_ms / 1e3))


def run_command(args):
    settings = settings_from(args)
    if args.self_check:
        with LocalUpstream(latency_ms=20) as upstream:
            report = run_tail(settings, upstream.url, key='stand-in')
    else:
        report = run_tail(settings)
    layout = public_layout()
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    out = layout.require_inside(Path(args.out) if args.out else layout.root / 'jev-tail' / stamp)
    out.mkdir(parents=True, exist_ok=True)
    (out / 'report.json').write_text(json.dumps(report, indent=1, sort_keys=True) + '\n', encoding='utf-8')
    print(json.dumps({'status': report['status'], 'reason': report.get('reason'),
                      'report': str(out / 'report.json')}, indent=1))
    return 0


def main(argv=None):
    parser = add_arguments(argparse.ArgumentParser(prog='memory_bench jev-tail', description=__doc__,
                                                   formatter_class=argparse.RawDescriptionHelpFormatter))
    args = parser.parse_args(argv)
    try:
        return args.action(args)
    except BenchError as failure:
        print(f'memory_bench jev-tail: {failure}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
