"""Typed failures of the public corpora (BENCH_SPEC section 4.6, BT17)."""
from ..domain.errors import BenchError


class CorpusError(BenchError):
    """A public dataset is malformed, or an adapter was asked for something it cannot build."""
    code = 'PUBLIC_CORPUS_INVALID'


class FetchFailed(BenchError):
    """A download failed, or its bytes do not match the lock."""
    code = 'DATASET_FETCH_FAILED'


class LicenceRefused(BenchError):
    """Material whose licence forbids it would land where it may not (the repository)."""
    code = 'DATASET_LICENCE_REFUSED'
