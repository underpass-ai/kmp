#!/usr/bin/env python3
"""Replay JSON call blocks from the shipped lesson against a fresh real store."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from urllib.parse import urlsplit, urlunsplit

from stdio import Stdio
from decision_history_checks import check as check_history
from alias_ownership_checks import check as check_alias
from distributed_incident_checks import check as check_incident
from four_clocks_checks import check as check_clocks, clock_bindings
from quantities_checks import check as check_quantities, clock_bindings as quantity_clock
from late_conflict_checks import check as check_late_conflict
from labels_negation_checks import check as check_labels_negation
from budget_proof_checks import check as check_budget_proof
from guide_reads import prepare
from dimensional_memberships_checks import check as check_dimensions
from semantic_batch_checks import check as check_batch
from capability_checks import check as check_capability

ROOT = Path(__file__).resolve().parents[2]
LESSONS = {'semantic-batch': check_batch, 'dimensional-memberships': check_dimensions, 'decision-history': check_history, 'alias-ownership': check_alias,
           'distributed-incident': check_incident, 'four-clocks': check_clocks,
           'quantities': check_quantities, 'late-conflict': check_late_conflict,
           'labels-negation': check_labels_negation, 'budget-proof': check_budget_proof,
           'first-decision': check_capability, 'preference-delta': check_capability, 'workflow-proof': check_capability,
           'structure-parts': check_capability, 'canonical-ingest': check_capability}


def bind(value, saved):
    if isinstance(value, dict):
        return {key: bind(item, saved) for key, item in value.items()}
    if isinstance(value, list):
        return [bind(item, saved) for item in value]
    if isinstance(value, str) and value.startswith('${') and value.endswith('}'):
        parts = value[2:-1].split('.')
        result = saved[parts.pop(0)]
        for part in parts:
            result = result[int(part)] if isinstance(result, list) else result[part]
        return result
    return value


def redact(value):
    if isinstance(value, dict):
        return {key: redact(item) for key, item in value.items()}
    if isinstance(value, list):
        return [redact(item) for item in value]
    if isinstance(value, str):
        def public_url(match):
            url = urlsplit(match.group())
            return urlunsplit((url.scheme, url.netloc, url.path, '', ''))
        return re.sub(r'http://(?:127\.0\.0\.1|localhost):\d+(?:/[^\s"\\]*)?', public_url, value)
    return value


def complete(result):
    for page in (result.get('page', {}), result.get('projection', {}).get('page', {})):
        if page.get('has_more'):
            raise ValueError('Lesson replay is incomplete; follow its continuation before asserting coverage')


def run(args):
    binary = args.binary.resolve()
    lesson = ROOT / 'plugins/kmp/guide/examples' / (args.lesson + '.md')
    args.trace.parent.mkdir(parents=True, exist_ok=True)
    (ROOT / 'tmp').mkdir(exist_ok=True)
    saved, authored = {}, {}
    if args.lesson == 'four-clocks':
        saved['clock'] = clock_bindings()
    with tempfile.TemporaryDirectory(prefix='guide-example-', dir=ROOT / 'tmp') as directory:
        store = Path(directory)
        env = {k: v for k, v in os.environ.items() if not k.startswith(('KMP_', 'KERNEL_'))}
        env.update(KMP_MCP_BACKEND='embedded', KMP_MCP_DATA_DIR=str(store),
                   KMP_VIEWER_ADDR='127.0.0.1:0' if args.hold_view else 'off',
                   XDG_DATA_HOME=str(store / 'xdg'))
        sync = subprocess.run([str(binary), 'guide', 'sync', '--plugin-root', str(ROOT / 'plugins/kmp')],
                              env={**env, 'KMP_VIEWER_ADDR': 'off'}, cwd=store,
                              text=True, capture_output=True, check=True)
        with args.trace.open('w') as trace:
            def record(event):
                trace.write(json.dumps(redact(event), ensure_ascii=False) + '\n')
                trace.flush()

            record({'preparation': 'guide sync in isolated store', 'output': sync.stdout,
                    'lesson_sha256': hashlib.sha256(lesson.read_bytes()).hexdigest(),
                    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                    'model_calls': 0})
            if 'clock' in saved:
                record({'preparation': 'UTC clock bindings for the lesson', 'clock': saved['clock']})
            client = Stdio(binary, store, env, record)
            try:
                client.rpc('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {},
                                         'clientInfo': {'name': 'guide-example-replay', 'version': '1'}})
                client.rpc('tools/list', {})
                guide_reads = prepare(client, ROOT, lesson, args.guide_mode)
                calls = [json.loads(block) for block in re.findall(r'```json\n(.*?)\n```', lesson.read_text(), re.S)]
                for call in calls:
                    if args.lesson == 'quantities' and call['save_as'] == 'total':
                        saved['clock'] = quantity_clock()
                        record({'preparation': 'Read real UTC clock after inspecting settlement sources',
                                'clock': saved['clock']})
                    arguments = bind(call['arguments'], saved)
                    result = client.call(call['tool'], arguments, call.get('expect_error'))
                    if call.get('expect_partial'):
                        pages = (result.get('page', {}), result.get('projection', {}).get('page', {}), result.get('selection', {}))
                        if not any(page.get('has_more') for page in pages):
                            raise ValueError('Expected an explicitly partial teaching response')
                    else:
                        complete(result)
                    saved[call['save_as']] = result
                    authored[call['save_as']] = arguments
                checks = LESSONS[args.lesson](saved, client, authored)
                summary = {'lesson': str(lesson.relative_to(ROOT)), 'calls_in_lesson': len(calls), 'guide_reads': guide_reads,
                           'checks': checks, 'model_calls': 0, 'results': redact(saved)}
                args.result.write_text(json.dumps(summary, indent=2, ensure_ascii=False) + '\n')
                print(json.dumps({'checks': checks, 'calls_in_lesson': len(calls), 'model_calls': 0}), flush=True)
                if args.hold_view:
                    print('ChronoLoom: ' + saved['view_state']['url'], flush=True)
                    while True:
                        command = input('Enter a JSON MCP call, or quit: ').strip()
                        if command == 'quit':
                            break
                        if command:
                            call = json.loads(command)
                            print(json.dumps(client.call(call['tool'], bind(call['arguments'], saved)), ensure_ascii=False), flush=True)
            finally:
                client.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--trace', type=Path, required=True)
    parser.add_argument('--result', type=Path, required=True)
    parser.add_argument('--hold-view', action='store_true')
    parser.add_argument('--lesson', choices=LESSONS, default='decision-history')
    parser.add_argument('--guide-mode', choices=('markdown', 'directed', 'full'), default='markdown')
    run(parser.parse_args())
