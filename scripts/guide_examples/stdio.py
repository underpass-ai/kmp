"""Public MCP transport for isolated guide replays; never invokes a model."""
import json
import selectors
import subprocess


class Stdio:
    def __init__(self, binary, store, env, record):
        self.record, self.counter = record, 0
        self.stderr = (store / 'stderr.log').open('w')
        self.process = subprocess.Popen(
            [str(binary)], cwd=store, env=env, text=True, bufsize=1,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)

    def rpc(self, method, params, expect_error=None):
        self.counter += 1
        request = {'jsonrpc': '2.0', 'id': self.counter, 'method': method, 'params': params}
        self.process.stdin.write(json.dumps(request, ensure_ascii=False) + '\n')
        self.process.stdin.flush()
        if not self.selector.select(60):
            raise TimeoutError(f'MCP did not answer {method}')
        response = json.loads(self.process.stdout.readline())
        self.record({'request': request, 'response': response})
        if response.get('id') != self.counter or response.get('error'):
            raise ValueError(response)
        result = response['result']
        if expect_error is not None:
            if not result.get('isError') or result.get('structuredContent', {}).get('error', {}).get('code') != expect_error:
                raise ValueError(f'Expected {expect_error}, received {result}')
        elif result.get('isError'):
            raise ValueError(result)
        return result

    def call(self, tool, arguments, expect_error=None):
        return self.rpc('tools/call', {'name': tool, 'arguments': arguments}, expect_error)['structuredContent']

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=10)
        self.selector.close()
        self.process.stdout.close()
        self.stderr.close()
