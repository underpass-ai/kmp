"""Paired comparison of two arms (BENCH_SPEC sections 8, 10 and 11).

Two runs are compared only when their anti-drift fields agree (questions digest,
driver version, encoders with their asset SHA-256, bench version:
`RunManifest.comparability`, `cachekey.drift`); otherwise the comparison is
refused and the verdict is `no_comparable`, never a number.

Questions are paired by (question id, sample), repeat 0. A rate is compared with
exact McNemar on the discordant pairs and a paired bootstrap CI of the difference;
a mean (tokens, pages, coverage) with the paired bootstrap alone. Every Delta
carries the minimum detectable effect at alpha 0.05 and 80 % power
(`domain/power.py`): for a rate MDE = 2.8 * sqrt(d / n) with d the discordant
rate, floored at one discordant pair (d >= 1/n) so that a run without
disagreements does not claim unlimited power; for a mean MDE = 2.8 * sd(diff) /
sqrt(n). A Delta is `decidable` only when a pre-registered effect exists and
MDE <= effect; a rate Delta also needs at least `stats.mcnemar_min_discordant()`
discordant pairs (6 at alpha 0.05), because with fewer no split can reach
McNemar p < alpha and the comparison has no power whatever its MDE says.

A rate moves (`improvement` +1 or -1) only when its bootstrap CI95 lies wholly on
one side of 0 *and* exact McNemar gives p < alpha: the bootstrap alone calls 4-0
out of 100 an improvement although McNemar gives p = 0.125.
"""
from dataclasses import dataclass
import math

from ..domain import cachekey, power, stats
from ..domain.errors import BenchError
from ..domain.metric_catalog import POOLED, RATE, higher_is_better, spec
from . import aggregate
from .tokens import PRIMARY_ENCODING

NO_EFFECT = 'no pre-registered effect for this metric'
ALPHA = power.ALPHA
MIN_DISCORDANT = stats.mcnemar_min_discordant(ALPHA)


class ComparisonRefused(BenchError):
    code = 'NO_COMPARABLE'


def drift(baseline_run, candidate_run):
    """Anti-drift fields on which the two runs disagree; () means comparable."""
    return tuple(cachekey.drift(baseline_run.comparability(), candidate_run.comparability()))


def require_comparable(baseline_run, candidate_run):
    differing = drift(baseline_run, candidate_run)
    if differing:
        raise ComparisonRefused('runs differ in ' + ', '.join(differing))


@dataclass(frozen=True)
class Pairing:
    keys: tuple  # (question_id, sample), sorted
    baseline: dict  # key -> QuestionScore
    candidate: dict
    unpaired: int  # scores present on one side only

    @classmethod
    def of(cls, baseline_scores, candidate_scores):
        base = {s.key: s for s in baseline_scores}
        cand = {s.key: s for s in candidate_scores}
        keys = tuple(sorted(set(base) & set(cand)))
        return cls(keys, base, cand, len(set(base) ^ set(cand)))

    def restricted(self, predicate):
        keys = tuple(k for k in self.keys if predicate(self.baseline[k]))
        return Pairing(keys, self.baseline, self.candidate, self.unpaired)


def _values(pairing, name, extra, encoding):
    """Paired observations (baseline, candidate) of a metric where both sides have one."""
    pairs = []
    for key in pairing.keys:
        if extra is not None:
            a, b = extra[0].get(key), extra[1].get(key)
        else:
            a = aggregate.per_question(pairing.baseline[key], name, encoding)
            b = aggregate.per_question(pairing.candidate[key], name, encoding)
        if a is not None and b is not None:
            pairs.append((a, b))
    return pairs


def _kind(name, extra):
    return 'mean' if extra is not None else spec(name).kind


def _numeric_mde(differences):
    if len(differences) < 2:
        return None
    return power.coefficient() * statistics_sd(differences) / math.sqrt(len(differences))


def statistics_sd(values):
    mean = math.fsum(values) / len(values)
    return math.sqrt(math.fsum((v - mean) ** 2 for v in values) / (len(values) - 1))


def delta(pairing, name, effect=None, b=stats.BOOTSTRAP_B, encoding=PRIMARY_ENCODING, extra=None):
    """One Delta object (SCHEMAS.md section 5). `extra`: ({key: v}, {key: v}) for metrics
    outside the per-question scores (Jev dollars)."""
    kind = _kind(name, extra)
    pairs = _values(pairing, name, extra, encoding)
    better_up = higher_is_better(name)
    row = {'metric': name, 'delta': None, 'ci95': None, 'method': None, 'p_value': None,
           'discordant': None, 'mde': None, 'effect': effect, 'decidable': False}
    if kind == POOLED:
        hits = [sum(h for h, _ in side) for side in zip(*pairs)] if pairs else [0, 0]
        totals = [sum(t for _, t in side) for side in zip(*pairs)] if pairs else [0, 0]
        row['baseline'] = aggregate.rate_metric(hits[0], totals[0])
        row['candidate'] = aggregate.rate_metric(hits[1], totals[1])
        if totals[0] and totals[1]:
            row['delta'] = hits[1] / totals[1] - hits[0] / totals[0]
        return row
    if not pairs:
        row['baseline'] = row['candidate'] = aggregate.absent('no paired question has this metric')
        return row
    n = len(pairs)
    if kind == RATE:
        base, cand = [bool(a) for a, _ in pairs], [bool(c) for _, c in pairs]
        row['baseline'] = aggregate.rate_metric(sum(base), n)
        row['candidate'] = aggregate.rate_metric(sum(cand), n)
        up = sum(1 for a, c in zip(base, cand) if c and not a)
        down = sum(1 for a, c in zip(base, cand) if a and not c)
        improved, worsened = (up, down) if better_up else (down, up)
        test = stats.mcnemar_exact(improved, worsened)
        rate = (improved + worsened) / n
        row.update(delta=(sum(cand) - sum(base)) / n, method='mcnemar_exact', p_value=test.p_value,
                   discordant={'improved': improved, 'worsened': worsened, 'rate': rate})
        row['mde'] = power.mde(n, max(rate, 1 / n))
    else:
        base, cand = [float(a) for a, _ in pairs], [float(c) for _, c in pairs]
        row['baseline'] = aggregate.mean_metric(base)
        row['candidate'] = aggregate.mean_metric(cand)
        differences = [c - a for a, c in zip(base, cand)]
        row.update(delta=math.fsum(differences) / n, method='paired_bootstrap',
                   mde=_numeric_mde(differences))
        worse = sum(1 for d in differences if (d < 0 if better_up else d > 0))
        better = sum(1 for d in differences if (d > 0 if better_up else d < 0))
        row['discordant'] = {'improved': better, 'worsened': worse, 'rate': (better + worse) / n}
    if b is not None:
        interval = stats.paired_bootstrap(base, cand, b=b)
        row['ci95'] = [interval.lo, interval.hi]
    row['decidable'] = effect is not None and row['mde'] is not None and row['mde'] <= effect
    if kind == RATE and improved + worsened < MIN_DISCORDANT:
        row['decidable'] = False
    return row


def improvement(row):
    """+1 when the CI95 of Δ lies wholly on the better side, -1 on the worse side, else 0.

    A rate compared with McNemar also needs p < alpha to move either way. Without a CI
    (no bootstrap) the point Δ decides only when it is exactly 0 (A/A)."""
    if row['delta'] is None:
        return None
    sign = 1 if higher_is_better(row['metric']) else -1
    if row['ci95'] is None:
        return 0 if row['delta'] == 0 else None
    lo, hi = (value * sign for value in row['ci95'])
    lo, hi = min(lo, hi), max(lo, hi)
    if row.get('method') == 'mcnemar_exact' and not (row.get('p_value') is not None
                                                     and row['p_value'] < ALPHA):
        return 0
    if lo > 0:
        return 1
    if hi < 0:
        return -1
    return 0


def worsening(row):
    """How much the candidate is worse than the baseline on this metric (<= 0 is not worse)."""
    if row['delta'] is None:
        return None
    return -row['delta'] if higher_is_better(row['metric']) else row['delta']
