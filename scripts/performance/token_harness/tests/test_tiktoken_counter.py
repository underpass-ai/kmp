import os
from pathlib import Path
import tempfile
import unittest

from ..adapters import tiktoken_counter
from ..domain.errors import TokenizerNotReady
from ..domain.tokenizer import TokenizerIdentity

INTEGRATION = os.environ.get('KMP_RUN_TIKTOKEN_INTEGRATION') == '1'
ROOT = Path(__file__).resolve().parents[4]


class SpecialRefusingEncoding:
    """Mimics tiktoken: encode() refuses special literals, encode_ordinary() does not."""

    def encode(self, text):
        if '<|endoftext|>' in text:
            raise ValueError('disallowed special token')
        return text.split()

    def encode_ordinary(self, text):
        return text.replace('<|', ' <| ').split()


class CounterPolicyTest(unittest.TestCase):
    def test_special_token_literal_is_ordinary_text(self):
        identity = TokenizerIdentity('tiktoken', '0.14.0', 'stub', 'encode_ordinary', 'a', 'b')
        counter = tiktoken_counter.TiktokenCounter(SpecialRefusingEncoding(), identity)
        self.assertEqual(counter.count('data <|endoftext|> more'), 4)


class AssetVerificationTest(unittest.TestCase):
    def setUp(self):
        self.saved = tiktoken_counter._cache_dir
        self.tmp = tempfile.TemporaryDirectory()
        tiktoken_counter._cache_dir = Path(self.tmp.name)

    def tearDown(self):
        tiktoken_counter._cache_dir = self.saved
        self.tmp.cleanup()

    def test_missing_asset_is_not_ready(self):
        with self.assertRaises(TokenizerNotReady):
            tiktoken_counter.load_counter('o200k_base')

    def test_altered_asset_is_not_ready(self):
        tiktoken_counter.asset_path('cl100k_base').write_bytes(b'altered')
        with self.assertRaises(TokenizerNotReady) as caught:
            tiktoken_counter.load_counter('cl100k_base')
        self.assertIn('altered', str(caught.exception))

    def test_unpinned_encoding_is_not_ready(self):
        with self.assertRaises(TokenizerNotReady):
            tiktoken_counter.load_counter('p50k_base')


@unittest.skipUnless(INTEGRATION, 'set KMP_RUN_TIKTOKEN_INTEGRATION=1 after `prepare`')
class TiktokenIntegrationTest(unittest.TestCase):
    """Enabled, it fails (never skips) when the pinned library or assets are missing."""

    def test_pinned_encoders_count_offline(self):
        tiktoken_counter.configure_cache(os.environ.get('TIKTOKEN_CACHE_DIR') or ROOT / 'tmp/tiktoken-cache')
        for encoding in tiktoken_counter.PINNED_ASSETS:
            counter = tiktoken_counter.load_counter(encoding)  # raises TOKENIZER_NOT_READY
            self.assertEqual(counter.identity.asset_sha256, tiktoken_counter.PINNED_ASSETS[encoding])
            self.assertEqual(counter.count('hello world'), 2)
            self.assertGreater(counter.count('<|endoftext|>'), 1)


if __name__ == '__main__':
    unittest.main()
