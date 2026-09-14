#!/usr/bin/env python3
"""Freeze native four-arm contexts and measure whole CLI bytes/tokens.

No model is invoked. Claims are fixed writer declarations, not generated or
graded predictions. Independent readers receive only contexts and questions.
Run: uv run --with tiktoken==0.14.0 scripts/consolidation/evaluate.py --binary ... --out ...
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import tiktoken

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/guide_examples'))
from stdio import Stdio


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--archive-repeats', type=int, default=8,
                        help='Archival filler lines per source (0..32); use 0 for a short-source cost control.')
    args = parser.parse_args()
    if not 0 <= args.archive_repeats <= 32:
        parser.error('--archive-repeats must be 0..32')
    binary = args.binary.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    tokenizer = tiktoken.get_encoding('o200k_base')
    rows, budget_failures = [], []
    about = 'project:consolidation-control'
    specifications = [
        ('person:elena-17', 'owns', 'account:amber', 'September 2026', 'affirmed', 'reported', ['staging only'], 'In September 2026 Elena (person 17) owns Amber for staging only.'),
        ('person:elena-29', 'owns', 'account:blue', 'September 2026', 'affirmed', 'reported', ['production only'], 'A different Elena (person 29) owns Blue for production only in September 2026.'),
        ('person:elena-17', 'owns', 'account:amber', 'October 2026', 'negated', 'reported', [], 'From October 2026 Elena (person 17) no longer owns Amber.'),
        ('person:nora-3', 'owns', 'account:amber', 'October 2026', 'affirmed', 'reported', ['staging'], 'Nora (person 3) takes over Amber for staging in October 2026.'),
        ('release:r7', 'permission', 'publication', 'release:r7', 'negated', 'established', [], 'R7 has no permission for publication.'),
        ('release:r7', 'verification', 'copy checksum', 'release:r7', 'affirmed', 'established', ['copy only'], 'The R7 copy checksum was verified; this check concerns the copy only.'),
        ('release:r8', 'permission', 'publication', 'release:r8', 'affirmed', 'tentative', ['pending signature'], 'Publication permission for R8 is tentative and awaits signature.'),
        ('release:r8', 'permission', 'publication', 'release:r8', 'negated', 'reported', ['unresolved report'], 'Another report denies publication permission for R8; the conflict is unresolved.'),
    ]
    assertions, entries = [], []
    for index, spec in enumerate(specifications):
        referent, predicate, value, scope, polarity, epistemic, qualifiers, text = spec
        claim = dict(referent=referent, predicate=predicate, value=value, temporal_scope=scope,
                     polarity=polarity, epistemic_status=epistemic, qualifiers=qualifiers)
        for report in range(2):
            ref = f'{about}:source:{index}:{report}'
            # Repeated archival context deliberately stresses redundant reports;
            # report repetition is not labelled independent corroboration.
            body = text + '\n' + ('Archive note: retain this report with its scope and original reference.\n' * args.archive_repeats)
            entries.append({'id': ref, 'kind': 'observation', 'text': body,
                'coordinates': [{'dimension': 'task', 'scope_id': 'control',
                    'occurred_at': '2026-09-01T00:00:00Z', 'observed_at': '2026-09-02T00:00:00Z'}]})
            assertions.append({'source_ref': ref, 'quote': text, 'claim': claim,
                'why': 'Fixed source-only fixture interpretation; preserve the explicit identity, period, polarity and qualifications.'})
    questions = [
        'Who owns Amber for staging in September 2026? Identify the person unambiguously.',
        'What account does the other Elena own, and in what environment?',
        'Who owns Amber in October 2026, and does Elena person 17 still own it?',
        'Does checksum verification authorize publication of R7?',
        'Is R8 publication permission settled? State the qualifications and conflicting evidence.',
        'Do two repeated reports prove independent corroboration?',
        'Does anyone have production permission for Amber?',
        'What is the exact signature date for R8?',
    ]
    (args.out/'questions.json').write_bytes(encoded(questions))
    (args.out/'writer-input.json').write_bytes(encoded({'entries': entries, 'assertions': assertions}))
    scratch = ROOT/'tmp'; scratch.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='consolidation-control-', dir=scratch) as path:
        store = Path(path)
        env = {**os.environ, 'KMP_MCP_DATA_DIR': str(store), 'KMP_MCP_BACKEND': 'embedded', 'KMP_VIEWER_ADDR': 'off'}
        records = []
        mcp = Stdio(binary, store, env, records.append)
        try:
            mcp.rpc('initialize', {'protocolVersion':'2024-11-05','capabilities':{},'clientInfo':{'name':'consolidation-control','version':'1'}})
            seeded = mcp.call('kmp_ingest', {'about': about, 'idempotency_key':'consolidation-control-originals', 'memory': {
                'dimensions':[{'id':'control','kind':'task'}], 'entries':entries,'relations':[],'evidence':[]}})
            if seeded.get('memory', {}).get('accepted', {}).get('entries') != len(entries):
                raise RuntimeError(f'seeding did not accept: {seeded}')
        finally:
            mcp.close()

        def cli(verb, value, group='consolidation', expect_rejection=False):
            start = time.perf_counter_ns()
            response = subprocess.run([str(binary), group, verb], input=encoded(value), env=env,
                capture_output=True, timeout=60)
            if expect_rejection:
                assert response.returncode != 0, 'invalid proposal was accepted'
                result = {'exit_code':response.returncode, 'stderr':response.stderr.decode()}
                (args.out/'rejected-proposal.json').write_bytes(encoded({'request':value, 'result':result}))
            else:
                response.check_returncode()
                result = json.loads(response.stdout)
            rows.append({'verb':f'{group} {verb}', 'request_bytes':len(encoded(value)),
                'response_bytes':len(response.stdout), 'request_tokens':len(tokenizer.encode(encoded(value).decode())),
                'response_tokens':len(tokenizer.encode(response.stdout.decode())),
                'stderr_bytes':len(response.stderr), 'stderr_tokens':len(tokenizer.encode(response.stderr.decode())),
                'exit_code':response.returncode,
                'wall_ms_including_process_start':(time.perf_counter_ns()-start)/1e6})
            return result

        sources = cli('sources', {'about':about,'refs':[e['id'] for e in entries]})
        command = {'about':about,'view':'control','author':'fixed-fixture-writer','expect_revision':0,
            'idempotency_key':'control-first','sources':{s['reference']:s['stamp'] for s in sources},'assertions':assertions}
        accepted = cli('write', command)
        assert accepted['claim_count'] == len(specifications)
        assert accepted['source_count'] == len(entries)
        replay = cli('write', command)
        assert replay == accepted
        rejected = {**command, 'idempotency_key':'control-invalid-quote', 'expect_revision':1,
            'assertions':[{**assertions[0], 'quote':'This passage is absent from the source.'}]}
        cli('write', rejected, expect_rejection=True)
        source_only = [{'ref':e['id'],'text':e['text']} for e in entries]
        formed = source_only + [{'ref':f'formed:{i}', 'text':a['quote'],'source_ref':a['source_ref']} for i,a in enumerate(assertions)]
        # Native lossless presentation arm, with executable original-source reads.
        groups = [{'id':str(i),'packets':[{'object':packet}], 'reads':[{'tool':'kmp_inspect','arguments':{'about':about,'ref':packet.get('source_ref',packet['ref'])}}]} for i,packet in enumerate(formed)]
        baseline_budget = len(encoded({'records':formed, 'omitted_records':0}))
        for fraction in (1.0, .75, .5):
            limit = int(baseline_budget*fraction)
            projected = cli('project', {'about':about,'view':'control','max_bytes':limit})
            dedup = cli('project', {'groups':groups,'max_bytes':limit}, 'context')
            for arm, candidates in [('source-only',source_only), ('formed',formed)]:
                selected=[]
                for candidate in candidates:
                    proposal = {'records':selected+[candidate], 'omitted_records':len(candidates)-len(selected)-1}
                    if len(encoded(proposal)) <= limit: selected.append(candidate)
                packet={'records':selected,'omitted_records':len(candidates)-len(selected)}
                assert len(encoded(packet)) <= limit
                (args.out/f'{arm}-{int(fraction*100)}.json').write_bytes(encoded(packet))
            if len(encoded(dedup)) > limit:
                # Existing lossless context projection keeps its omission/read
                # manifest even when that metadata cannot fit. Preserve the
                # overshoot as a failed budget delivery, not a compressed win.
                budget_failures.append({'arm':'deduplicated', 'budget_percent':int(fraction*100),
                    'requested_bytes':limit, 'actual_bytes':len(encoded(dedup))})
            assert len(encoded(projected)) <= limit
            (args.out/f'deduplicated-{int(fraction*100)}.json').write_bytes(encoded(dedup))
            (args.out/f'consolidated-{int(fraction*100)}.json').write_bytes(encoded(projected))
        audit = cli('read', accepted['expansion'])
        assert audit['view']['revision'] == accepted['revision']
        assert len(audit['view']['claims']) == accepted['claim_count']
        assert len(audit['view']['sources']) == accepted['source_count']
        (args.out/'audit.json').write_bytes(encoded(audit))
    packets = []
    for path in sorted(args.out.glob('*-*.json')):
        if not any(path.name.startswith(arm+'-') for arm in ('source-only','formed','deduplicated','consolidated')): continue
        data = path.read_bytes()
        packets.append({'packet':path.name,'bytes':len(data),'tokens':len(tokenizer.encode(data.decode())),
            'sha256':hashlib.sha256(data).hexdigest()})
    report={'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),
        'script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'archive_repeats':args.archive_repeats,
        'tokenizer':f'tiktoken {tiktoken.__version__} o200k_base', 'original_sources':len(entries),
        'fixed_declarations':len(assertions),'consolidated_groups':len(specifications),
        'budgets':'100%, 75%, 50% of full formed JSON packet byte size including envelope; same ceiling for all arms',
        'budget_bytes':{str(int(f*100)):int(baseline_budget*f) for f in (1.0,.75,.5)},
        'budget_delivery_failures':budget_failures,
        'cli_calls':rows,'packets':packets,'reader_results':'not run by this script',
        'limitations':['Development fixture with manually fixed writer declarations, not held-out evaluation.',
            'Byte savings and token counts are representation measurements, not reader quality or billed cost.',
            'CLI latency includes process startup; this is not a warmed embedded-service latency benchmark.',
            'Preparation, writes and audit expansion are reported separately; no generation model was invoked.']}
    (args.out/'measurements.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))


if __name__ == '__main__':
    main()
