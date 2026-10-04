"""Trace lines that keep the exact wire lexemes of every request and response.

Each line is assembled by concatenation, so `request` and `response` hold the
bytes that crossed stdio, not a re-serialization; the I0 lexical view counts
them as sent. Harness markers carry no `request`/`response` and are metadata.
Lines are counted from 0, the meter's `event_index`, so `pair` can return the
`<file>#<line>` path of the response it just wrote.
"""
import json


class TraceRecorder:
    def __init__(self, path, redact=None):
        self.path = path
        self.redact = redact or (lambda text: text)
        self.handle = path.open('x', encoding='utf-8')  # never overwrite evidence
        self.lines = 0

    def _session(self, session):
        return json.dumps(session, ensure_ascii=False)

    def pair(self, session, raw_request, raw_response):
        """Write one exchange; return where its response lives, `<file>#<line>`."""
        index = self.lines
        self._write('{"session":' + self._session(session) + ',"request":' + raw_request
                    + ',"response":' + raw_response + '}')
        return f'{self.path.name}#{index}'

    def notification(self, session, raw_request):
        self._write('{"session":' + self._session(session) + ',"request":' + raw_request + '}')

    def marker(self, event):
        self._write(self.redact(json.dumps(event, ensure_ascii=False)))

    def _write(self, line):
        if '\n' in line:
            raise ValueError('a trace event must be one line')
        self.handle.write(line + '\n')
        self.handle.flush()
        self.lines += 1

    def close(self):
        self.handle.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
