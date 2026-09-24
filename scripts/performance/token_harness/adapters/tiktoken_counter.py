"""Pinned tiktoken counter: verified local assets, encode_ordinary, no network.

Composition calls `configure_cache` once, before any thread starts. After that,
`load_counter` verifies the cached asset's SHA-256 against the pinned table and
disables tiktoken's downloader, so a missing or altered asset raises
TOKENIZER_NOT_READY instead of fetching a replacement mid-measurement.
"""
import hashlib
import importlib
import importlib.metadata
import os
from pathlib import Path

from ..domain.errors import TokenizerNotReady
from ..domain.tokenizer import TokenizerIdentity

LIBRARY = 'tiktoken'
PINNED_VERSION = '0.14.0'
POLICY = 'encode_ordinary'
ASSET_ROOT = 'https://openaipublic.blob.core.windows.net/encodings/'
# Pinned in tiktoken_ext/openai_public.py of tiktoken 0.14.0.
PINNED_ASSETS = {
    'o200k_base': '446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d',
    'cl100k_base': '223921b76ee99bde995b7ff738513eef100fb51d18c93597a113bcffe865b2a7',
}
_cache_dir = None


def configure_cache(directory):
    """Set TIKTOKEN_CACHE_DIR once per process; a second, different value is refused."""
    global _cache_dir
    directory = Path(directory).resolve()
    if _cache_dir is not None and _cache_dir != directory:
        raise TokenizerNotReady('tokenizer cache directory already set to ' + str(_cache_dir))
    os.environ['TIKTOKEN_CACHE_DIR'] = str(directory)
    _cache_dir = directory
    return directory


def asset_path(encoding):
    if _cache_dir is None:
        raise TokenizerNotReady('tokenizer cache directory not configured')
    # tiktoken keys its cache by the SHA-1 of the blob URL (tiktoken/load.py).
    key = hashlib.sha1((ASSET_ROOT + encoding + '.tiktoken').encode()).hexdigest()
    return _cache_dir / key


def verified_asset_sha256(encoding):
    if encoding not in PINNED_ASSETS:
        raise TokenizerNotReady('encoding not in the pinned table: ' + encoding)
    path = asset_path(encoding)
    if path.is_symlink() or not path.is_file():
        raise TokenizerNotReady(f'{encoding} asset missing at {path}; run `prepare` first')
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != PINNED_ASSETS[encoding]:
        raise TokenizerNotReady(f'{encoding} asset altered: sha256 {digest}')
    return digest


def _tiktoken():
    try:
        version = importlib.metadata.version(LIBRARY)
        module = importlib.import_module(LIBRARY)
    except (importlib.metadata.PackageNotFoundError, ImportError) as error:
        raise TokenizerNotReady(f'{LIBRARY}=={PINNED_VERSION} is not installed') from error
    if version != PINNED_VERSION:
        raise TokenizerNotReady(f'{LIBRARY} {version} installed, {PINNED_VERSION} pinned')
    return module


def _deny_network(blobpath):
    raise TokenizerNotReady('download attempted during measurement: ' + blobpath)


class TiktokenCounter:
    """TokenCounter over one tiktoken Encoding object."""

    def __init__(self, encoding_object, identity):
        self._encoding = encoding_object
        self._identity = identity

    @property
    def identity(self):
        return self._identity

    def count(self, text):
        # Ordinary text: literals such as <|endoftext|> are data, never control tokens.
        return len(self._encoding.encode_ordinary(text))


def load_counter(encoding):
    """Offline load of a pinned encoding; the asset is checked before tiktoken reads it."""
    asset_sha256 = verified_asset_sha256(encoding)
    tiktoken = _tiktoken()
    importlib.import_module('tiktoken.load').read_file = _deny_network
    encoding_object = tiktoken.get_encoding(encoding)
    identity = TokenizerIdentity(LIBRARY, PINNED_VERSION, encoding, POLICY, asset_sha256,
                                 hashlib.sha256(encoding_object._pat_str.encode()).hexdigest())
    return TiktokenCounter(encoding_object, identity)


def prepare_assets(encodings):
    """Network step, outside any measurement: fetch assets into the cache, then verify."""
    tiktoken = _tiktoken()
    rows = []
    for encoding in encodings:
        tiktoken.get_encoding(encoding)
        rows.append({'encoding': encoding, 'asset': str(asset_path(encoding)),
                     'sha256': verified_asset_sha256(encoding)})
    return rows
