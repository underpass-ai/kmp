"""mcp_json_compact_legacy_v1: the whole request or response object, re-serialized."""
from ..domain.representation import RepresentationId, RepresentationUnit
from .compact_json import compact


def units(exposure, payload, raw_text):
    return [RepresentationUnit(exposure.exposure_id, exposure.exposure_id,
                               RepresentationId.MCP_JSON_COMPACT_LEGACY_V1, compact(payload))]
