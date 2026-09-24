"""Trace lines that keep the exact wire lexemes of every request and response.

Each line is assembled by concatenation, so `request` and `response` hold the
bytes that crossed stdio, not a re-serialization; the I0 lexical view counts
them as sent. Harness markers carry no `request`/`response` and are metadata.
"""
import json


class TraceRecorder:
    def __init__(self, path, redactions=()):
        self.path = path
        self.redactions = tuple(redactions)
        self.handle = path.open('x', encoding='utf-8')  # never overwrite evidence

    def _session(self, session):
        return json.dumps(session, ensure_ascii=False)

    def pair(self, session, raw_request, raw_response):
        self._write('{"session":' + self._session(session) + ',"request":' + raw_request
                    + ',"response":' + raw_response + '}')

    def notification(self, session, raw_request):
        self._write('{"session":' + self._session(session) + ',"request":' + raw_request + '}')

    def marker(self, event):
        text = json.dumps(event, ensure_ascii=False)
        for secret, public in self.redactions:
            text = text.replace(secret, public)
        self._write(text)

    def _write(self, line):
        if '\n' in line:
            raise ValueError('a trace event must be one line')
        self.handle.write(line + '\n')
        self.handle.flush()

    def close(self):
        self.handle.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
