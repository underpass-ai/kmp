"""A counted unit. Absent values are None with a reason, never an invented zero."""
from dataclasses import dataclass

from .representation import RepresentationId
from .tokenizer import TokenizerIdentity


@dataclass(frozen=True)
class Measurement:
    unit_id: str
    exposure_id: str
    representation: RepresentationId
    tokenizer: TokenizerIdentity
    tokens: int | None
    utf8_bytes: int | None
    absent_reason: str | None = None

    def __post_init__(self):
        counted = self.tokens is not None and self.utf8_bytes is not None
        absent = self.tokens is None and self.utf8_bytes is None
        if not (counted and self.absent_reason is None or absent and self.absent_reason):
            raise ValueError('A measurement is either fully counted or absent with a reason')

    def as_dict(self):
        return {'unit_id': self.unit_id, 'exposure_id': self.exposure_id,
                'representation': self.representation.value,
                'encoding': self.tokenizer.encoding, 'tokens': self.tokens,
                'utf8_bytes': self.utf8_bytes, 'absent_reason': self.absent_reason}
