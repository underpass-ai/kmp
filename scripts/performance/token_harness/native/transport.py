"""One MCP stdio process per session; frames are recorded before they are interpreted."""
import json
import selectors
import subprocess

from ..domain.errors import HarnessError
from .isolation import confirm_effective_store

PROTOCOL_VERSION = '2024-11-05'
TIMEOUT_SECONDS = 60


class TransportError(HarnessError):
    code = 'TRANSPORT_FAILED'


class StdioSession:
    def __init__(self, binary, store, recorder, name, ids):
        self.store, self.recorder, self.name, self.ids = store, recorder, name, ids
        self.stderr_path = store.root / f'stderr-{name}.log'
        self._stderr = self.stderr_path.open('w')
        self.process = subprocess.Popen(
            [str(binary)], cwd=store.root, env=store.env, text=True, encoding='utf-8', bufsize=1,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self._stderr)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.exit_code = None

    def _send(self, message):
        raw = json.dumps(message, ensure_ascii=False)
        self.process.stdin.write(raw + '\n')
        self.process.stdin.flush()
        return raw

    def rpc(self, method, params):
        identifier = next(self.ids)
        raw_request = self._send({'jsonrpc': '2.0', 'id': identifier, 'method': method, 'params': params})
        if not self.selector.select(TIMEOUT_SECONDS):
            raise TransportError(f'{self.name}: no answer to {method} within {TIMEOUT_SECONDS}s')
        raw_response = self.process.stdout.readline().rstrip('\n')
        if not raw_response:
            raise TransportError(f'{self.name}: stdout closed during {method}')
        self.recorder.pair(self.name, raw_request, raw_response)
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
        effective = confirm_effective_store(self.store, self.stderr_path.read_text(errors='replace'))
        self.notify('notifications/initialized', {})
        tools = self.rpc('tools/list', {})
        return {'negotiated': init.get('result', {}).get('protocolVersion'),
                'server': init.get('result', {}).get('serverInfo'),
                'tools': [tool.get('name') for tool in tools.get('result', {}).get('tools', [])],
                'effective_store': effective}

    def call(self, tool, arguments):
        return self.rpc('tools/call', {'name': tool, 'arguments': arguments})

    def close(self):
        if self.exit_code is not None:
            return self.exit_code
        self.process.stdin.close()
        try:
            self.exit_code = self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.exit_code = self.process.wait(timeout=10)
        self.selector.close()
        self.process.stdout.close()
        self._stderr.close()
        return self.exit_code
