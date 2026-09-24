"""Whole-unit counting and the only aggregation allowed over measurements."""
from collections import Counter
from dataclasses import dataclass, field

from .errors import AggregationError, MeasurementLimitExceeded
from .measurement import Measurement
from .ports import TokenCounter
from .representation import RepresentationUnit

DEFAULT_MAX_UNIT_BYTES = 64 * 1024 * 1024


def measure_unit(counter: TokenCounter, unit: RepresentationUnit,
                 max_unit_bytes=DEFAULT_MAX_UNIT_BYTES) -> Measurement:
    """Tokenize the unit entire; oversize raises instead of truncating."""
    def absent(reason):
        return Measurement(unit.unit_id, unit.exposure_id, unit.representation,
                           counter.identity, None, None, reason)

    if unit.text is None:
        return absent(unit.absent_reason)
    try:
        raw = unit.text.encode('utf-8')  # strict: a lone surrogate is not repaired
    except UnicodeEncodeError:
        return absent('invalid_unicode_text')
    if len(raw) > max_unit_bytes:
        raise MeasurementLimitExceeded(
            f'{unit.unit_id} has {len(raw)} bytes, limit {max_unit_bytes}')
    return Measurement(unit.unit_id, unit.exposure_id, unit.representation,
                       counter.identity, counter.count(unit.text), len(raw))


@dataclass(frozen=True)
class Total:
    """Sum of distinct units of one series (encoder x representation)."""
    units: int = 0
    tokens: int = 0
    utf8_bytes: int = 0
    absent: dict = field(default_factory=dict)

    def as_dict(self):
        return {'units': self.units, 'tokens': self.tokens,
                'utf8_bytes': self.utf8_bytes, 'absent': dict(sorted(self.absent.items()))}


def aggregate(measurements) -> Total:
    """Reject duplicate unit ids and mixed encoders or representations, then sum."""
    measurements = list(measurements)
    seen = set()
    for item in measurements:
        if item.unit_id in seen:
            raise AggregationError('duplicate exposure unit ' + item.unit_id)
        seen.add(item.unit_id)
    if len({m.tokenizer for m in measurements}) > 1:
        raise AggregationError('mixed tokenizers in one total')
    if len({m.representation for m in measurements}) > 1:
        raise AggregationError('mixed representations in one total')
    counted = [m for m in measurements if m.tokens is not None]
    absent = Counter(m.absent_reason for m in measurements if m.tokens is None)
    return Total(len(counted), sum(m.tokens for m in counted),
                 sum(m.utf8_bytes for m in counted), dict(absent))
