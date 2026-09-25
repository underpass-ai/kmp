"""What the harness observed around one JSON-RPC exchange, beside its wire bytes.

It becomes two trace markers, `timing` and `resources`, written right after the
exchange. Markers carry no `request`/`response`, so the meter never counts them.
"""
from dataclasses import dataclass


@dataclass(frozen=True)
class CallObservation:
    session: str
    rpc_id: int
    method: str
    tool: str | None
    process_call_index: int | None  # position among this process's tools/call; None otherwise
    started_ns: int  # request write, relative to the process spawn
    wall_ns: int  # request write to response read; a lower bound when censored
    response_path: str | None  # '<trace file>#<line>' of the paired event
    response_sha256: str | None
    response_bytes: int | None
    censored: bool
    censor_reason: str | None
    resources: dict  # rss_peak_kb, cpu_ns, io, absent (see resources.py)

    def timing_marker(self):
        return {'timing': {
            'session': self.session, 'rpc_id': self.rpc_id, 'method': self.method,
            'tool': self.tool, 'process_call_index': self.process_call_index,
            'started_ns': self.started_ns, 'wall_ns': self.wall_ns,
            'response_path': self.response_path, 'response_sha256': self.response_sha256,
            'response_bytes': self.response_bytes, 'censored': self.censored,
            'censor_reason': self.censor_reason}}

    def resources_marker(self):
        return {'resources': {'session': self.session, 'rpc_id': self.rpc_id, **self.resources}}
