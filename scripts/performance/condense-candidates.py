#!/usr/bin/env python3
"""Compare complete native card journeys. No model calls; bytes are not tokens."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ABOUT = 'project:card-replay'
ROOT, B, C = [f'{ABOUT}:{name}' for name in ('root', 'b', 'c')]
SOURCE = f'evidence:{ABOUT}:shared'


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode()


class Native:
    def __init__(self, binary, directory):
        env = {k: v for k, v in os.environ.items() if not k.startswith('KMP_')}
        env.update(KMP_MCP_BACKEND='embedded', KMP_MCP_DATA_DIR=str(directory / 'store'),
                   KMP_VIEWER_ADDR='127.0.0.1:0')
        self.process = subprocess.Popen([str(binary)], cwd=directory, env=env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.rows = []

    def rpc(self, method, params, phase):
        request = encoded(dict(jsonrpc='2.0', id=len(self.rows)+1, method=method, params=params))
        self.process.stdin.write(request+b'\n')
        self.process.stdin.flush()
        raw = self.process.stdout.readline().rstrip(b'\r\n')
        if not raw:
            raise RuntimeError(f'No response from native binary during {phase}')
        response = json.loads(raw)
        result = response.get('result', {})
        if 'error' in response or result.get('isError'):
            raise RuntimeError(response)
        field = result.get('structuredContent', {}).get('proof', {}).get('condense_candidates')
        self.rows.append(dict(phase=phase, request_bytes=len(request), response_bytes=len(raw),
                              candidate_field_bytes=len(encoded(field)) if field is not None else 0))
        return result

    def call(self, name, arguments, phase):
        return self.rpc('tools/call', dict(name=name, arguments=arguments), phase)['structuredContent']

    def pages(self, arguments, phase):
        results = []
        while True:
            result = self.call('kmp_trace', arguments, phase)
            results.append(result)
            if not result.get('page', {}).get('has_more'):
                return results
            assert len(results) < 100
            arguments = result['next_actions'][0]['arguments']

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=10)


def seed(size):
    entries = [dict(id=ref, kind='observation', text='entry '+('e'*size),
        coordinates=[dict(dimension='task', scope_id='read', occurred_at='2026-07-22T10:00:00Z')])
        for ref in [ROOT, B, C]]
    relations = [dict(**{'from': ROOT, 'to': ref}, rel='depends_on',
        **{'class': 'causal'}, confidence='high', why='The source declares the dependency.',
        evidence='Source register.') for ref in [B, C]]
    return dict(about=ABOUT, idempotency_key='replay', memory=dict(
        dimensions=[dict(id='read', kind='task')], entries=entries, relations=relations,
        evidence=[dict(id=SOURCE, supports=[ROOT,B,C], text='shared '+('s'*(size*4)), source='signed register')]))


def run(binary, scratch, size, mode):
    with tempfile.TemporaryDirectory(prefix='card-replay-', dir=scratch) as directory:
        client = Native(binary, Path(directory))
        try:
            client.rpc('initialize', dict(protocolVersion='2024-11-05', capabilities={},
                clientInfo=dict(name='condense-candidates-replay',version='1')), 'initialize')
            client.rpc('tools/list', {}, 'tools_list')
            client.call('kmp_ingest', seed(size), 'seed')
            query = dict(about=ABOUT, **{'from':ROOT, 'to':[B,C]},
                search=dict(proof=True, compact=dict(language='en')),
                page=dict(entries=2), budget=dict(max_bytes=200000))
            discovery = client.pages(query, 'discovery')
            objects = [o for page in discovery for o in page.get('objects', [])]
            if mode == 'candidate':
                choices = discovery[0]['proof']['condense_candidates']['items']
                if choices:
                    target = choices[0]
                    assert target['ref'] == SOURCE
                    source, expect = target['source'], target['expect']
                    record_bytes = target['record_bytes']
                else:
                    # The small-body negative control deliberately exercises an
                    # unoffered target; this does not attribute benefit to the list.
                    target = next(o for o in objects if o['ref'] == SOURCE)
                    source = {k:target['descriptor'][k] for k in ['revision','record_digest']}
                    expect, record_bytes = dict(absent=True), target['descriptor']['record_bytes']
            else:
                # A scripted full scan supplies the same target, without claiming
                # to model a reader's target choice or identity-copy error rate.
                target = next(o for o in objects if o['ref'] == SOURCE)
                source = {k:target['descriptor'][k] for k in ['revision','record_digest']}
                expect, record_bytes = dict(absent=True), target['descriptor']['record_bytes']
            expand = json.loads(json.dumps(query))
            expand['search'].update(proof_refs=[SOURCE],
                expect_selection=discovery[0]['proof']['manifest_id'], max_body_record_bytes=record_bytes)
            expansion = client.pages(expand, 'expansion')
            body = next(o['text'] for p in expansion for o in p.get('objects', []) if o['ref']==SOURCE)
            assert body == seed(size)['memory']['evidence'][0]['text']
            client.call('kmp_condense', dict(about=ABOUT, ref=SOURCE, scope='node_body', language='en',
                card='The signed register supports the three entries.', source=source, expect=expect,
                actor='replay-reader'), 'condense')
            fresh = client.pages(query, 'fresh_compact')
            assert fresh[0]['proof']['compact']['valid']==1
            journey = [r for r in client.rows if r['phase'] not in ['initialize','tools_list','seed']]
            return dict(mode=mode, entry_filler_bytes=size, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                calls=len(journey), request_bytes=sum(r['request_bytes'] for r in journey),
                response_bytes=sum(r['response_bytes'] for r in journey),
                total_bytes=sum(r['request_bytes']+r['response_bytes'] for r in journey),
                candidate_field_bytes=sum(r['candidate_field_bytes'] for r in journey),
                source_sha256=hashlib.sha256(body.encode()).hexdigest(), phases=client.rows)
        finally:
            client.close()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline',type=Path,required=True)
    parser.add_argument('--candidate',type=Path,required=True)
    parser.add_argument('--scratch',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    args.scratch.mkdir(parents=True,exist_ok=True)
    results=[]
    for size in [4096, 120]:
        for mode in ['baseline','candidate']:
            results.append(run(getattr(args,mode).resolve(),args.scratch.resolve(),size,mode))
    for baseline,candidate in zip(results[::2],results[1::2]):
        assert baseline['source_sha256']==candidate['source_sha256']
    args.output.write_text(json.dumps(dict(contract='condense-candidate-replay-v1',
        scope='Complete JSON-RPC requests/responses, without line delimiters. No model, token or latency claim.',
        results=results),indent=2)+'\n')
    print(json.dumps([{k:v for k,v in r.items() if k!='phases'} for r in results],indent=2))


if __name__=='__main__':
    main()
