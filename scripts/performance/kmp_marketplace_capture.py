#!/usr/bin/env python3
"""Read KMP catalogue records through a fresh Codex app-server; no install or reload."""
import argparse
import json
from pathlib import Path
import select
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error('Preserve captures; choose a new output path')
    process = subprocess.Popen(['codex', 'app-server', '--stdio'], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                               text=True, bufsize=1)

    def rpc(request_id, method, params):
        process.stdin.write(json.dumps({'id': request_id, 'method': method, 'params': params}) + '\n')
        process.stdin.flush()
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            if not select.select([process.stdout], [], [], max(0, deadline-time.monotonic()))[0]:
                break
            line = process.stdout.readline()
            if not line:
                raise RuntimeError('App server exited before response')
            response = json.loads(line)
            if response.get('id') == request_id:
                return response
        raise TimeoutError(method)

    try:
        rpc(1, 'initialize', {'clientInfo': {'name': 'kmp-catalogue-audit', 'version': '1'},
                             'capabilities': {'experimentalApi': True, 'requestAttestation': False}})
        process.stdin.write(json.dumps({'method': 'initialized'}) + '\n')
        process.stdin.flush()
        response = rpc(2, 'plugin/list', {'marketplaceKinds': ['local'], 'forceRefetch': False})
        if 'error' in response:
            raise RuntimeError(response['error'])
        selected = []
        for marketplace in response['result']['marketplaces']:
            plugins = [p for p in marketplace['plugins'] if p['name'] == 'kmp']
            if plugins:
                selected.append({**marketplace, 'plugins': plugins})
        capture = {'source': 'fresh local Codex app-server, existing configuration; filtered to KMP',
                   'codex_version': subprocess.check_output(['codex', '--version'], text=True).strip(),
                   'model_calls': 0, 'mutations': 0, 'marketplaces': selected}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(capture, indent=2) + '\n')
        print(json.dumps({'marketplaces': len(selected), 'output': str(args.output)}))
    finally:
        process.terminate()
        process.wait(timeout=10)


if __name__ == '__main__':
    main()
