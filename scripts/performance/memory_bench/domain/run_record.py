"""Run records: one line per measured tool call, one per journey, one manifest per run.

kmp.bench.call.v1 is what BT15 writes and BT09/BT10/BT11 read. A run directory
is also a token_harness capture (manifest.json + one trace per journey), so
`response_path` points at the exact wire lexemes of the response inside that
trace instead of copying them. Measured values follow token_harness'
Measurement rule: absent is None with a reason, never an invented zero.
"""
from dataclasses import dataclass
import re

from . import cachekey
from .errors import RunRecordInvalid
from .fields import Fields

CALL_SCHEMA = 'kmp.bench.call.v1'
JOURNEY_SCHEMA = 'kmp.bench.journey.v1'
RUN_SCHEMA = 'kmp.bench.run.v1'
MAX_CALLS_LIMIT = 256  # section 1 item 12: a project:made wake asks for about 87 pages
QUESTION_ID = r'[A-Za-z0-9][A-Za-z0-9_.-]{0,127}'
VARIANT = r'[a-z0-9][a-z0-9_-]{0,63}'
JOURNEY = r'[A-Za-z0-9][A-Za-z0-9_.~-]{0,191}'
TOOL = r'kmp_[a-z_]{1,40}'
RESPONSE_PATH = re.compile(r'(?P<trace>[A-Za-z0-9][A-Za-z0-9_.~-]{0,191}\.jsonl)#(?P<line>0|[1-9][0-9]*)')
PHASES = ('first', 'warm', 'repeat')
CALL_STATUSES = ('ok', 'tool_error', 'rpc_error', 'transport_error')
# token_harness.native.driver.JourneyOutcome statuses, unchanged.
JOURNEY_STATUSES = ('completed', 'call_cap_reached', 'tool_error', 'rpc_error', 'transport_error')
JEV_SOURCES = ('remote', 'cassette_hit', 'cassette_miss', 'book_hit')
IO_FIELDS = ('rchar', 'wchar', 'syscr', 'syscw', 'read_bytes', 'write_bytes', 'cancelled_write_bytes')
CALL_MEASURED = ('response_path', 'response_sha256', 'response_bytes', 'wall_ns', 'server_ms',
                 'server_us', 'cpu_ns', 'rss_peak_kb', 'io', 'jev')
JOURNEY_MEASURED = ('startup_ns',)


def _hex(fields, key, required=True):
    return fields.text(key, required, cachekey.HEX64.pattern)


def _absent(fields, measured, record):
    reasons = fields.mapping('absent') or {}
    for name, reason in reasons.items():
        if name not in measured:
            fields.fail('absent', f'{name} is not a measured field')
        if not isinstance(reason, str) or not reason:
            fields.fail('absent', f'{name} needs a non-empty reason')
    for name in measured:
        if (getattr(record, name) is None) != (name in reasons):
            fields.fail(name, 'a measured value is either present or absent with a reason')
    return dict(sorted(reasons.items()))


@dataclass(frozen=True)
class JevEvaluation:
    """One `kmp_mcp::judgement` telemetry line (BT03): an evaluate at one site."""
    site: str  # p rerank/wake, r focus/write-relations, w/n/c paths, t typing, s/b audit
    questions: int
    requests: int
    input_tokens: int
    source: str
    us: int

    @classmethod
    def from_dict(cls, data, where):
        fields = Fields(data, where, RunRecordInvalid)
        value = cls(fields.text('site', pattern=r'[a-z][a-z0-9_]{0,31}'),
                    fields.integer('questions', minimum=0), fields.integer('requests', minimum=0),
                    fields.integer('input_tokens', minimum=0),
                    fields.text('source', choices=JEV_SOURCES), fields.integer('us', minimum=0))
        fields.done()
        return value

    def as_dict(self):
        return {'site': self.site, 'questions': self.questions, 'requests': self.requests,
                'input_tokens': self.input_tokens, 'source': self.source, 'us': self.us}


def _io(fields):
    value = fields.mapping('io')
    if value is None:
        return None
    counters = Fields(value, fields.where + '.io', RunRecordInvalid)
    result = {name: counters.integer(name, minimum=0) for name in IO_FIELDS}
    counters.done()
    return result


def _jev(fields):
    value = fields.mapping('jev')
    if value is None:
        return None
    table = Fields(value, fields.where + '.jev', RunRecordInvalid)
    evaluations = tuple(JevEvaluation.from_dict(item, f'{table.where}.evaluations[{i}]')
                        for i, item in enumerate(table.objects('evaluations')))
    table.done()
    return evaluations


@dataclass(frozen=True)
class CallRecord:
    run_id: str
    question_id: str
    variant: str
    sample: int
    repeat: int
    journey: str
    call_index: int
    process_call_index: int
    phase: str
    binary_sha256: str
    tool: str
    arguments: dict
    status: str
    rpc_id: int | None = None
    response_path: str | None = None
    response_sha256: str | None = None
    response_bytes: int | None = None
    wall_ns: int | None = None
    server_ms: int | None = None
    server_us: int | None = None
    cpu_ns: int | None = None
    rss_peak_kb: int | None = None
    io: dict | None = None
    jev: tuple | None = None  # JevEvaluation items; () is a binary that judged nothing
    censored: bool = False
    censor_reason: str | None = None
    absent: dict | None = None

    @classmethod
    def from_dict(cls, data, where='call'):
        f = Fields(data, where, RunRecordInvalid)
        f.text('schema', choices=(CALL_SCHEMA,))
        record = cls(
            run_id=_hex(f, 'run_id'), question_id=f.text('question_id', pattern=QUESTION_ID),
            variant=f.text('variant', pattern=VARIANT), sample=f.integer('sample', minimum=0),
            repeat=f.integer('repeat', minimum=0), journey=f.text('journey', pattern=JOURNEY),
            call_index=f.integer('call_index', minimum=0, maximum=MAX_CALLS_LIMIT - 1),
            process_call_index=f.integer('process_call_index', minimum=0),
            phase=f.text('phase', choices=PHASES), binary_sha256=_hex(f, 'binary_sha256'),
            tool=f.text('tool', pattern=TOOL), arguments=dict(f.mapping('arguments', required=True)),
            status=f.text('status', choices=CALL_STATUSES),
            rpc_id=f.integer('rpc_id', required=False, minimum=0),
            response_path=f.text('response_path', False, RESPONSE_PATH.pattern),
            response_sha256=_hex(f, 'response_sha256', False),
            response_bytes=f.integer('response_bytes', False, 0),
            wall_ns=f.integer('wall_ns', False, 0), server_ms=f.integer('server_ms', False, 0),
            server_us=f.integer('server_us', False, 0), cpu_ns=f.integer('cpu_ns', False, 0),
            rss_peak_kb=f.integer('rss_peak_kb', False, 0), io=_io(f), jev=_jev(f),
            censored=f.flag('censored', False),
            censor_reason=f.text('censor_reason', False, choices=('timeout',)))
        record = cls(**{**record.__dict__, 'absent': _absent(f, CALL_MEASURED, record)})
        f.done()
        if record.status == 'ok' and record.response_path is None:
            f.fail('response_path', 'an ok call points at its response')
        if record.censored != (record.censor_reason is not None):
            f.fail('censored', 'censored and censor_reason go together')
        if record.censored and (record.status != 'transport_error' or record.wall_ns is None):
            f.fail('censored', 'a timed-out call is a transport_error whose wall_ns is a lower bound')
        return record

    def as_dict(self):
        return {'schema': CALL_SCHEMA, 'run_id': self.run_id, 'question_id': self.question_id,
                'variant': self.variant, 'sample': self.sample, 'repeat': self.repeat,
                'journey': self.journey, 'call_index': self.call_index,
                'process_call_index': self.process_call_index, 'phase': self.phase,
                'binary_sha256': self.binary_sha256, 'tool': self.tool,
                'arguments': dict(self.arguments), 'status': self.status, 'rpc_id': self.rpc_id,
                'response_path': self.response_path, 'response_sha256': self.response_sha256,
                'response_bytes': self.response_bytes, 'wall_ns': self.wall_ns,
                'server_ms': self.server_ms, 'server_us': self.server_us, 'cpu_ns': self.cpu_ns,
                'rss_peak_kb': self.rss_peak_kb, 'io': None if self.io is None else dict(self.io),
                'jev': None if self.jev is None else {'evaluations': [e.as_dict() for e in self.jev]},
                'censored': self.censored, 'censor_reason': self.censor_reason,
                'absent': dict(self.absent or {})}

    def response_location(self):
        """(trace file name inside the run directory, 0-based line index) or None."""
        return parse_response_path(self.response_path) if self.response_path else None


def parse_response_path(value):
    match = RESPONSE_PATH.fullmatch(value or '')
    if match is None:
        raise RunRecordInvalid(f'response_path {value!r} is not <trace>.jsonl#<line>')
    return match['trace'], int(match['line'])


@dataclass(frozen=True)
class JourneyRecord:
    run_id: str
    question_id: str
    variant: str
    sample: int
    repeat: int
    journey: str
    tool: str
    status: str
    calls: int
    max_calls: int
    censored: bool = False
    censor_reason: str | None = None
    startup_ns: int | None = None
    error: str | None = None
    absent: dict | None = None

    @classmethod
    def from_dict(cls, data, where='journey'):
        f = Fields(data, where, RunRecordInvalid)
        f.text('schema', choices=(JOURNEY_SCHEMA,))
        record = cls(
            run_id=_hex(f, 'run_id'), question_id=f.text('question_id', pattern=QUESTION_ID),
            variant=f.text('variant', pattern=VARIANT), sample=f.integer('sample', minimum=0),
            repeat=f.integer('repeat', minimum=0), journey=f.text('journey', pattern=JOURNEY),
            tool=f.text('tool', pattern=TOOL), status=f.text('status', choices=JOURNEY_STATUSES),
            calls=f.integer('calls', minimum=0, maximum=MAX_CALLS_LIMIT),
            max_calls=f.integer('max_calls', minimum=1, maximum=MAX_CALLS_LIMIT),
            censored=f.flag('censored', False),
            censor_reason=f.text('censor_reason', False, choices=('timeout', 'max_calls')),
            startup_ns=f.integer('startup_ns', False, 0), error=f.text('error', required=False))
        record = cls(**{**record.__dict__, 'absent': _absent(f, JOURNEY_MEASURED, record)})
        f.done()
        if record.calls > record.max_calls:
            f.fail('calls', 'more calls than max_calls')
        if record.censored != (record.censor_reason is not None):
            f.fail('censored', 'censored and censor_reason go together')
        if (record.status == 'call_cap_reached') != (record.censor_reason == 'max_calls'):
            f.fail('censor_reason', 'a journey stopped by max_calls is censored by max_calls')
        return record

    def as_dict(self):
        return {'schema': JOURNEY_SCHEMA, 'run_id': self.run_id, 'question_id': self.question_id,
                'variant': self.variant, 'sample': self.sample, 'repeat': self.repeat,
                'journey': self.journey, 'tool': self.tool, 'status': self.status,
                'calls': self.calls, 'max_calls': self.max_calls, 'censored': self.censored,
                'censor_reason': self.censor_reason, 'startup_ns': self.startup_ns,
                'error': self.error, 'absent': dict(self.absent or {})}


def journey_name(question_id, sample, repeat):
    """Trace name of one question journey; never collides with run.json/calls/journeys."""
    return f'{question_id}~s{sample}~r{repeat}'
