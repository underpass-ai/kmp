import unittest

from ..domain.accounting import aggregate, measure_unit
from ..domain.errors import AggregationError, MeasurementLimitExceeded
from ..domain.representation import RepresentationId, RepresentationUnit
from .fakes import FakeCounter

LEGACY = RepresentationId.MCP_JSON_COMPACT_LEGACY_V1


def unit(unit_id, text='abc', representation=LEGACY, reason=None):
    return RepresentationUnit(unit_id, unit_id, representation, text, reason)


class AccountingTest(unittest.TestCase):
    def test_sums_distinct_units(self):
        counter = FakeCounter()
        total = aggregate([measure_unit(counter, unit('a', 'ab')), measure_unit(counter, unit('b', 'é'))])
        self.assertEqual((total.units, total.tokens, total.utf8_bytes), (2, 3, 4))

    def test_duplicate_exposure_rejected(self):
        counter = FakeCounter()
        with self.assertRaises(AggregationError):
            aggregate([measure_unit(counter, unit('a')), measure_unit(counter, unit('a'))])

    def test_mixed_encoders_rejected(self):
        rows = [measure_unit(FakeCounter('one'), unit('a')), measure_unit(FakeCounter('two'), unit('b'))]
        with self.assertRaises(AggregationError):
            aggregate(rows)

    def test_mixed_representations_rejected(self):
        counter = FakeCounter()
        rows = [measure_unit(counter, unit('a')),
                measure_unit(counter, unit('b', representation=RepresentationId.STRUCTURED_COMPACT_V1))]
        with self.assertRaises(AggregationError):
            aggregate(rows)

    def test_absence_is_null_with_reason_not_zero(self):
        row = measure_unit(FakeCounter(), unit('a', None, reason='text_only_result'))
        self.assertIsNone(row.tokens)
        self.assertIsNone(row.utf8_bytes)
        total = aggregate([row])
        self.assertEqual((total.units, total.absent), (0, {'text_only_result': 1}))

    def test_oversize_unit_is_not_truncated(self):
        with self.assertRaises(MeasurementLimitExceeded):
            measure_unit(FakeCounter(), unit('a', 'x' * 11), max_unit_bytes=10)

    def test_lone_surrogate_is_marked_invalid(self):
        row = measure_unit(FakeCounter(), unit('a', '\ud800'))
        self.assertEqual(row.absent_reason, 'invalid_unicode_text')


if __name__ == '__main__':
    unittest.main()
