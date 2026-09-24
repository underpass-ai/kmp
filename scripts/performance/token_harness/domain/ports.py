"""Ports the application depends on; adapters implement them."""
from typing import Protocol

from .tokenizer import TokenizerIdentity


class TokenCounter(Protocol):
    @property
    def identity(self) -> TokenizerIdentity: ...

    def count(self, text: str) -> int:
        """Count one whole unit of ordinary text; special-token literals are text."""
        ...
