"""Every Jev evaluation a process logged, keyed as its cassette/book entry (SCHEMAS.md, Telemetry).

`server_log` owns the journal: it reads the files (`LogCursor`), attributes the
`kmp_judgement` lines to their tool call and turns the successful ones into
`JevEvaluation`. What it drops on purpose is what the Jev fixtures need: the
`request_key` of every line (the sha256 of the provider body, which is also
the cassette key), its model and whether it failed. This module keeps exactly
that, next to server_log's own parse of the same lines, and adds nothing else.

A binary older than BT03 logs no `kmp_judgement` line and no `duration_us`;
its log is `telemetry_present = False` with server_log's reason, never an
empty "nothing was judged".
"""
from collections import Counter
from dataclasses import dataclass
import json

from . import server_log

REPLAY_SOURCES = ('cassette_hit', 'book_hit')  # answered without asking the provider


@dataclass(frozen=True)
class JudgementLine:
    """One `kmp_judgement` line, with the fields the fixture join needs."""
    site: str
    model: str | None
    source: str | None
    status: str
    request_key: str | None
    input_tokens: int | None  # absent on error, never 0
    error_hash: str | None

    @property
    def failed(self):
        return self.status != 'ok'


@dataclass(frozen=True)
class JudgementLog:
    lines: tuple  # JudgementLine, journal order
    absent_reason: str | None  # set when the binary predates the telemetry

    @property
    def telemetry_present(self):
        return self.absent_reason is None

    def request_keys(self):
        return frozenset(line.request_key for line in self.lines if line.request_key)

    def by_source(self):
        """{source: evaluations}; `error` counts the failed ones whatever their source."""
        return dict(sorted(Counter('error' if line.failed else str(line.source)
                                   for line in self.lines).items()))

    def asked_provider(self):
        """Lines answered neither by a cassette nor by the book: record them, never replay them."""
        return tuple(line for line in self.lines if line.failed or line.source not in REPLAY_SOURCES)


def _integer(value):
    return value if isinstance(value, int) and not isinstance(value, bool) else None


def _text(value):
    return value if isinstance(value, str) and value else None


def judgement_lines(lines):
    """The `kmp_judgement` events among journal lines; anything else, or unparsable, is skipped."""
    found = []
    for line in lines:
        try:
            fields = json.loads(line).get('fields') or {}
        except (ValueError, AttributeError):
            continue
        if not isinstance(fields, dict) or fields.get('event') != server_log.JUDGEMENT_EVENT:
            continue
        found.append(JudgementLine(str(fields.get('site')), _text(fields.get('model')),
                                   _text(fields.get('source')), str(fields.get('status')),
                                   _text(fields.get('request_key')), _integer(fields.get('input_tokens')),
                                   _text(fields.get('error_hash'))))
    return tuple(found)


def _absent_reason(telemetry, lines):
    if lines:
        return None
    if any(call.tool.duration_us is not None for call in telemetry.calls):
        return None  # BT03 binary that judged nothing
    return server_log.NO_JUDGEMENT


def parse(lines, telemetry=None):
    """`telemetry` is server_log's parse of the same lines when the caller already has it."""
    lines = list(lines)
    telemetry = server_log.parse(lines) if telemetry is None else telemetry
    found = judgement_lines(lines)
    return JudgementLog(found, _absent_reason(telemetry, found))


def merge(logs):
    """The lines of several processes of one sample; telemetry counts as present if any had it."""
    logs = tuple(logs)
    present = any(log.telemetry_present for log in logs)
    return JudgementLog(tuple(line for log in logs for line in log.lines),
                        None if present else server_log.NO_JUDGEMENT)
