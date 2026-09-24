"""Turn one verified journey trace into exposures, keeping every failure visible.

Trace lines come from guide_examples replays: paired `{request, response}`
events, plus harness events (preparation, session markers, fixtures) that are
metadata and never count as KMP tokens. A request without a response is an
unpaired failure; a request without a JSON-RPC id is a notification and
legitimately has none.
"""
from collections import Counter
from dataclasses import dataclass, field

from ..domain.errors import CaptureIntegrityError
from ..domain.exposure import Exposure, ExposureKind, Stage
from . import strict_json
from .stages import StageClassifier

HARNESS_MARKERS = ('preparation', 'session_start', 'session_end', 'fixture')


@dataclass(frozen=True)
class ObservedExposure:
    exposure: Exposure
    payload: dict
    raw_text: str  # exact captured lexemes of this side of the event


@dataclass
class JourneyTrace:
    journey: str
    observed: list = field(default_factory=list)
    harness_events: Counter = field(default_factory=Counter)
    failures: list = field(default_factory=list)
    rpc_pairs: int = 0
    notifications: int = 0

    def summary(self):
        return {'journey': self.journey, 'rpc_pairs': self.rpc_pairs,
                'notifications': self.notifications,
                'harness_events': dict(sorted(self.harness_events.items())),
                'failures': self.failures}


def _lines(raw, journey):
    for number, line in enumerate(raw.split(b'\n')):
        if not line.strip():
            continue
        try:
            text = line.decode('utf-8')
        except UnicodeDecodeError as error:
            raise CaptureIntegrityError(f'{journey} line {number} is not UTF-8') from error
        try:
            yield number, text, strict_json.loads(text), strict_json.member_spans(text)
        except ValueError as error:
            raise CaptureIntegrityError(f'{journey} line {number}: {error}') from error


def _object(value, journey, index, side):
    if value is not None and not isinstance(value, dict):
        raise CaptureIntegrityError(f'{journey} line {index}: {side} is not an object')
    return value


def read_trace(journey, raw):
    trace, stages = JourneyTrace(journey), StageClassifier()
    for index, text, event, spans in _lines(raw, journey):
        session = event.get('session') if isinstance(event.get('session'), str) else None
        request = _object(event.get('request'), journey, index, 'request')
        response = _object(event.get('response'), journey, index, 'response')
        method = request.get('method') if request else None
        tool = (request.get('params') or {}).get('name') if method == 'tools/call' else None

        def observe(kind, side, stage):
            payload = event[side]
            start, end = spans[side]
            exposure = Exposure(f'{journey}/{index:05d}/{kind.value}', journey, index, kind,
                                stage, method, tool, payload.get('id'), session)
            trace.observed.append(ObservedExposure(exposure, payload, text[start:end]))

        def fail(code):
            trace.failures.append({'code': code, 'journey': journey, 'event_index': index})

        if request is None and response is None:
            marker = next((key for key in HARNESS_MARKERS if key in event), 'other')
            trace.harness_events[marker] += 1
        elif response is None and 'id' not in request:
            trace.notifications += 1
            observe(ExposureKind.NOTIFICATION, 'request', stages.classify(request))
        elif response is None:
            observe(ExposureKind.REQUEST, 'request', stages.classify(request))
            fail('UNPAIRED_REQUEST')
        elif request is None:
            observe(ExposureKind.RESPONSE, 'response', Stage.UNATTRIBUTED)
            fail('RESPONSE_WITHOUT_REQUEST')
        else:
            stage = stages.classify(request)
            observe(ExposureKind.REQUEST, 'request', stage)
            observe(ExposureKind.RESPONSE, 'response', stage)
            trace.rpc_pairs += 1
            if response.get('id') != request.get('id'):
                fail('RESPONSE_ID_MISMATCH')
            stages.observe(response, stage)
    return trace
