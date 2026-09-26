"""receipt.is_accepted reads a write exactly as kmp-testkit's WriteReceipt does."""
from pathlib import Path
import unittest

from ..domain import receipt

RUST = Path(__file__).resolve().parents[4] / 'crates/kmp-testkit/src/write_receipt.rs'


class ReceiptTest(unittest.TestCase):
    def test_table(self):
        cases = [
            ({'accepted': True, 'status': 'committed'}, True),
            ({'accepted': True, 'status': 'replayed'}, True),
            ({'accepted': True, 'status': 'unconfirmed'}, False),
            ({'accepted': False, 'status': 'validated'}, False),
            ({'accepted': False, 'status': 'needs_review'}, False),
            ({'accepted': True, 'status': 'committed', 'dry_run': True}, False),
            ({'memory': {'read_after_write_ready': True}}, True),  # canonical ingest
            ({'memory': {'read_after_write_ready': False}}, False),
            ({'accepted': 'yes', 'memory': {'read_after_write_ready': True}}, True),  # as_bool() is None
            ({}, False), (None, False),
        ]
        for structured, expected in cases:
            with self.subTest(structured=structured):
                self.assertIs(receipt.is_accepted(structured), expected)

    def test_review_action_is_followed_verbatim(self):
        pending = {'status': 'needs_review',
                   'next_actions': [{'tool': 'kmp_write_memory', 'arguments': {'review_token': 't'}}]}
        self.assertTrue(receipt.needs_review(pending))
        self.assertEqual(receipt.review_action(pending), ('kmp_write_memory', {'review_token': 't'}))
        for value in ({}, {'next_actions': []}, {'next_actions': [{'arguments': {}}]}, None):
            self.assertIsNone(receipt.review_action(value))

    def test_rust_rule_unchanged(self):
        source = RUST.read_text(encoding='utf-8')
        self.assertIn('Some("committed" | "replayed")', source)
        self.assertIn('content["memory"]["read_after_write_ready"].as_bool()', source)
        self.assertIn('content["dry_run"].as_bool().unwrap_or(false)', source)


if __name__ == '__main__':
    unittest.main()
