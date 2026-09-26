"""What Jev cost a run, per site (BENCH_SPEC sections 7 and 9).

Read from the `jev` field of the call records (the `kmp_mcp::judgement` telemetry
of BT03, attributed to the call that logged it), repeat 0 only, so a repeated
question is not billed twice. Per site: logical evaluations, provider requests,
input tokens and dollars at the pinned price, plus where the answers came from
(remote, cassette hit or miss, book hit). Latency is kept only for remote
evaluations: a cassette or book answer's `us` is not what Jev would take.

A binary without that telemetry leaves `jev = null` on its calls: the whole
section is then null with the call records' own reason, never 0 (SCHEMAS.md
section 0). With several samples each figure also carries min / mean / max over
samples.
"""
from collections import Counter
from pathlib import Path
import tomllib

from ..domain.errors import BenchError
from ..domain.run_record import JEV_SOURCES

PRICES = Path(__file__).resolve().parents[1] / 'config' / 'prices.toml'


def pinned_price(path=PRICES):
    """$ per million input tokens of the pinned model, from config/prices.toml."""
    try:
        data = tomllib.loads(Path(path).read_text(encoding='utf-8'))
        price = data['jev'][data['model']]['input_usd_per_mtok']
    except (OSError, KeyError, TypeError, tomllib.TOMLDecodeError) as error:
        raise BenchError(f'{path}: no input_usd_per_mtok for the pinned model: {error}') from error
    if isinstance(price, bool) or not isinstance(price, (int, float)) or price < 0:
        raise BenchError(f'{path}: input_usd_per_mtok must be a non-negative number')
    return float(price)


PRICE_USD_PER_MTOK = pinned_price()
SITE_LETTERS = {'rerank': 'p', 'wake_focus': 'p', 'curate_review': 'r', 'curate_focus': 'r',
                'write_relations': 'r', 'precheck': 't', 'paths': 'w/n/c', 'labels': 't',
                'summaries': 's/b'}
NO_REMOTE = 'latency is measured only in real (remote) mode'
NOUL_ABSENT = 'kmp_judgement telemetry carries no per-answer probabilities'


def usd(input_tokens):
    return input_tokens * PRICE_USD_PER_MTOK / 1_000_000


def _measured_calls(run):
    return [call for call in run.calls if call.repeat == 0]


def missing_reason(run):
    """Why a run has no Jev figures, or None when every call carries telemetry."""
    for call in _measured_calls(run):
        if call.jev is None:
            return (call.absent or {}).get('jev', 'call record without jev telemetry')
    return None


def _site_row(evaluations):
    sources = Counter(e.source for e in evaluations)
    tokens = sum(e.input_tokens for e in evaluations)
    remote = [e.us for e in evaluations if e.source == 'remote']
    return {'evaluations': len(evaluations), 'questions': sum(e.questions for e in evaluations),
            'requests': sum(e.requests for e in evaluations), 'input_tokens': tokens,
            'usd': usd(tokens), 'sources': {name: sources.get(name, 0) for name in JEV_SOURCES},
            'book_hits': sources.get('book_hit', 0),
            'remote_us': sum(remote) if remote else None,
            'remote_us_absent_reason': None if remote else NO_REMOTE}


def _spread(values):
    return {'min': min(values), 'mean': sum(values) / len(values), 'max': max(values)}


def jev_by_site(run):
    """Per-site Jev cost of one run (see the module docstring)."""
    reason = missing_reason(run)
    if reason is not None:
        return {'sites': None, 'totals': None, 'absent_reason': reason}
    tagged = [(call.sample, e) for call in _measured_calls(run) for e in call.jev]
    samples = sorted({call.sample for call in _measured_calls(run)})
    sites = {}
    for site in sorted({e.site for _, e in tagged}):
        chosen = [e for _, e in tagged if e.site == site]
        row = {'letter': SITE_LETTERS.get(site), **_site_row(chosen)}
        if len(samples) > 1:
            per = [sum(e.input_tokens for s, e in tagged if e.site == site and s == sample)
                   for sample in samples]
            row['input_tokens_by_sample'] = _spread(per)
        sites[site] = row
    totals = _site_row([e for _, e in tagged])
    if len(samples) > 1:
        totals['input_tokens_by_sample'] = _spread(
            [sum(e.input_tokens for s, e in tagged if s == sample) for sample in samples])
    return {'sites': sites, 'totals': totals, 'samples': len(samples),
            'noul_fraction': None, 'noul_absent_reason': NOUL_ABSENT, 'absent_reason': None}


def per_question_usd(run):
    """{(question_id, sample): dollars} over repeat 0, or None without telemetry."""
    if missing_reason(run) is not None:
        return None
    cost = {}
    for call in _measured_calls(run):
        key = (call.question_id, call.sample)
        cost[key] = cost.get(key, 0.0) + usd(sum(e.input_tokens for e in call.jev))
    return cost


def cassette_misses(run):
    """Replay evaluations the cassette/book could not answer: a replay run is then no_comparable."""
    return sum(1 for call in _measured_calls(run) for e in (call.jev or ()) if e.source == 'cassette_miss')
