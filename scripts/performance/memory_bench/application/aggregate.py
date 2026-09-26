"""From per-question scores to the report's Metric objects (SCHEMAS.md section 5).

A Metric is `{value, n, ci95, method, absent_reason}`: a rate carries its Wilson
interval, a mean its bootstrap interval when one is asked for (bootstrap is the
expensive part, so descriptive tables pass `b=None` and publish the point value
with `method: null`), and anything that could not be computed is `value: null`
with the reason. Nothing here compares arms; `compare.py` does.
"""
from collections import Counter
import math

from ..domain import metrics, stats
from ..domain.metric_catalog import CATALOG, POOLED, RATE, TOKEN_METRICS, spec
from .tokens import PRIMARY_ENCODING

HIGH_FLOOR = 0.95
HIGH_DELTA = 0.1
NO_QUESTION = 'no question of this kind in the run'


def metric(value, n, ci95=None, method=None, absent_reason=None):
    if value is None and absent_reason is None:
        absent_reason = NO_QUESTION
    if isinstance(value, float) and not math.isfinite(value):
        raise ValueError('non-finite metric value')
    return {'value': value, 'n': n, 'ci95': None if ci95 is None else [ci95[0], ci95[1]],
            'method': method, 'absent_reason': None if value is not None else absent_reason}


def absent(reason, n=0):
    return metric(None, n, absent_reason=reason)


def rate_metric(hits, n):
    if n == 0:
        return absent(NO_QUESTION)
    return metric(hits / n, n, stats.wilson(hits, n), 'wilson')


def mean_metric(values, b=None, seed=stats.BOOTSTRAP_SEED):
    values = [float(v) for v in values]
    if not values:
        return absent(NO_QUESTION)
    value = math.fsum(values) / len(values)
    if b is None:
        return metric(value, len(values))
    interval = stats.bootstrap_ci(values, b=b, seed=seed)
    return metric(value, len(values), (interval.lo, interval.hi), 'bootstrap')


def per_question(score, name, encoding=PRIMARY_ENCODING):
    """The per-question observation of a metric, or None when it does not apply."""
    if name in TOKEN_METRICS:
        row = score.tokens.get(encoding)
        return None if row is None else row.get(TOKEN_METRICS[name])
    if name == 'pages':
        return None if score.error else score.pages
    if name == 'response_bytes':
        return None if score.error else score.response_bytes
    source = spec(name).source
    if spec(name).kind == POOLED:
        return score.counts.get(source)
    return score.values.get(source)


def compute(scores, name, b=None, encoding=PRIMARY_ENCODING, reason=None):
    """One Metric over `scores`; `reason` overrides the absent reason of an empty metric."""
    kind = spec(name).kind
    observed = [per_question(s, name, encoding) for s in scores]
    observed = [value for value in observed if value is not None]
    if kind == POOLED:
        hits, n = sum(h for h, _ in observed), sum(t for _, t in observed)
        result = rate_metric(hits, n)
    elif kind == RATE:
        result = rate_metric(sum(1 for v in observed if v), len(observed))
    else:
        result = mean_metric(observed, b)
    if result['value'] is None and reason is not None:
        result['absent_reason'] = reason
    return result


def summarize(scores, b=None, only_present=True, names=None):
    """Every catalog metric (or `names`) over the scores; empty metrics dropped unless asked."""
    table = {}
    for name in names or [s.name for s in CATALOG]:
        value = compute(scores, name, b)
        if value['n'] or not only_present:
            table[name] = value
    return table


def scorecard_port(scores):
    """`RetrievalScorecard::score` over every kmp_ask question, as the Rust bins print it."""
    outcomes = [s.retrieval for s in scores if s.retrieval is not None]
    if not outcomes:
        return None
    card = metrics.RetrievalScorecard.score(outcomes)
    return {'cases': card.cases, **dict(card.quality_columns()),
            'false_unknown_rate': card.false_unknown_rate}


def false_unknown_both(scores):
    """BENCH_SPEC section 7: the scorecard definition and P(UNKNOWN | KNOWN), side by side."""
    ask = [s for s in scores if 'unknown' in s.values]
    pairs = [('useful' in s.values, s.values['unknown']) for s in ask]
    both = metrics.false_unknown_rate(pairs)
    return {'scorecard': rate_metric(both.scorecard.hits, both.scorecard.n),
            'given_known': rate_metric(both.given_known.hits, both.given_known.n)}


def confidence_table(scores):
    """Precision per `proof.confidence` level; `high` certified by one-sided Clopper-Pearson."""
    cases = [(s.confidence, s.values['confidence_correct']) for s in scores
             if 'confidence_correct' in s.values]
    rows = {}
    for level, rate in sorted(metrics.precision_by_confidence(cases).items()):
        row = rate_metric(rate.hits, rate.n)
        lower = stats.clopper_pearson_lower(rate.hits, rate.n, HIGH_DELTA)
        rows[level] = {**row, 'clopper_pearson_lower': lower,
                       'certifies_high': level == 'high' and stats.certifies(rate.hits, rate.n, HIGH_FLOOR, HIGH_DELTA)}
    curve = metrics.aurc([case for case in cases if case[0] in metrics.CONFIDENCE_ORDER])
    high = rows.get('high')
    return {'by_level': rows,
            'high_precision': ({k: high[k] for k in ('value', 'n', 'ci95', 'method', 'absent_reason')}
                               if high else absent('no answer at confidence high')),
            'high_clopper_pearson_lower': high['clopper_pearson_lower'] if high else None,
            'high_certified': bool(high and high['certifies_high']),
            'aurc': curve.aurc, 'risk_coverage': [list(p) for p in curve.points]}


def reasons_table(scores):
    """UNKNOWN answers by native reason and by the bench reason it maps to."""
    unknown = [s for s in scores if s.decision == 'UNKNOWN']
    native = Counter(s.native_reason or 'none' for s in unknown)
    mapped = Counter(s.reason or 'unmapped' for s in unknown)
    return {'unknown_answers': len(unknown), 'native': dict(sorted(native.items())),
            'bench': dict(sorted(mapped.items()))}


def outcomes_table(scores):
    return dict(sorted(Counter(s.outcome or s.status for s in scores).items()))


def agent_load(scores, encoding=PRIMARY_ENCODING):
    """Pages, response bytes and tokens a journey costs the agent until it completes."""
    done = [s for s in scores if not s.error]
    if not done:
        return None
    pages = [s.pages for s in done]
    sizes = [s.response_bytes for s in done if s.response_bytes is not None]
    tokens = [v for v in (per_question(s, 'tokens_journey', encoding) for s in done) if v is not None]

    def spread(values):
        if not values:
            return None
        return {'mean': math.fsum(values) / len(values), 'p50': stats.quantile(values, 0.5),
                'max': max(values), 'n': len(values)}

    return {'journeys': len(done), 'pages': spread(pages), 'response_bytes': spread(sizes),
            'tokens': spread(tokens),
            'completed_rate': rate_metric(sum(1 for s in done if s.status == 'completed'), len(done))}


def tokens_per_useful(scores, encoding=PRIMARY_ENCODING):
    """Journey tokens of the positive kmp_ask questions over the useful answers among them."""
    positives = [s for s in scores if 'useful' in s.values]
    counted = [per_question(s, 'tokens_journey', encoding) for s in positives]
    if not positives:
        return None, NO_QUESTION
    if any(value is None for value in counted):
        return None, 'tokens not counted for every positive question'
    useful = sum(1 for s in positives if s.values['useful'])
    if useful == 0:
        return None, 'no useful answer'
    return sum(counted) / useful, None

