"""The only conclusion states a paired row may carry (annex A.14)."""
from dataclasses import dataclass
from enum import Enum


class Conclusion(str, Enum):
    REDUCTION_WITH_QUALITY_PASS = 'reference_reduction_with_quality_pass'
    INCREASE_WITH_QUALITY_PASS = 'reference_increase_with_quality_pass'
    QUALITY_FIX = 'quality_fix_not_equivalent_compression'
    QUALITY_REGRESSION = 'quality_regression_or_failure'
    DESCRIPTIVE_ONLY = 'descriptive_only'
    NOT_COMPARABLE = 'not_comparable'
    CAPTURE_FAILED = 'capture_failed'


@dataclass(frozen=True)
class Side:
    captured: bool  # a verified, fully paired trace and its measurement exist
    tokens: int | None
    quality_pass: bool | None  # None: no oracle verdict retained


def conclude(baseline, candidate, incompatibility=None):
    """(conclusion, reason). A smaller failing candidate is never a saving."""
    if not (baseline.captured and candidate.captured) or None in (baseline.tokens, candidate.tokens):
        return Conclusion.CAPTURE_FAILED, 'journey missing or incompletely captured'
    if incompatibility:
        return Conclusion.NOT_COMPARABLE, incompatibility
    if baseline.quality_pass is None or candidate.quality_pass is None:
        return Conclusion.DESCRIPTIVE_ONLY, 'oracle verdict missing'
    if not candidate.quality_pass:
        return Conclusion.QUALITY_REGRESSION, 'candidate fails the oracle'
    if not baseline.quality_pass:
        return Conclusion.QUALITY_FIX, 'baseline fails the oracle, candidate passes; delta descriptive'
    delta = candidate.tokens - baseline.tokens
    if delta < 0:
        return Conclusion.REDUCTION_WITH_QUALITY_PASS, None
    if delta > 0:
        return Conclusion.INCREASE_WITH_QUALITY_PASS, None
    return Conclusion.DESCRIPTIVE_ONLY, 'no reference delta'
