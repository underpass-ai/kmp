"""Statistics of the bench (BENCH_SPEC section 11), stdlib only and deterministic.

- `wilson`: 95 % Wilson score interval of a proportion.
- `mcnemar_exact`: exact two-sided McNemar test on discordant pairs.
- `clopper_pearson_lower` / `_upper`: one-sided exact binomial bounds, used to
  certify `high` confidence (lower bound >= 0.95 at delta = 0.1).
- `paired_bootstrap`: percentile CI of the mean paired difference, resampling
  questions with a counted SplitMix64 (`domain/prng.py`), B = 10000 by default.
- `bootstrap_ci`: the same for any statistic of one sample (p50/p95 latency).
- `loglog_exponent`: scaling exponent b of y = a * x^b by least squares on logs,
  with a Student t interval.

Exact binomial tails use `math.comb` with `fractions.Fraction`, so p-values are
exact rationals rounded once.
"""
from dataclasses import dataclass
from fractions import Fraction
import math
from statistics import NormalDist

from .prng import SplitMix64

Z95 = NormalDist().inv_cdf(0.975)  # 1.959963984540054
BOOTSTRAP_B = 10_000
BOOTSTRAP_SEED = 7


def _check_counts(k, n):
    if not (isinstance(k, int) and isinstance(n, int)) or n < 0 or not 0 <= k <= n:
        raise ValueError(f'invalid counts {k}/{n}')


def wilson(k, n, z=Z95):
    """Wilson score interval `(lo, hi)`; None when n == 0."""
    _check_counts(k, n)
    if n == 0:
        return None
    p = k / n
    denominator = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / denominator
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denominator
    lo = 0.0 if k == 0 else max(0.0, centre - half)
    hi = 1.0 if k == n else min(1.0, centre + half)
    return (lo, hi)


def _binomial_cdf_half(k, n):
    """P(X <= k) for X ~ Bin(n, 1/2), exact."""
    return Fraction(sum(math.comb(n, i) for i in range(k + 1)), 2 ** n)


@dataclass(frozen=True)
class McNemar:
    improved: int  # discordant pairs where the candidate is right and the baseline wrong
    worsened: int
    p_value: float

    @property
    def discordant(self):
        return self.improved + self.worsened


def mcnemar_exact(improved, worsened):
    """Exact two-sided McNemar: p = min(1, 2 * P(X <= min(b, c))), X ~ Bin(b + c, 1/2).

    5-0 gives 0.0625 and 6-0 gives 0.03125. No discordant pairs gives p = 1.
    """
    for value in (improved, worsened):
        if not isinstance(value, int) or value < 0:
            raise ValueError('discordant counts are non-negative ints')
    n = improved + worsened
    if n == 0:
        return McNemar(improved, worsened, 1.0)
    p = min(Fraction(1), 2 * _binomial_cdf_half(min(improved, worsened), n))
    return McNemar(improved, worsened, float(p))


def mcnemar_from_pairs(pairs):
    """`pairs`: `(baseline_correct, candidate_correct)` per question."""
    pairs = [(bool(a), bool(b)) for a, b in pairs]
    return mcnemar_exact(sum(1 for a, b in pairs if b and not a),
                         sum(1 for a, b in pairs if a and not b))


def _binomial_upper_tail(k, n, p):
    """P(X >= k) for X ~ Bin(n, p), in floats (complement taken on the short side)."""
    if k <= 0:
        return 1.0
    if k > n:
        return 0.0
    if p <= 0.0:
        return 0.0
    if p >= 1.0:
        return 1.0
    log_p, log_q = math.log(p), math.log1p(-p)

    def term(i):
        return math.exp(math.lgamma(n + 1) - math.lgamma(i + 1) - math.lgamma(n - i + 1)
                        + i * log_p + (n - i) * log_q)

    if k > n * p:
        return math.fsum(term(i) for i in range(k, n + 1))
    return max(0.0, 1.0 - math.fsum(term(i) for i in range(0, k)))


def _bisect(predicate, lo=0.0, hi=1.0, iterations=200):
    """Largest x in [lo, hi] with predicate(x) true, for a predicate true then false."""
    for _ in range(iterations):
        mid = (lo + hi) / 2
        if predicate(mid):
            lo = mid
        else:
            hi = mid
    return lo


def clopper_pearson_lower(k, n, delta=0.1):
    """One-sided exact lower bound at confidence 1 - delta: P(X >= k | p_L) = delta.

    With no errors (k == n) this is delta ** (1 / n): n = 45 gives 0.9501.
    """
    _check_counts(k, n)
    if not 0 < delta < 1:
        raise ValueError('delta in (0, 1)')
    if n == 0 or k == 0:
        return 0.0
    if k == n:
        return delta ** (1 / n)
    return _bisect(lambda p: _binomial_upper_tail(k, n, p) <= delta)


def clopper_pearson_upper(k, n, delta=0.1):
    """One-sided exact upper bound at confidence 1 - delta: P(X <= k | p_U) = delta."""
    _check_counts(k, n)
    if n == 0 or k == n:
        return 1.0
    return 1.0 - clopper_pearson_lower(n - k, n, delta)


def certifies(k, n, floor=0.95, delta=0.1):
    """Whether k correct out of n certifies precision >= floor (one-sided, 1 - delta)."""
    return n > 0 and clopper_pearson_lower(k, n, delta) >= floor


def samples_to_certify(errors, floor=0.95, delta=0.1, limit=100_000):
    """Smallest n with `errors` wrong that certifies `floor` (45 / 77 / 105 for 0 / 1 / 2)."""
    for n in range(errors + 1, limit + 1):
        if certifies(n - errors, n, floor, delta):
            return n
    return None


def quantile(values, q):
    """Linear-interpolation quantile (Hyndman-Fan type 7, numpy's default)."""
    ordered = sorted(values)
    if not ordered:
        raise ValueError('quantile of nothing')
    if not 0 <= q <= 1:
        raise ValueError('q in [0, 1]')
    position = (len(ordered) - 1) * q
    below = math.floor(position)
    above = min(below + 1, len(ordered) - 1)
    return ordered[below] + (ordered[above] - ordered[below]) * (position - below)


def _mean(values):
    return math.fsum(values) / len(values)


@dataclass(frozen=True)
class Interval:
    estimate: float
    lo: float
    hi: float
    b: int
    seed: int


def bootstrap_ci(values, statistic=_mean, b=BOOTSTRAP_B, seed=BOOTSTRAP_SEED, level=0.95):
    """Percentile bootstrap CI of `statistic` over one sample."""
    values = list(values)
    if not values:
        raise ValueError('bootstrap of nothing')
    stream = SplitMix64(seed)
    n = len(values)
    replicates = [statistic([values[stream.below(n)] for _ in range(n)]) for _ in range(b)]
    tail = (1 - level) / 2
    return Interval(statistic(values), quantile(replicates, tail), quantile(replicates, 1 - tail), b, seed)


def paired_bootstrap(baseline, candidate, b=BOOTSTRAP_B, seed=BOOTSTRAP_SEED, level=0.95):
    """Percentile CI of mean(candidate - baseline), resampling paired questions.

    Values may be booleans (per-question correctness) or numbers.
    """
    baseline, candidate = list(baseline), list(candidate)
    if len(baseline) != len(candidate):
        raise ValueError('paired samples differ in length')
    differences = [float(c) - float(a) for a, c in zip(baseline, candidate)]
    return bootstrap_ci(differences, _mean, b, seed, level)


# --- Student t (for the log-log exponent) ----------------------------------------------------

def _betacf(a, b, x):
    """Continued fraction of the incomplete beta function (modified Lentz)."""
    tiny = 1e-300
    c, d = 1.0, 1.0 - (a + b) * x / (a + 1)
    d = 1.0 / (d if abs(d) > tiny else tiny)
    h = d
    for m in range(1, 1000):
        m2 = 2 * m
        for numerator in (m * (b - m) * x / ((a + m2 - 1) * (a + m2)),
                          -(a + m) * (a + b + m) * x / ((a + m2) * (a + m2 + 1))):
            d = 1.0 + numerator * d
            d = 1.0 / (d if abs(d) > tiny else tiny)
            c = 1.0 + numerator / c
            c = c if abs(c) > tiny else tiny
            h *= d * c
        if abs(d * c - 1.0) < 1e-15:
            break
    return h


def regularized_beta(a, b, x):
    """I_x(a, b)."""
    if x <= 0.0:
        return 0.0
    if x >= 1.0:
        return 1.0
    front = math.exp(math.lgamma(a + b) - math.lgamma(a) - math.lgamma(b)
                     + a * math.log(x) + b * math.log1p(-x))
    if x < (a + 1) / (a + b + 2):
        return front * _betacf(a, b, x) / a
    return 1.0 - front * _betacf(b, a, 1 - x) / b


def student_t_cdf(t, df):
    x = df / (df + t * t)
    tail = 0.5 * regularized_beta(df / 2, 0.5, x)
    return 1 - tail if t >= 0 else tail


def student_t_ppf(p, df):
    if not 0 < p < 1 or df <= 0:
        raise ValueError('p in (0, 1), df > 0')
    if p < 0.5:
        return -student_t_ppf(1 - p, df)
    lo, hi = 0.0, 1.0
    while student_t_cdf(hi, df) < p:
        hi *= 2
    return _bisect(lambda t: student_t_cdf(t, df) < p, lo, hi)


@dataclass(frozen=True)
class Exponent:
    b: float
    lo: float | None
    hi: float | None
    points: int
    r2: float | None


def loglog_exponent(xs, ys, level=0.95):
    """Slope b of log(y) = log(a) + b log(x) by OLS, with a t interval on n - 2 df.

    Repeated measurements per level are separate points. With two points the slope is
    exact and the interval is None.
    """
    xs, ys = list(xs), list(ys)
    if len(xs) != len(ys) or len(xs) < 2:
        raise ValueError('need at least two (x, y) points')
    if min(xs) <= 0 or min(ys) <= 0:
        raise ValueError('log-log needs positive values')
    lx = [math.log(x) for x in xs]
    ly = [math.log(y) for y in ys]
    n = len(lx)
    mx, my = _mean(lx), _mean(ly)
    sxx = math.fsum((x - mx) ** 2 for x in lx)
    if sxx == 0:
        raise ValueError('x does not vary')
    sxy = math.fsum((x - mx) * (y - my) for x, y in zip(lx, ly))
    slope = sxy / sxx
    intercept = my - slope * mx
    sse = math.fsum((y - intercept - slope * x) ** 2 for x, y in zip(lx, ly))
    syy = math.fsum((y - my) ** 2 for y in ly)
    r2 = None if syy == 0 else 1 - sse / syy
    if n < 3:
        return Exponent(slope, None, None, n, r2)
    se = math.sqrt(sse / (n - 2) / sxx)
    t = student_t_ppf(1 - (1 - level) / 2, n - 2)
    return Exponent(slope, slope - t * se, slope + t * se, n, r2)
