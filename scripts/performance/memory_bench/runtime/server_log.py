"""The server's own account of each call, read from its file journal (SCHEMAS.md, Telemetry).

With the embedded backend kmp-mcp writes every log line to
`<data dir>/logs/kmp-mcp.log.<UTC date>` as well as to stderr; the file is not
lossy, so the bench reads it and never parses stderr. A `LogCursor` remembers
how far each file had been read, so a process only sees its own lines even
when a store directory outlives it; a day boundary just adds a file.

Attribution needs no clock: tool calls of one process are serial, the server
logs exactly one `kmp_mcp_tool` line per `tools/call` when the call ends, and
every `kmp_judgement` line of a call is logged before that call's tool line.
So the n-th tool line belongs to the call with `process_call_index` n, and the
judgement lines just before it are that call's Jev evaluations.
"""
from dataclasses import dataclass
import json
from pathlib import Path

from ..domain.run_record import JevEvaluation

LOG_DIR = 'logs'
LOG_PREFIX = 'kmp-mcp.log.'
TOOL_EVENT = 'kmp_mcp_tool'
JUDGEMENT_EVENT = 'kmp_judgement'
STORE_CONFIG_EVENT = 'kmp_store_config'
STORE_CONFIG_SUMMARY = 'kmp_store_config_summary'
# Appended to token_harness' `kmp_mcp=info` (isolation.BASE_LOG_FILTER); a variant's
# RUST_LOG goes after it. `kmp_mcp::judgement=debug` alone would silence kmp_mcp_tool.
BENCH_LOG_FILTER = 'kmp_mcp::judgement=debug,kmp_mcp::store_config=debug'
NO_DURATION_US = 'binary logs duration_ms only (predates BT03 duration_us)'
NO_JUDGEMENT = 'binary emits no kmp_mcp::judgement telemetry (predates BT03)'
PREDATES_STORE_CONFIG = 'binary predates store_config telemetry'


class LogCursor:
    """Byte offsets into every journal file of one data directory."""

    def __init__(self, data_dir):
        self.folder = Path(data_dir) / LOG_DIR
        self.offsets = {path.name: path.stat().st_size for path in self._files()}

    def _files(self):
        if not self.folder.is_dir():
            return []
        return sorted(p for p in self.folder.iterdir() if p.name.startswith(LOG_PREFIX) and p.is_file())

    def read_new(self):
        """Complete lines appended since the last read, file (= date) order kept."""
        lines = []
        for path in self._files():
            start = self.offsets.get(path.name, 0)
            with path.open('rb') as handle:
                handle.seek(start)
                chunk = handle.read()
            complete = chunk[:chunk.rfind(b'\n') + 1]
            self.offsets[path.name] = start + len(complete)
            lines.extend(complete.decode('utf-8', errors='replace').splitlines())
        return lines


@dataclass(frozen=True)
class ToolLine:
    move: str
    status: str
    duration_ms: int | None
    duration_us: int | None


@dataclass(frozen=True)
class FailedJudgement:
    site: str
    source: str
    error_hash: str | None


@dataclass(frozen=True)
class CallTelemetry:
    tool: ToolLine
    evaluations: tuple  # JevEvaluation of the evaluations that returned ok
    failed: tuple = ()  # FailedJudgement: token fields absent, so the call's Jev cost is unknown

    def jev(self):
        """(value for CallRecord.jev, absent reason or None)."""
        if self.tool.duration_us is None:
            return None, NO_JUDGEMENT
        if self.failed:
            names = ', '.join(f'{f.site}/{f.source}/{f.error_hash}' for f in self.failed)
            return None, f'{len(self.failed)} Jev evaluation(s) failed ({names}); cost unknown'
        return self.evaluations, None


@dataclass(frozen=True)
class StoreConfigLine:
    file: str
    status: str
    sha256: str | None
    reason: str | None


@dataclass(frozen=True)
class ProcessTelemetry:
    calls: tuple  # CallTelemetry, in process_call_index order
    store_config: tuple  # StoreConfigLine
    store_config_reported: bool  # a summary line was seen: the binary has the telemetry
    unattributed_judgements: int  # judgement lines after the last tool line (a killed call)
    malformed: int

    def for_call(self, process_call_index, tool, trusted_below=None):
        """(CallTelemetry or None, reason when None).

        `trusted_below` bounds attribution when the process made more calls than it
        logged: after a call that may have left no line, positions no longer line up.
        """
        if process_call_index is None:
            return None, 'call never reached the server'
        if process_call_index >= len(self.calls):
            return None, 'no kmp_mcp_tool line for this call (process killed or journal cut)'
        if trusted_below is not None and process_call_index >= trusted_below:
            return None, 'kmp_mcp_tool lines out of step with tools/call after a failed call'
        found = self.calls[process_call_index]
        if found.tool.move != tool:
            return None, f'kmp_mcp_tool line names {found.tool.move}, the call was {tool}'
        return found, None


def _integer(value):
    return value if isinstance(value, int) and not isinstance(value, bool) else None


def _evaluation(fields):
    values = [_integer(fields.get(k)) for k in ('questions', 'requests', 'input_tokens', 'elapsed_us')]
    if None in values or fields.get('source') not in ('remote', 'cassette_hit', 'cassette_miss', 'book_hit'):
        return None
    questions, requests, input_tokens, elapsed_us = values
    return JevEvaluation(str(fields.get('site')), questions, requests, input_tokens,
                         fields['source'], elapsed_us)


def parse(lines):
    calls, config, pending, failed, malformed, summary = [], [], [], [], 0, False
    for line in lines:
        try:
            event = json.loads(line)
            fields = event.get('fields') or {}
            kind = fields.get('event')
        except (ValueError, AttributeError):
            malformed += 1
            continue
        if kind == TOOL_EVENT:
            calls.append(CallTelemetry(
                ToolLine(str(fields.get('kmp_move')), str(fields.get('status')),
                         _integer(fields.get('duration_ms')), _integer(fields.get('duration_us'))),
                tuple(pending), tuple(failed)))
            pending, failed = [], []
        elif kind == JUDGEMENT_EVENT:
            evaluation = _evaluation(fields) if fields.get('status') == 'ok' else None
            if evaluation is None:
                failed.append(FailedJudgement(str(fields.get('site')), str(fields.get('source')),
                                              fields.get('error_hash')))
            else:
                pending.append(evaluation)
        elif kind == STORE_CONFIG_EVENT:
            config.append(StoreConfigLine(str(fields.get('file')), str(fields.get('status')),
                                          fields.get('sha256'), fields.get('reason')))
        elif kind == STORE_CONFIG_SUMMARY:
            summary = True
    return ProcessTelemetry(tuple(calls), tuple(config), summary, len(pending) + len(failed), malformed)


@dataclass(frozen=True)
class StoreFileAck:
    name: str
    sha256: str
    applied: bool
    reason: str | None

    def as_dict(self):
        return {'name': self.name, 'sha256': self.sha256,
                'status': 'applied' if self.applied else 'not_applied', 'reason': self.reason}


def acknowledge(store_files, telemetry):
    """SCHEMAS.md section 4: a store file is applied iff a `loaded` line names it with its sha."""
    acks = []
    for name, sha256 in store_files:
        lines = [line for line in telemetry.store_config if line.file == name]
        if any(line.status == 'loaded' and line.sha256 == sha256 for line in lines):
            acks.append(StoreFileAck(name, sha256, True, None))
        elif lines:
            last = lines[-1]
            reason = last.reason or (f'loaded with sha256 {last.sha256}, copied {sha256}'
                                     if last.status == 'loaded' else f'status {last.status}')
            acks.append(StoreFileAck(name, sha256, False, reason))
        elif not telemetry.store_config_reported:
            acks.append(StoreFileAck(name, sha256, False, PREDATES_STORE_CONFIG))
        else:
            acks.append(StoreFileAck(name, sha256, False, 'no kmp_store_config line names it'))
    return tuple(acks)
