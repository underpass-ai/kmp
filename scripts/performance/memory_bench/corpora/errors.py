"""Typed failures of the real-store corpora (hard negatives, identifier guards)."""
from ..domain.errors import BenchError


class StoreUnreadable(BenchError):
    """The real-store copy cannot be read as a KMP SQLite store."""
    code = 'STORE_UNREADABLE'


class ProbeFailed(BenchError):
    """kmp_search_probe refused its input or answered out of contract."""
    code = 'SEARCH_PROBE_FAILED'


class NegativeRulesInvalid(BenchError):
    """negatives.toml breaks its schema, or its hash is not the registered one."""
    code = 'NEGATIVE_RULES_INVALID'


class NegativesShortfall(BenchError):
    """A type or form ended with fewer verified questions than the rules require."""
    code = 'NEGATIVES_SHORTFALL'


class AuditInvalid(BenchError):
    """An audit verdict file does not match the sample it audits."""
    code = 'AUDIT_INVALID'
