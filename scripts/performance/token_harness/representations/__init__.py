"""Versioned projections from one captured exposure to whole textual units."""
from ..domain.representation import RepresentationId
from . import content_blocks, legacy, lexical, structured

BUILDERS = {
    RepresentationId.MCP_JSON_COMPACT_LEGACY_V1: legacy.units,
    RepresentationId.JSON_COMPACT_LEXICAL_V1: lexical.units,
    RepresentationId.STRUCTURED_COMPACT_V1: structured.units,
    RepresentationId.CONTENT_BLOCKS_STANDALONE_V1: content_blocks.units,
}


def units(representation, exposure, payload, raw_text):
    """Units of one representation for one exposure; may be empty when not applicable."""
    return BUILDERS[representation](exposure, payload, raw_text)
