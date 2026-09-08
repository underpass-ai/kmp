#!/usr/bin/env python3
"""Replay one handoff across fresh MCP processes and a real shared-view boundary."""
import argparse
import hashlib
import http.cookiejar
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from urllib.parse import urlencode, urlsplit
from urllib.request import build_opener, HTTPCookieProcessor, Request

from guide_reads import prepare
from replay import bind, complete, redact
from shared_resumption_checks import check
from stdio import Stdio

ROOT = Path(__file__).resolve().parents[2]


def human_fixture(saved, record):
    """Simulate the browser report through its HTTP adapter; never write memory."""
    url = saved['before_human']['url']
    parsed = urlsplit(url)
    if parsed.hostname != '127.0.0.1':
        raise ValueError('The teaching gesture requires its own loopback viewer')
    opener = build_opener(HTTPCookieProcessor(http.cookiejar.CookieJar()))
    with opener.open(url, timeout=10) as response:
        response.read()
    params = {'id': saved['before_human']['view_id'],
              'about': saved['before_human']['state']['about'],
              'clock': 'observed', 'from': '2026-09-01T08:30:00Z', 'to': '2026-09-01T11:00:00Z',
              'zoom': 'moment', 'labels': 'review in HND-9', 'layer_abouts': '[]',
              'selection': saved['recovered_constraint']['object']['ref']}
    endpoint = f'{parsed.scheme}://{parsed.netloc}/api/view/report'
    with opener.open(Request(endpoint + '?' + urlencode(params), method='POST'), timeout=10) as response:
        result = json.load(response)
    record({'fixture': 'simulated browser report; not an independent human review',
            'endpoint': endpoint, 'parameters': params, 'result': result})


def run(args):
    binary = args.binary.resolve()
    lesson = ROOT / 'plugins/kmp/guide/examples/shared-resumption.md'
    calls = [json.loads(s) for s in re.findall(r'```json\n(.*?)\n```', lesson.read_text(), re.S)]
    args.trace.parent.mkdir(parents=True, exist_ok=True)
    (ROOT / 'tmp').mkdir(exist_ok=True)
    saved, authored, guides, sessions = {}, {}, {}, []
    session, client, completed = 'writer', None, False
    with tempfile.TemporaryDirectory(prefix='guide-shared-', dir=ROOT / 'tmp') as directory:
        store = Path(directory)
        env = {k: v for k, v in os.environ.items() if not k.startswith(('KMP_', 'KERNEL_'))}
        env.update(KMP_MCP_BACKEND='embedded', KMP_MCP_DATA_DIR=str(store),
                   KMP_VIEWER_ADDR='off', XDG_DATA_HOME=str(store / 'xdg'))
        sync = subprocess.run([str(binary), 'guide', 'sync', '--plugin-root', str(ROOT / 'plugins/kmp')],
                              cwd=store, env=env, text=True, capture_output=True, check=True)
        with args.trace.open('w') as output:
            def record(event):
                output.write(json.dumps(redact({'session': session, **event}), ensure_ascii=False) + '\n')
                output.flush()

            def start():
                result = Stdio(binary, store, {**env, 'KMP_VIEWER_ADDR': '127.0.0.1:0' if session == 'reader' else 'off'}, record)
                sessions.append({'name': session, 'pid': result.process.pid})
                record({'session_start': session, 'pid': result.process.pid})
                result.rpc('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {},
                                         'clientInfo': {'name': 'guide-shared-' + session, 'version': '1'}})
                result.rpc('tools/list', {})
                guides[session] = prepare(result, ROOT, lesson, 'markdown')
                return result

            record({'preparation': 'guide sync in isolated persistent store', 'output': sync.stdout,
                    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                    'lesson_sha256': hashlib.sha256(lesson.read_bytes()).hexdigest(), 'model_calls': 0})
            try:
                client = start()
                for call in calls:
                    if call.get('new_session'):
                        if call['new_session'] != 'reader' or session != 'writer':
                            raise ValueError('This lesson has exactly one writer-to-reader handoff')
                        client.close()
                        sessions[-1]['exit_code'] = client.process.returncode
                        record({'session_end': session, 'exit_code': client.process.returncode})
                        client = None
                        session = 'reader'
                        client = start()
                    if call.get('human_checkpoint'):
                        if args.interactive:
                            print('ChronoLoom: ' + saved['before_human']['url'], flush=True)
                            input('Choose Observed and select C1 in the browser, then press Enter: ')
                            record({'fixture': 'actual UI checkpoint; browser evidence recorded separately'})
                        else:
                            human_fixture(saved, record)
                    arguments = bind(call['arguments'], saved)
                    result = client.call(call['tool'], arguments, call.get('expect_error'))
                    complete(result)
                    saved[call['save_as']], authored[call['save_as']] = result, arguments
                checks = check(saved, authored, sessions)
                summary = {'calls_in_lesson': len(calls), 'guide_reads': guides, 'sessions': sessions,
                           'interactive': args.interactive, 'checks': checks, 'model_calls': 0,
                           'results': redact(saved)}
                args.result.write_text(json.dumps(summary, indent=2, ensure_ascii=False) + '\n')
                completed = True
                print(json.dumps({'calls': len(calls), 'checks': checks, 'model_calls': 0}), flush=True)
                if args.interactive:
                    while True:
                        command = input('Review the final view. Enter a JSON MCP call or quit: ').strip()
                        if command == 'quit':
                            break
                        if command:
                            call = json.loads(command)
                            print(json.dumps(client.call(call['tool'], bind(call['arguments'], saved)), ensure_ascii=False), flush=True)
            finally:
                if client is not None:
                    client.close()
                    sessions[-1]['exit_code'] = client.process.returncode
                    record({'session_end': session, 'exit_code': client.process.returncode})
                    if completed:
                        summary['sessions'] = sessions
                        args.result.write_text(json.dumps(summary, indent=2, ensure_ascii=False) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--trace', type=Path, required=True)
    parser.add_argument('--result', type=Path, required=True)
    parser.add_argument('--interactive', action='store_true')
    run(parser.parse_args())
