"""Typed bench failures. Each carries a stable code a report or a log can quote.

They extend token_harness' HarnessError so one handler covers both packages.
"""
from ...token_harness.domain.errors import HarnessError


class BenchError(HarnessError):
    code = 'BENCH_ERROR'


class QuestionInvalid(BenchError):
    """A question record breaks kmp.bench.question.v1."""
    code = 'QUESTION_INVALID'


class VariantInvalid(BenchError):
    """A variant TOML breaks kmp.bench.variant.v1 (including the env allowlist)."""
    code = 'VARIANT_INVALID'


class RunRecordInvalid(BenchError):
    """A call or journey record breaks kmp.bench.call.v1 / kmp.bench.journey.v1."""
    code = 'RUN_RECORD_INVALID'


class CacheKeyInvalid(BenchError):
    """Key material that cannot be hashed canonically, or a malformed digest."""
    code = 'CACHE_KEY_INVALID'


class PrivacyViolation(BenchError):
    """Private material was about to be written inside the repository."""
    code = 'PRIVATE_DATA_IN_REPO'


class NotImplementedYet(BenchError):
    """A CLI command whose task has not landed yet."""
    code = 'NOT_IMPLEMENTED'
