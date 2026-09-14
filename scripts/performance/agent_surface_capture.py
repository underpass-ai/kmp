#!/usr/bin/env python3
"""Capture native startup/catalogue/App representations in isolated stores."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/guide_examples'))
from stdio import Stdio


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    result = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'profiles': {}, 'model_calls': 0}
    (ROOT / 'tmp').mkdir(exist_ok=True)
    for apps in (False, True):
        with tempfile.TemporaryDirectory(prefix='surface-capture-', dir=ROOT / 'tmp') as folder:
            store = Path(folder)
            env = {key: value for key, value in os.environ.items()
                   if not key.startswith(('KMP_', 'KERNEL_'))}
            env.update(KMP_MCP_BACKEND='embedded', KMP_MCP_DATA_DIR=str(store),
                       KMP_VIEWER_ADDR='off', XDG_DATA_HOME=str(store / 'xdg'))
            events = []
            client = Stdio(binary, store, env, events.append)
            try:
                capabilities = {'extensions': {'io.modelcontextprotocol/ui': {
                    'mimeTypes': ['text/html;profile=mcp-app']}}} if apps else {}
                client.rpc('initialize', {'protocolVersion': '2024-11-05',
                    'capabilities': capabilities,
                    'clientInfo': {'name': 'agent-surface-capture', 'version': '1'}})
                client.rpc('tools/list', {})
                if apps:
                    catalogue = client.rpc('resources/list', {})
                    for resource in catalogue['resources']:
                        client.rpc('resources/read', {'uri': resource['uri']})
            finally:
                client.close()
            result['profiles']['apps' if apps else 'model'] = events
    if hashlib.sha256(binary.read_bytes()).hexdigest() != result['binary_sha256']:
        raise RuntimeError('The captured binary changed during the run')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    raw = json.dumps(result, ensure_ascii=False, separators=(',', ':')).encode()
    args.output.write_bytes(gzip.compress(raw, mtime=0))
    print(json.dumps({'binary_sha256': result['binary_sha256'], 'raw_bytes': len(raw),
                      'gzip_bytes': args.output.stat().st_size, 'model_calls': 0}))


if __name__ == '__main__':
    main()
