"""Latency, resources and scale of a run (BENCH_SPEC sections 7 and 11).

Per (tool, phase): harness wall time, server time, CPU, peak RSS and read I/O of the
ok calls, with bootstrap CIs of p50/p95 when `b` is given and p99 only from 100
calls up. A timed-out call is censored: its wall time is a lower bound, so it is
counted apart and never averaged in. Startup (process start to `initialize`) comes
from the run's process table. The scale exponent b of `wall = a * N^b` is fitted on
log-log over every warm `kmp_ask` call of runs at different ladder levels.
"""
from ..domain import stats

P99_MIN_N = 100


def _quantile_metric(values, q, b):
    if not values:
        return None
    value = stats.quantile(values, q)
    if b is None:
        return {'value': value, 'n': len(values), 'ci95': None, 'method': None}
    interval = stats.bootstrap_ci(values, lambda xs: stats.quantile(xs, q), b=b)
    return {'value': value, 'n': len(values), 'ci95': [interval.lo, interval.hi], 'method': 'bootstrap'}


def _spread(values):
    if not values:
        return None
    return {'p50': stats.quantile(values, 0.5), 'max': max(values), 'n': len(values)}


def by_tool_phase(run, b=None):
    rows = []
    groups = {}
    for call in run.calls:
        groups.setdefault((call.tool, call.phase), []).append(call)
    for (tool, phase), calls in sorted(groups.items()):
        ok = [c for c in calls if c.status == 'ok' and c.wall_ns is not None]
        wall = [c.wall_ns / 1e6 for c in ok]
        server = [c.server_ms for c in ok if c.server_ms is not None]
        cpu = [c.cpu_ns / 1e6 for c in ok if c.cpu_ns is not None]
        rss = [c.rss_peak_kb for c in ok if c.rss_peak_kb is not None]
        rchar = [c.io['rchar'] for c in ok if c.io is not None]
        rows.append({
            'tool': tool, 'phase': phase, 'calls': len(calls), 'ok': len(ok),
            'censored': sum(1 for c in calls if c.censored),
            'wall_ms_p50': _quantile_metric(wall, 0.5, b), 'wall_ms_p95': _quantile_metric(wall, 0.95, b),
            'wall_ms_p99': _quantile_metric(wall, 0.99, b) if len(wall) >= P99_MIN_N else None,
            'server_ms': _spread(server), 'cpu_ms': _spread(cpu), 'rss_peak_kb': _spread(rss),
            'io_rchar': _spread(rchar),
            'absent': sorted({name for c in ok for name in (c.absent or {})})})
    return rows


def startup(run):
    processes = ((run.manifest.get('isolation') or {}).get('processes')) or []
    values = [p['startup_ns'] / 1e6 for p in processes if isinstance(p.get('startup_ns'), int)]
    return _spread(values)


def section(run, b=None):
    return {'by_tool_phase': by_tool_phase(run, b), 'startup_ms': startup(run)}


def scale_exponent(points, metric='wall_ms'):
    """`points`: (level N, value) pairs; b of value = a * N^b with its t interval."""
    points = [(n, v) for n, v in points if n and v and v > 0]
    if len({n for n, _ in points}) < 2:
        return None
    fit = stats.loglog_exponent([n for n, _ in points], [v for _, v in points])
    return {'metric': metric, 'b': fit.b, 'ci95': None if fit.lo is None else [fit.lo, fit.hi],
            'points': fit.points, 'r2': fit.r2, 'levels': sorted({n for n, _ in points})}


def warm_ask_points(run, level):
    return [(level, c.wall_ns / 1e6) for c in run.calls
            if c.tool == 'kmp_ask' and c.phase == 'warm' and c.status == 'ok' and c.wall_ns]
