"""json_compact_lexical_v1: captured lexemes with whitespace outside strings removed.

Numbers, escapes and key order are kept exactly as captured; nothing is reparsed.
"""
from ..domain.representation import RepresentationId, RepresentationUnit

JSON_WHITESPACE = frozenset(' \t\n\r')


def compact_lexical(text):
    out = []
    inside = escaped = False
    for char in text:
        if inside:
            out.append(char)
            if escaped:
                escaped = False
            elif char == '\\':
                escaped = True
            elif char == '"':
                inside = False
        elif char == '"':
            inside = True
            out.append(char)
        elif char not in JSON_WHITESPACE:
            out.append(char)
    if inside:
        raise ValueError('Unterminated JSON string')
    return ''.join(out)


def units(exposure, payload, raw_text):
    return [RepresentationUnit(exposure.exposure_id, exposure.exposure_id,
                               RepresentationId.JSON_COMPACT_LEXICAL_V1, compact_lexical(raw_text))]
