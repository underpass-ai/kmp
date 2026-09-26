"""Controls of a report (BENCH_SPEC section 10), wired to the parity and determinism oracles.

- `aa`: with one variant on both arms, the quality and token deltas must be exactly 0.
- `parity`: `parity.compare_captures` over the two runs' traces, call by call, after
  the documented volatile fields; this is what a `claim = 'parity'` variant is judged by.
- `repeat`: the same oracle between repeat 0 and every later repeat of one run
  (same process, same arguments), per arm; the JSON-RPC id, which a repeat on the
  same process cannot share, is set aside before comparing.
- `determinism`: the report `determinism.py` wrote (fresh processes on identical
  stores), read back and summarized; the bench does not rerun it.

Every control keeps counts, rates and generalized JSON paths only. Trace names are
question ids, so for a private run the per-call `differences` list is dropped and
only `path_counts` remain.
"""
import dataclasses
import json
from pathlib import Path

from ..domain.errors import BenchError
from . import parity
from .compare import delta

AA_QUALITY = ('useful_rate', 'answer_correct_rate', 'partial_useful_rate', 'false_unknown_rate',
              'false_answer_rate', 'facet_coverage', 'recall_at_5', 'core_precision', 'path_found',
              'wake_obligation_coverage')
AA_TOKENS = ('tokens_journey', 'tokens_first_page', 'pages')
PARITY_KEYS = ('calls', 'identical', 'normalized', 'different', 'parity_rate', 'byte_identical_rate',
               'by_kind', 'volatile_reconciled', 'path_counts')


class ControlUnreadable(BenchError):
    code = 'CONTROL_UNREADABLE'


def aa(pairing):
    """Δ of every A/A metric that both arms measured; `holds` when all are exactly 0."""
    rows = {}
    for name in AA_QUALITY + AA_TOKENS:
        row = delta(pairing, name, b=None)
        if row['delta'] is not None:
            rows[name] = row['delta']
    quality = {k: v for k, v in rows.items() if k in AA_QUALITY}
    tokens = {k: v for k, v in rows.items() if k in AA_TOKENS}
    moved = sorted(k for k, v in rows.items() if v != 0)
    return {'quality_delta_zero': all(v == 0 for v in quality.values()),
            'token_delta_zero': all(v == 0 for v in tokens.values()) if tokens else None,
            'token_absent_reason': None if tokens else 'tokens not counted',
            'deltas': dict(sorted(rows.items())), 'moved': moved, 'holds': not moved}


def _public(summary, private):
    kept = {k: summary[k] for k in PARITY_KEYS if k in summary}
    if not private:
        kept['differences'] = summary.get('differences', [])
    return kept


def parity_between(baseline_run, candidate_run):
    """The parity oracle between the two runs' journeys (trace by trace, call by call)."""
    left, right = parity.load_capture(baseline_run.root), parity.load_capture(candidate_run.root)
    summary = parity.compare_captures(left, right)
    return _public(summary, baseline_run.private or candidate_run.private)


def _without_rpc_id(exchange):
    """The exchange with its JSON-RPC ids set to its position in the trace.

    Repeats run on one process, so their request ids differ by construction; that is
    transport bookkeeping, not an answer. Both sides are re-serialized the same way, so
    byte identity of everything else is preserved."""
    def rewrite(raw):
        try:
            value = json.loads(raw)
        except ValueError:
            return raw
        if isinstance(value, dict) and 'id' in value:
            value['id'] = exchange.index
        return json.dumps(value, ensure_ascii=False, separators=(',', ':'))
    return dataclasses.replace(exchange, request=rewrite(exchange.request),
                               response=rewrite(exchange.response))


def _split_repeats(capture):
    """{question~sample: exchanges} per repeat index, from journey names `<q>~s<k>~r<n>`."""
    repeats = {}
    for trace, exchanges in capture.items():
        stem, sep, repeat = trace.rpartition('~r')
        if not sep or not repeat.isdigit():
            continue
        repeats.setdefault(int(repeat), {})[stem] = tuple(_without_rpc_id(e) for e in exchanges)
    return repeats


def repeat_consistency(run):
    """Repeat 0 against each later repeat of the same run; None when the run has one repeat."""
    repeats = _split_repeats(parity.load_capture(run.root))
    if len(repeats) < 2 or 0 not in repeats:
        return None
    rows = []
    for index in sorted(k for k in repeats if k):
        summary = parity.compare_captures(repeats[0], repeats[index])
        rows.append({'repeat': index, **_public(summary, run.private)})
    calls = sum(row['calls'] for row in rows)
    equal = sum(row['identical'] + row['normalized'] for row in rows)
    return {'pairs': rows, 'calls': calls, 'parity_rate': equal / calls if calls else None}


def determinism_summary(path, binaries=()):
    """Summary of a `determinism.py` report (`determinism.json`, or the directory holding it)."""
    path = Path(path)
    if path.is_dir():
        path = path / 'determinism.json'
    try:
        report = json.loads(path.read_text(encoding='utf-8'))
    except (OSError, ValueError) as error:
        raise ControlUnreadable(f'{path}: {error}') from error
    if report.get('schema') != 'kmp.bench.determinism.v1':
        raise ControlUnreadable(f'{path}: not a kmp.bench.determinism.v1 report')
    return {'determinism_rate': report.get('determinism_rate'), 'calls': report.get('calls'),
            'runs': report.get('runs'), 'by_kind': report.get('by_kind'), 'label': report.get('label'),
            'binary_sha256': report.get('binary_sha256'),
            'arch': (report.get('machine') or {}).get('arch'),
            'binary_matches_arm': (report.get('binary_sha256') in binaries) if binaries else None}
