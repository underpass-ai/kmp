"""Deterministic stand-ins: a counter that is not a tokenizer, and a synthetic capture."""
import gzip
import hashlib
import json
from pathlib import Path

from ..domain.tokenizer import TokenizerIdentity


class FakeCounter:
    """Counts characters; enough to test our accounting, not tiktoken."""

    def __init__(self, encoding='fake_chars'):
        self._identity = TokenizerIdentity('fake', '0', encoding, 'encode_ordinary', 'x', 'y')

    @property
    def identity(self):
        return self._identity

    def count(self, text):
        return len(text)


def rpc(identifier, method, params, result):
    return {'request': {'jsonrpc': '2.0', 'id': identifier, 'method': method, 'params': params},
            'response': {'jsonrpc': '2.0', 'id': identifier, 'result': result}}


def tool(identifier, name, arguments, structured):
    return rpc(identifier, 'tools/call', {'name': name, 'arguments': arguments},
               {'content': [{'type': 'text', 'text': json.dumps(structured)}],
                'structuredContent': structured})


def sample_events():
    return [
        {'preparation': 'guide sync in isolated store', 'model_calls': 0},
        rpc(1, 'initialize', {'protocolVersion': '2024-11-05'}, {'serverInfo': {'name': 'kmp'}}),
        {'request': {'jsonrpc': '2.0', 'method': 'notifications/initialized'}},
        rpc(2, 'tools/list', {}, {'tools': []}),
        tool(3, 'kmp_guide', {'topic': 'start'}, {'page': {'has_more': True, 'continuation': 'c-guide'}}),
        tool(4, 'kmp_recall', {'continuation': 'c-guide'}, {'text': 'guide page two'}),
        tool(5, 'kmp_recall', {'about': 'project:x', 'query': 'why'}, {'text': 'memory  body', 'n': 1.0e10}),
        rpc(6, 'tools/call', {'name': 'kmp_status', 'arguments': {}},
            {'content': [{'type': 'text', 'text': 'plain text only'}]}),
    ]


def write_capture(folder, events_by_lesson):
    """Write a journey directory the way acceptance_surface_journeys.py does."""
    folder = Path(folder)
    folder.mkdir(parents=True)
    files, rows = {}, []
    for lesson, events in events_by_lesson.items():
        raw = ''.join(json.dumps(e, ensure_ascii=False) + '\n' for e in events).encode()
        originals = {lesson + '.jsonl': raw, lesson + '.stdout': b'{}\n', lesson + '.stderr': b''}
        for name, content in originals.items():
            files[name] = {'bytes': len(content), 'sha256': hashlib.sha256(content).hexdigest()}
            if name.endswith('.jsonl'):
                (folder / (name + '.gz')).write_bytes(gzip.compress(content, mtime=0))
            else:
                (folder / name).write_bytes(content)
        rows.append({'lesson': lesson, 'command': [], 'exit_code': 0, 'client_total_seconds': 0})
    manifest = {'binary_sha256': 'synthetic', 'model_calls': 0, 'journeys': rows,
                'uncompressed_files': files}
    (folder / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return folder
