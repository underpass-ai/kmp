#!/usr/bin/env python3
"""#771 native Ask ABBA control. BASELINE CANDIDATE OUTPUT SCRATCH [SAMPLES].
Synthetic closed seed copied per process. Exact complete responses checked before
interpreting samples. No model, network or tokenizer; no performance CI gate.
"""
import copy
import json
import os
from pathlib import Path
import platform
import shutil
import sys
from temporal_body_selection import Client, dump, fixture, sha, stats


def main():
    baseline, candidate, out, scratch = [Path(p).resolve() for p in sys.argv[1:5]]
    count_samples = int(sys.argv[5]) if len(sys.argv) > 5 else 20
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    dump(out / 'environment.json', {
        'baseline_sha256': sha(baseline), 'candidate_sha256': sha(candidate),
        'runner_sha256': sha(Path(__file__)),
        'transport_runner_sha256': sha(Path(__file__).with_name('temporal_body_selection.py')),
        'platform': platform.platform(), 'cpu': Path('/proc/cpuinfo').read_text(),
        'profile': 'dev, workspace Cargo configuration; Rust 1.97.1',
        'order': ['before', 'after', 'after', 'before'], 'samples_per_process': count_samples,
        'warmups': 3, 'startup_included': False, 'concurrency': 1,
        'cold': 'fresh process, OS page cache not flushed; seed created before measurement',
        'memory': 'process peak RSS includes startup and correctness checks',
        'tokens': 'not measured; this runner measures native bytes', 'model_calls': 0,
        'physical_disk_io': 'not measured', 'allocations': 'separate LD_PRELOAD control',
    })
    rows = []
    try:
        for name, entries, body, vocabulary in [('small', 8, 256, 4), ('medium', 64, 1024, 16), ('wide_vocabulary', 256, 1024, 64), ('large_body', 32, 32768, 16)]:
            seed, refs = fixture(entries, body)
            for i, entry in enumerate(seed['memory']['entries']):
                entry['text'] = ('The cache valkey rollout was delayed during the audit. ' if i % 3 == 0 else 'The invoice supplier payment was approved during the meeting. ') + ' '.join(f'vocabulary{n}' for n in range(vocabulary))
            for entry in seed['memory']['entries']:
                for coordinate in entry['coordinates']:
                    coordinate.pop('valid_until', None)
            dump(out / f'{name}-input.json', seed)
            with (out / f'{name}-seed.jsonl').open('w') as trace:
                client = Client(baseline, scratch / name / 'seed', trace)
                try:
                    client.call('kmp_ingest', seed)
                finally:
                    client.close()
            query = {'about': seed['about'], 'question': 'cache valkey rollout', 'budget': {'max_bytes': 2_000_000, 'detail': 'full'}}
            expected = None
            combined = {'before': [], 'after': []}
            first = {'before': [], 'after': []}
            for round_index, side in enumerate(['before', 'after', 'after', 'before']):
                store = scratch / name / f'{round_index}-{side}'
                shutil.copytree(scratch / name / 'seed', store)
                with (out / f'{name}-{round_index}-{side}.jsonl').open('w') as trace:
                    client = Client(baseline if side == 'before' else candidate, store, trace)
                    try:
                        for i in range(count_samples + 4):
                            response, measured = client.call('kmp_ask', query)
                            if expected is None:
                                assert response['structuredContent']['because'], 'fixture must return citations'
                                expected = response
                            assert response == expected, (name, side, i, 'complete result changed')
                            if i == 0:
                                first[side].append(measured['elapsed_ms'])
                            if i >= 4:
                                combined[side].append(measured)
                    finally:
                        client.close()
            row = {'fixture': name, 'entries': entries, 'body_bytes': body, 'vocabulary_per_entry': vocabulary,
                   'first_query_ms': first, **{side: stats(samples) for side, samples in combined.items()}}
            rows.append(row)
            dump(out / 'results.json', rows)
            print(json.dumps(row), flush=True)
    finally:
        shutil.rmtree(scratch)


if __name__ == '__main__':
    main()
