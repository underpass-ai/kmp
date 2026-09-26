"""Statistical power (BENCH_SPEC section 11): minimum detectable effect and `indecidible`.

For a paired comparison of two proportions with discordant-pair rate d over n
questions, the effect detectable with power 1 - beta at two-sided alpha is

    MDE = (z_{1-alpha/2} + z_{1-beta}) * sqrt(d / n)

which at alpha = 0.05 and 80 % power is 2.8016 * sqrt(d / n) (the spec's
"2,8"). A comparison whose MDE exceeds the effect it looks for cannot decide:
its verdict is `indecidible`, never `neutral` (spec principle 3).
"""
from dataclasses import dataclass
import math
from statistics import NormalDist

ALPHA = 0.05
POWER = 0.80
INDECIDIBLE = 'indecidible'


def coefficient(alpha=ALPHA, power=POWER):
    normal = NormalDist()
    return normal.inv_cdf(1 - alpha / 2) + normal.inv_cdf(power)


def mde(n, discordant_rate, alpha=ALPHA, power=POWER):
    """Minimum detectable effect (a proportion difference) for n paired questions."""
    if not isinstance(n, int) or n <= 0:
        raise ValueError('n must be a positive int')
    if not 0 <= discordant_rate <= 1:
        raise ValueError('discordant rate in [0, 1]')
    return coefficient(alpha, power) * math.sqrt(discordant_rate / n)


def required_n(effect, discordant_rate, alpha=ALPHA, power=POWER):
    """Smallest n whose MDE is at most `effect`."""
    if effect <= 0:
        raise ValueError('effect must be positive')
    if not 0 <= discordant_rate <= 1:
        raise ValueError('discordant rate in [0, 1]')
    return max(1, math.ceil(coefficient(alpha, power) ** 2 * discordant_rate / effect ** 2 - 1e-12))


@dataclass(frozen=True)
class Power:
    """The report's `power` row (SCHEMAS.md §5)."""
    metric: str
    n: int
    discordant_rate: float | None
    mde: float | None
    effect: float | None
    decidable: bool

    def as_dict(self):
        return {'metric': self.metric, 'n': self.n, 'discordant_rate': self.discordant_rate,
                'mde': self.mde, 'effect': self.effect, 'decidable': self.decidable}


def assess(metric, n, discordant_rate, effect, alpha=ALPHA, power=POWER):
    """Decidable only when n, d and the pre-registered effect are known and MDE <= effect."""
    value = None if n <= 0 or discordant_rate is None else mde(n, discordant_rate, alpha, power)
    decidable = value is not None and effect is not None and value <= effect
    return Power(metric, n, discordant_rate, value, effect, decidable)


def verdict_or_indecidible(assessment, verdict):
    """The verdict a comparison earned, or `indecidible` when it lacked the power to earn one."""
    return verdict if assessment.decidable else INDECIDIBLE
