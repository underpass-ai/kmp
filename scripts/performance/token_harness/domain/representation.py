"""Named analytic views of one exposure. Views overlap; their totals never add up."""
from dataclasses import dataclass
from enum import Enum


class RepresentationId(str, Enum):
    # Request/response object as surface_acceptance_metrics.py serialized it.
    MCP_JSON_COMPACT_LEGACY_V1 = 'mcp_json_compact_legacy_v1'
    # Captured JSON lexemes with whitespace outside strings removed.
    JSON_COMPACT_LEXICAL_V1 = 'json_compact_lexical_v1'
    # result.structuredContent alone, compact serializer.
    STRUCTURED_COMPACT_V1 = 'structured_compact_v1'
    # Each textual result.content block, standalone.
    CONTENT_BLOCKS_STANDALONE_V1 = 'content_blocks_standalone_v1'


@dataclass(frozen=True)
class RepresentationUnit:
    """One whole unit to tokenize, or an explained absence (text is None)."""
    unit_id: str
    exposure_id: str
    representation: RepresentationId
    text: str | None
    absent_reason: str | None = None

    def __post_init__(self):
        if (self.text is None) == (self.absent_reason is None):
            raise ValueError('A unit carries either text or an absent reason, not both')
