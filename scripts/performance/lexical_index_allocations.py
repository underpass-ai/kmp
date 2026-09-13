#!/usr/bin/env python3
"""#771 Linux/glibc allocation requests. BASELINE CANDIDATE COUNTER OUT SCRATCH.
One-query and five-query processes separate startup/first-build from additional
warm-query allocation requests. Counts are not live bytes or retained heap.
"""
import json
import os
from pathlib import Path
import shutil
import sys
from temporal_body_selection import Client, dump, fixture, sha


def main():
    before, after, counter, out, scratch = [Path(p).resolve() for p in sys.argv[1:]]
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    rows = []
    try:
        for name, entries, vocabulary in [('small', 8, 4), ('wide_vocabulary', 256, 64)]:
            seed, _ = fixture(entries, 1024)
            for entry in seed['memory']['entries']:
                entry['text'] += ' '.join(f'vocabulary{n}' for n in range(vocabulary))
            for entry in seed['memory']['entries']:
                for coordinate in entry['coordinates']:
                    coordinate.pop('valid_until', None)
            with (out / f'{name}-seed.jsonl').open('w') as trace:
                client = Client(before, scratch / name / 'seed', trace)
                try:
                    client.call('kmp_ingest', seed)
                finally:
                    client.close()
            expected = None
            for side, binary in [('before', before), ('after', after)]:
                for queries in [1, 5]:
                    store = scratch / name / f'{side}-{queries}'
                    shutil.copytree(scratch / name / 'seed', store)
                    previous = os.environ.get('LD_PRELOAD')
                    os.environ['LD_PRELOAD'] = str(counter)
                    try:
                        with (out / f'{name}-{side}-{queries}.jsonl').open('w') as trace:
                            client = Client(binary, store, trace)
                    finally:
                        if previous is None:
                            del os.environ['LD_PRELOAD']
                        else:
                            os.environ['LD_PRELOAD'] = previous
                    # Keep the trace open until the client closes.
                    with (out / f'{name}-{side}-{queries}.jsonl').open('a') as trace:
                        client.trace = trace
                        try:
                            for _ in range(queries):
                                result, _ = client.call('kmp_ask', {'about': seed['about'], 'question': 'quantity 17 excludes B', 'budget': {'max_bytes': 2_000_000, 'detail': 'full'}})
                                if expected is None:
                                    expected = result
                                assert result == expected, (name, side, queries)
                        finally:
                            client.close()
                    stderr = (store / 'stderr.log').read_text()
                    (out / f'{name}-{side}-{queries}-stderr.log').write_text(stderr)
                    counters = [json.loads(line) for line in stderr.splitlines() if line.startswith('{"native_allocation_control":true,')]
                    assert len(counters) == 1
                    rows.append({'fixture': name, 'side': side, 'queries': queries, 'counts': counters[0]})
                    dump(out / 'results.json', rows)
        dump(out / 'environment.json', {'baseline_sha256': sha(before), 'candidate_sha256': sha(after), 'counter_sha256': sha(counter), 'runner_sha256': sha(Path(__file__)), 'scope': 'process startup, initialize and 1 or 5 identical queries; no seeding', 'measurement': 'allocation request count and cumulative requested bytes; not live heap', 'latency': 'instrumented timing is not interpreted'})
        print(json.dumps(rows))
    finally:
        shutil.rmtree(scratch)


if __name__ == '__main__':
    main()
