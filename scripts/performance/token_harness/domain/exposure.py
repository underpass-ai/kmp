"""One observed transport exposure: a request, a response or a notification."""
from dataclasses import dataclass
from enum import Enum


class ExposureKind(str, Enum):
    REQUEST = 'request'
    RESPONSE = 'response'
    NOTIFICATION = 'notification'


class Stage(str, Enum):
    STARTUP_CATALOGUE = 'startup_catalogue'
    GUIDE = 'guide'
    MEMORY = 'memory'
    UNATTRIBUTED = 'unattributed'


@dataclass(frozen=True)
class Exposure:
    exposure_id: str
    journey: str
    event_index: int
    kind: ExposureKind
    stage: Stage
    method: str | None
    tool: str | None
    jsonrpc_id: object
    session: str | None = None

    def as_dict(self):
        return {'exposure_id': self.exposure_id, 'event_index': self.event_index,
                'kind': self.kind.value, 'stage': self.stage.value, 'method': self.method,
                'tool': self.tool, 'jsonrpc_id': self.jsonrpc_id, 'session': self.session}
