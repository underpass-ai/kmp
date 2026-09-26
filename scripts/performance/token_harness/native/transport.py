"""One MCP stdio process per session; frames are recorded before they are interpreted.

Every exchange is timed (request write to response read, monotonic ns) and,
unless disabled, the child's resources are probed around it. Both go into the
trace as `timing` and `resources` markers right after the exchange. A call that
gets no answer within the session's timeout raises `TransportTimeout`
(`TRANSPORT_FAILED`), after writing its censored timing; the session is then
unusable and `close` kills the child.

The recorder can be swapped between journeys (`record_into`), so one process may
serve many questions while each question keeps its own trace file.
"""
import hashlib
import json
import os
import selectors
import subprocess
import time

from ..domain.errors import HarnessError
from .isolation import confirm_effective_store
from .observation import CallObservation
from .resources import NoProbe, ProcessProbe

PROTOCOL_VERSION = '2024-11-05'
TIMEOUT_SECONDS = 60
READ_CHUNK = 1 << 16


class TransportError(HarnessError):
    code = 'TRANSPORT_FAILED'


class TransportTimeout(TransportError):
    """No answer in time; `observation` holds the censored timing."""

    def __init__(self, message, observation):
        super().__init__(message)
        self.observation = observation


def _timeout(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not 0 < value <= 3600:
        raise HarnessError(f'timeout must be in (0, 3600] seconds, got {value!r}')
    return float(value)


class StdioSession:
    def __init__(self, binary, store, recorder, name, ids, *, timeout_seconds=TIMEOUT_SECONDS,
                 probe_resources=True, clock=time.monotonic_ns):
        self.store, self.recorder, self.name, self.ids = store, recorder, name, ids
        self.timeout_seconds, self.clock = _timeout(timeout_seconds), clock
        self.stderr_path = store.root / f'stderr-{name}.log'
        self._stderr = self.stderr_path.open('w')
        self.spawned_ns = clock()
        self.process = subprocess.Popen(
            [str(binary)], cwd=store.root, env=store.env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self._stderr)
        self.probe = ProcessProbe(self.process.pid) if probe_resources else NoProbe()
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self._buffer = b''
        self.exit_code, self.broken = None, False
        self.tool_calls, self.observations = 0, []

    def record_into(self, recorder):
        """Send the following frames and markers to `recorder`; return the previous one."""
        previous, self.recorder = self.recorder, recorder
        return previous

    @property
    def last_observation(self):
        return self.observations[-1] if self.observations else None

    def _send(self, message):
        raw = json.dumps(message, ensure_ascii=False)
        self.process.stdin.write(raw.encode('utf-8') + b'\n')
        self.process.stdin.flush()
        return raw

    def _read_line(self, deadline):
        """One response line, or None when the deadline passes first."""
        while b'\n' not in self._buffer:
            left = deadline - self.clock()
            if left <= 0 or not self.selector.select(left / 1e9):
                return None
            chunk = os.read(self.process.stdout.fileno(), READ_CHUNK)
            if not chunk:
                return ''
            self._buffer += chunk
        line, _, self._buffer = self._buffer.partition(b'\n')
        return line.decode('utf-8').rstrip('\r')

    def _observe(self, identifier, method, tool, started, ended, where, raw_response, reason, usage):
        index = None
        if method == 'tools/call':
            index, self.tool_calls = self.tool_calls, self.tool_calls + 1
        body = raw_response.encode('utf-8') if raw_response is not None else None
        observation = CallObservation(
            self.name, identifier, method, tool, index, started - self.spawned_ns, ended - started,
            where, hashlib.sha256(body).hexdigest() if body is not None else None,
            len(body) if body is not None else None, reason is not None, reason, usage)
        self.observations.append(observation)
        self.recorder.marker(observation.timing_marker())
        self.recorder.marker(observation.resources_marker())
        return observation

    def rpc(self, method, params):
        if self.broken:
            raise TransportError(f'{self.name}: session unusable after a timeout')
        identifier = next(self.ids)
        tool = params.get('name') if method == 'tools/call' else None
        snapshot = self.probe.before()
        started = self.clock()
        raw_request = self._send({'jsonrpc': '2.0', 'id': identifier, 'method': method, 'params': params})
        raw_response = self._read_line(started + int(self.timeout_seconds * 1e9))
        ended = self.clock()
        if raw_response is None:
            self.broken = True
            observation = self._observe(identifier, method, tool, started, ended, None, None,
                                        'timeout', self.probe.after(snapshot))
            raise TransportTimeout(f'{self.name}: no answer to {method} within '
                                   f'{self.timeout_seconds:g}s', observation)
        if not raw_response:
            raise TransportError(f'{self.name}: stdout closed during {method}')
        usage = self.probe.after(snapshot)
        where = self.recorder.pair(self.name, raw_request, raw_response)
        self._observe(identifier, method, tool, started, ended, where, raw_response, None, usage)
        response = json.loads(raw_response)
        if response.get('id') != identifier:
            raise TransportError(f'{self.name}: response id {response.get("id")!r} for {identifier}')
        return response

    def notify(self, method, params):
        self.recorder.notification(self.name, self._send({'jsonrpc': '2.0', 'method': method,
                                                           'params': params}))

    def handshake(self, client_name):
        """initialize -> effective store confirmed -> initialized -> tools/list."""
        init = self.rpc('initialize', {'protocolVersion': PROTOCOL_VERSION, 'capabilities': {},
                                       'clientInfo': {'name': client_name, 'version': '1'}})
        answered = self.last_observation
        startup_ns = answered.started_ns + answered.wall_ns  # spawn to initialize answered
        effective = confirm_effective_store(self.store, self.stderr_path.read_text(errors='replace'))
        self.notify('notifications/initialized', {})
        tools = self.rpc('tools/list', {})
        return {'negotiated': init.get('result', {}).get('protocolVersion'),
                'server': init.get('result', {}).get('serverInfo'),
                'tools': [tool.get('name') for tool in tools.get('result', {}).get('tools', [])],
                'effective_store': effective, 'startup_ns': startup_ns}

    def call(self, tool, arguments):
        return self.rpc('tools/call', {'name': tool, 'arguments': arguments})

    def close(self):
        if self.exit_code is not None:
            return self.exit_code
        if self.broken:
            self.process.kill()
        try:
            self.process.stdin.close()
        except OSError:
            pass
        try:
            self.exit_code = self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.exit_code = self.process.wait(timeout=10)
        self.selector.close()
        self.process.stdout.close()
        self._stderr.close()
        return self.exit_code
