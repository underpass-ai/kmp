"""JSON parsing that rejects duplicate keys and records top-level member spans."""
import json

from ..domain.errors import CaptureIntegrityError

WHITESPACE = ' \t\n\r'
DECODER = json.JSONDecoder()


def unique_pairs(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise CaptureIntegrityError('duplicate JSON key ' + repr(key))
        value[key] = item
    return value


def loads(text):
    return json.loads(text, object_pairs_hook=unique_pairs)


def _skip(text, index):
    while index < len(text) and text[index] in WHITESPACE:
        index += 1
    return index


def member_spans(text):
    """Map each top-level key of a JSON object to the exact (start, end) of its value."""
    index = _skip(text, 0)
    if text[index:index + 1] != '{':
        raise CaptureIntegrityError('trace event is not a JSON object')
    index = _skip(text, index + 1)
    spans = {}
    if text[index:index + 1] == '}':
        return spans
    while True:
        key, index = DECODER.raw_decode(text, index)
        index = _skip(text, index)
        if not isinstance(key, str) or text[index:index + 1] != ':':
            raise CaptureIntegrityError('malformed trace event member')
        start = _skip(text, index + 1)
        _, end = DECODER.raw_decode(text, start)
        spans[key] = (start, end)
        index = _skip(text, end)
        if text[index:index + 1] == ',':
            index = _skip(text, index + 1)
        elif text[index:index + 1] == '}':
            break
        else:
            raise CaptureIntegrityError('malformed trace event object')
    if _skip(text, index + 1) != len(text):
        raise CaptureIntegrityError('trailing data after trace event')
    return spans
