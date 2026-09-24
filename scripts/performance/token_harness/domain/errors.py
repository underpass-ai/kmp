"""Typed harness failures. Each carries a stable code the report can quote."""


class HarnessError(Exception):
    code = 'HARNESS_ERROR'

    def __init__(self, message):
        super().__init__(f'{self.code}: {message}')
        self.detail = message


class TokenizerNotReady(HarnessError):
    """The pinned library or a verified local asset is unavailable; never download instead."""
    code = 'TOKENIZER_NOT_READY'


class MeasurementLimitExceeded(HarnessError):
    """A unit is larger than the configured limit; it is never truncated and counted."""
    code = 'MEASUREMENT_LIMIT_EXCEEDED'


class CaptureIntegrityError(HarnessError):
    """The capture does not match its manifest, or its layout is not trusted."""
    code = 'CAPTURE_INTEGRITY_MISMATCH'


class AggregationError(HarnessError):
    """Measurements cannot be summed: duplicated exposure or mixed series."""
    code = 'AGGREGATION_REJECTED'
