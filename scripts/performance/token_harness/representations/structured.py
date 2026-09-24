"""structured_compact_v1: result.structuredContent only, for responses.

A result without structuredContent is a legitimate text result: absent, not {}.
"""
from ..domain.exposure import ExposureKind
from ..domain.representation import RepresentationId, RepresentationUnit
from .compact_json import compact

MISSING = object()


def units(exposure, payload, raw_text):
    if exposure.kind is not ExposureKind.RESPONSE:
        return []

    def unit(text=None, reason=None):
        return [RepresentationUnit(exposure.exposure_id, exposure.exposure_id,
                                   RepresentationId.STRUCTURED_COMPACT_V1, text, reason)]

    if exposure.method != 'tools/call':
        return unit(reason='not_a_tool_result')
    result = payload.get('result')
    if not isinstance(result, dict):
        return unit(reason='no_result')
    structured = result.get('structuredContent', MISSING)
    if structured is MISSING:
        return unit(reason='text_only_result')
    if not isinstance(structured, dict):
        return unit(reason='structured_content_not_object')
    return unit(compact(structured))
