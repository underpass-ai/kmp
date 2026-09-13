#!/usr/bin/env python3
"""#771 write/invalidation control. BASELINE CANDIDATE OUT SCRATCH.
ABBA processes update one canonical entry 12 times, with two Ask reads after each
write. Read-only SQL dumps verify that those reads add zero kernel DB mutations.
"""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import sys
from temporal_body_selection import Client, dump, fixture, stats, sha


def kernel_state(store):
    with sqlite3.connect(f'file:{store / "store/kernel.sqlite3"}?mode=ro', uri=True) as connection:
        # Includes every kernel table, not the separate quality telemetry DB.
        digest = hashlib.sha256()
        for statement in connection.iterdump():
            digest.update(statement.encode())
            digest.update(b'\n')
        pages = connection.execute('PRAGMA page_count').fetchone()[0]
        return {'sha256': digest.hexdigest(), 'pages': pages}


def main():
    before, after, out, scratch = [Path(p).resolve() for p in sys.argv[1:]]
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    seed, refs = fixture(32, 1024)
    for entry in seed['memory']['entries']:
        for coordinate in entry['coordinates']:
            coordinate.pop('valid_until', None)
    rows = []
    try:
        with (out / 'seed.jsonl').open('w') as trace:
            client = Client(before, scratch / 'seed', trace)
            try:
                client.call('kmp_ingest', seed)
            finally:
                client.close()
        for round_index, side in enumerate(['before', 'after', 'after', 'before']):
            store = scratch / f'{round_index}-{side}'
            shutil.copytree(scratch / 'seed', store)
            observations = []
            with (out / f'{round_index}-{side}.jsonl').open('w') as trace:
                client = Client(before if side == 'before' else after, store, trace)
                try:
                    query = {'about': seed['about'], 'question': 'quantity 17 excludes B', 'budget': {'max_bytes': 2_000_000, 'detail': 'full'}}
                    client.call('kmp_ask', query)
                    for n in range(12):
                        entry = copy.deepcopy(seed['memory']['entries'][0])
                        marker = f'Revisionmarker{n:03}'
                        entry['text'] = f'{marker}: quantity 17 excludes B.'
                        packet = {'about': seed['about'], 'idempotency_key': f'update-{n}',
                            'memory': {'dimensions': seed['memory']['dimensions'], 'entries': [entry], 'evidence': [], 'relations': []}}
                        _, write_sample = client.call('kmp_ingest', packet)
                        committed = kernel_state(store)
                        first, first_sample = client.call('kmp_ask', query)
                        warm, warm_sample = client.call('kmp_ask', query)
                        assert first == warm, (side, n, 'warm response differs after mutation')
                        assert marker in json.dumps(first), (side, n, 'new source text missing')
                        assert kernel_state(store) == committed, (side, n, 'Ask changed the kernel store')
                        observations.append({'write': write_sample, 'first_read': first_sample, 'warm_read': warm_sample, 'kernel_state': committed})
                finally:
                    client.close()
            row = {'round': round_index, 'side': side, 'samples': observations,
                **{kind: stats([r[kind] for r in observations]) for kind in ['write', 'first_read', 'warm_read']}}
            rows.append(row)
            dump(out / 'results.json', rows)
            print(json.dumps({k:v for k,v in row.items() if k != 'samples'}), flush=True)
        dump(out / 'environment.json', {'baseline_sha256': sha(before), 'candidate_sha256': sha(after), 'runner_sha256': sha(Path(__file__)), 'concurrency': 1, 'scope': '24 writes and 48 post-write reads per side; SQL dump hashing excluded from call times', 'correctness': 'each fresh/warm result pair exactly equal; all kernel tables unchanged by reads; each replacement marker visible', 'timing': 'dev synthetic control, no latency thresholds; startup excluded'})
    finally:
        shutil.rmtree(scratch)


if __name__ == '__main__':
    main()
