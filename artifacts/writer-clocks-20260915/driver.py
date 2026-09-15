#!/usr/bin/env python3
"""Serve one fresh-writer MCP session and retain its complete native trace.

The driver never creates a memory packet or resumes a continuation. Send one
JSON line at a time on stdin as {"tool":"...","arguments":{...}}; every
response is emitted immediately on stdout. Send `quit` to finish.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import tempfile


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Client:
    def __init__(self, binary, store, env, trace):
        self.trace, self.counter = trace, 0
        self.stderr = (store / 'stderr.log').open('w')
        self.process = subprocess.Popen(
            [str(binary)], cwd=store, env=env, text=True, bufsize=1,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)

    def rpc(self, method, params):
        self.counter += 1
        request = {'jsonrpc': '2.0', 'id': self.counter, 'method': method, 'params': params}
        self.process.stdin.write(json.dumps(request, ensure_ascii=False) + '\n')
        self.process.stdin.flush()
        if not self.selector.select(60):
            raise TimeoutError(f'MCP did not answer {method}')
        response = json.loads(self.process.stdout.readline())
        self.trace.write(json.dumps({'request': request, 'response': response}, ensure_ascii=False) + '\n')
        self.trace.flush()
        return response

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


def emit(value):
    print(json.dumps(value, ensure_ascii=False), flush=True)


def run(args):
    binary = args.binary.resolve()
    root = Path(__file__).resolve().parents[2]
    source = Path(__file__).with_name('SOURCE.md')
    instructions = Path(__file__).with_name('WRITER-INSTRUCTIONS.md')
    args.trace.parent.mkdir(parents=True, exist_ok=True)
    scratch = root / 'tmp'
    scratch.mkdir(exist_ok=True)
    store = args.store or Path(tempfile.mkdtemp(prefix='writer-clocks-', dir=scratch))
    store.mkdir(parents=True, exist_ok=True)
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(('KMP_', 'KERNEL_'))}
    env.update(KMP_MCP_BACKEND='embedded', KMP_MCP_DATA_DIR=str(store),
               KMP_VIEWER_ADDR='off', XDG_DATA_HOME=str(store / 'xdg'))
    with args.trace.open('w') as trace:
        trace.write(json.dumps({
            'preparation': 'fresh persistent embedded store', 'store': str(store),
            'binary': str(binary), 'binary_sha256': digest(binary),
            'source_sha256': digest(source), 'instructions_sha256': digest(instructions),
            'driver_sha256': digest(Path(__file__)), 'model_calls_by_driver': 0,
        }) + '\n')
        trace.flush()
        plugin_root = (args.plugin_root or root / 'plugins/kmp').resolve()
        synced = subprocess.run(
            [str(binary), 'guide', 'sync', '--plugin-root', str(plugin_root)],
            cwd=store, env=env, text=True, capture_output=True, check=True)
        trace.write(json.dumps({'preparation': 'guide sync in the isolated store',
            'plugin_root': str(plugin_root), 'stdout': synced.stdout,
            'guide_sha256': digest(plugin_root / 'guide/memory.jsonl')}) + '\n')
        trace.flush()
        client = Client(binary, store, env, trace)
        try:
            for method, params in [
                ('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {},
                                'clientInfo': {'name': 'writer-clocks-driver', 'version': '1'}}),
                ('tools/list', {}),
            ]:
                emit(client.rpc(method, params))
            for line in sys.stdin:
                line = line.strip()
                if line == 'quit':
                    trace.write(json.dumps({'session': 'quit'}) + '\n')
                    trace.flush()
                    return
                try:
                    declared = json.loads(line)
                    response = client.rpc(
                        'tools/call',
                        {'name': declared['tool'], 'arguments': declared['arguments']},
                    )
                except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
                    response = {'driver_error': str(error)}
                    trace.write(json.dumps({'input': line, 'driver_error': str(error)}) + '\n')
                    trace.flush()
                emit(response)
        finally:
            client.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--trace', type=Path, required=True)
    parser.add_argument('--store', type=Path)
    parser.add_argument('--plugin-root', type=Path)
    run(parser.parse_args())
