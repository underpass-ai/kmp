"""The tools/call exchanges of one verified journey trace, in order."""
from dataclasses import dataclass

from ..domain.exposure import ExposureKind


@dataclass(frozen=True)
class Call:
    ordinal: int  # 1-based among the journey's tools/call exchanges
    event_index: int  # trace line; the same index the meter's exposures carry
    tool: str
    arguments: dict
    response: dict  # whole JSON-RPC response

    @property
    def result(self):
        return self.response.get('result') or {}

    @property
    def structured(self):
        return self.result.get('structuredContent') or {}

    @property
    def failed(self):
        return 'error' in self.response or bool(self.result.get('isError'))


def tool_calls(trace):
    requests, calls = {}, []
    for item in trace.observed:
        exposure = item.exposure
        if exposure.method != 'tools/call':
            continue
        if exposure.kind is ExposureKind.REQUEST:
            requests[exposure.event_index] = item.payload
        elif exposure.kind is ExposureKind.RESPONSE and exposure.event_index in requests:
            params = requests[exposure.event_index].get('params') or {}
            calls.append(Call(len(calls) + 1, exposure.event_index, params.get('name'),
                              params.get('arguments') or {}, item.payload))
    return calls
