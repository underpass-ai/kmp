"""content_blocks_standalone_v1: each textual result.content block, alone.

Diagnostic only: it does not show how a host joins or transforms the blocks.
"""
from ..domain.exposure import ExposureKind
from ..domain.representation import RepresentationId, RepresentationUnit

VIEW = RepresentationId.CONTENT_BLOCKS_STANDALONE_V1


def units(exposure, payload, raw_text):
    if exposure.kind is not ExposureKind.RESPONSE:
        return []
    base = exposure.exposure_id + '/content'

    def absent(unit_id, reason):
        return RepresentationUnit(unit_id, exposure.exposure_id, VIEW, None, reason)

    if exposure.method != 'tools/call':
        return [absent(base, 'not_a_tool_result')]
    result = payload.get('result')
    if not isinstance(result, dict):
        return [absent(base, 'no_result')]
    content = result.get('content')
    if content is None:
        return [absent(base, 'no_content')]
    if not isinstance(content, list):
        return [absent(base, 'content_not_list')]
    rows = []
    for index, block in enumerate(content):
        unit_id = f'{base}/{index}'
        if isinstance(block, dict) and block.get('type') == 'text' and isinstance(block.get('text'), str):
            rows.append(RepresentationUnit(unit_id, exposure.exposure_id, VIEW, block['text']))
        else:
            rows.append(absent(unit_id, 'non_text_block'))  # binary/base64 is never tokenized
    return rows
