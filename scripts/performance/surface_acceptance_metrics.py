#!/usr/bin/env python3
"""Count named serialized representations, never billed or hidden host context."""
import argparse
from collections import Counter
import gzip
import hashlib
import importlib.metadata
import json
from pathlib import Path

import tiktoken

ROOT = Path(__file__).resolve().parents[2]
ENCODER = tiktoken.get_encoding('o200k_base')


def measure(value):
    text = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False, separators=(',', ':'))
    return {'bytes': len(text.encode()), 'tokens': len(ENCODER.encode(text))}


def read(path):
    return json.loads(gzip.decompress(path.read_bytes()) if path.suffix == '.gz' else path.read_bytes())


def journey(folder):
    manifest = read(folder / 'manifest.json')
    rows = []
    for run in manifest['journeys']:
        events = [json.loads(line) for line in gzip.decompress(
            (folder / (run['lesson'] + '.jsonl.gz')).read_bytes()).splitlines()]
        groups = {}
        methods = Counter()
        for event in events:
            if 'request' not in event or 'response' not in event:
                continue
            request = event['request']
            method = request['method']
            params = request.get('params', {})
            name = params.get('name', method)
            arguments = params.get('arguments', {})
            if method != 'tools/call':
                group = 'startup_catalogue'
            elif name == 'kmp_guide' or arguments.get('about', '').startswith('guide:'):
                group = 'guide'
            else:
                group = 'memory'
            counts = groups.setdefault(group, Counter())
            counts['calls'] += 1
            methods[name] += 1
            for side in ('request', 'response'):
                for unit, amount in measure(event[side]).items():
                    counts[side + '_' + unit] += amount
            result = event['response'].get('result', {})
            structured = result.get('structuredContent', {})
            if any(page.get('has_more', False) for page in
                   (structured.get('page', {}), structured.get('projection', {}).get('page', {}))):
                counts['partial_responses'] += 1
            counts['error_responses'] += bool(event['response'].get('error') or result.get('isError'))
        rows.append({**run, 'groups': groups, 'methods': methods,
                     'all_rpc_request_response_tokens': sum(
                         g['request_tokens']+g['response_tokens'] for g in groups.values())})
    return {'binary_sha256': manifest['binary_sha256'], 'journeys': rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    root = args.evidence
    host = read(root / 'host-catalogue.json')
    native = read(root / 'native-main.json.gz')
    profiles = {}
    for name, events in native['profiles'].items():
        profiles[name] = [{'method': e['request']['method'],
                           'request': measure(e['request']), 'response': measure(e['response'])}
                          for e in events]
    files = [ROOT / 'plugins/kmp/guide/AGENT.md', ROOT / 'plugins/kmp/guide/editorial.json']
    files += sorted((ROOT / 'plugins/kmp/skills').glob('*/SKILL.md'))
    report = {'encoder': 'o200k_base', 'tiktoken_version': importlib.metadata.version('tiktoken'),
              'unit': 'compact UTF-8 JSON per RPC or exact file text; separately tokenized, not billing',
              'model_calls': 0, 'host': {'source': host['source'], 'metadata': measure(host),
                  'tools': len(host['tools']), 'opaque_argument_tools': [t['name'] for t in host['tools']
                      if 'args: { [key: string]: unknown; }' in t['description'] or 'args: unknown' in t['description']],
                  'candidate_host_refresh_observed': False},
              'native_main': profiles,
              'files': [{'path': str(p.relative_to(ROOT)), **measure(p.read_text()),
                         'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in files],
              'journeys': {side: journey(root / side) for side in
                           ('journeys-main', 'journeys-installed-0185')}}
    (root / 'surface-metrics.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'opaque_arguments': len(report['host']['opaque_argument_tools']),
                      'journeys': {k: len(v['journeys']) for k,v in report['journeys'].items()}}))


if __name__ == '__main__':
    main()
