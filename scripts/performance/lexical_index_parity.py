#!/usr/bin/env python3
"""#771 exact native selection/projection replay. BASELINE CANDIDATE OUT SCRATCH."""
import copy
import json
from pathlib import Path
import shutil
import sys
from temporal_body_selection import Client, dump, fixture, sha


def main():
    before, after, out, scratch = [Path(p).resolve() for p in sys.argv[1:]]
    out.mkdir(parents=True, exist_ok=False)
    scratch.mkdir(parents=True, exist_ok=False)
    try:
        seed, _ = fixture(32, 1024)
        with (out / 'seed.jsonl').open('w') as trace:
            client = Client(before, scratch / 'seed', trace)
            try:
                client.call('kmp_ingest', seed)
                other = json.loads(json.dumps(seed).replace(seed['about'], 'project:other-lexical'))
                other['idempotency_key'] = 'second-about'
                client.call('kmp_ingest', other)
            finally:
                client.close()
        base = {'about': seed['about'], 'question': 'quantity 17 excludes B', 'budget': {'max_bytes': 2_000_000, 'detail': 'full'}}
        cases = [base]
        for policy in ['evidence_or_unknown', 'best_effort', 'show_conflicts']:
            for question in ['quantity 17 excludes B', 'unrelated zeppelin silver', 'cantidad excluye']:
                cases.append({**base, 'question': question, 'answer_policy': policy})
        for axis in ['occurred', 'observed', 'ingested', 'validity', 'default']:
            cases.append({**base, 'axis': axis, 'as_of': {'time': '2026-09-10T10:00:14Z'}})
            cases.append({**base, 'axis': axis, 'interval': {'start': '2026-09-10T10:00:01Z', 'end': '2026-09-10T10:00:25Z'}})
        for dimensions in [{'mode': 'only', 'include': ['task']}, {'mode': 'except', 'exclude': ['topic']},
            {'selectors': [{'key': 'topic', 'op': 'in', 'values': ['shared']}]},
            {'selectors': [{'key': 'topic', 'op': 'notexists'}]},
            {'scope': 'abouts', 'abouts': ['project:other-lexical', seed['about']]},
            {'scope': 'all_abouts', 'selectors': [{'key': 'task', 'op': 'exists'}]}]:
            cases.append({**base, 'dimensions': dimensions})
        cases.append({**base, 'budget': {'max_bytes': 24000, 'detail': 'full', 'max_entries': 3}})
        results = {}
        page_counts = {}
        for side, binary in [('before', before), ('after', after)]:
            shutil.copytree(scratch / 'seed', scratch / side)
            with (out / f'{side}.jsonl').open('w') as trace:
                client = Client(binary, scratch / side, trace)
                try:
                    results[side] = []
                    for query in cases:
                        # Repeat immediately to exercise reuse, then revisit a
                        # prior collection after another scope/clock evicts it.
                        for _ in range(2):
                            result, _ = client.call('kmp_ask', copy.deepcopy(query))
                            results[side].append(result)
                    result, _ = client.call('kmp_ask', base)
                    results[side].append(result)
                    # A historical cut admits real evidence despite the
                    # deliberate expiry fixture. Follow every returned page
                    # verbatim, including any budget-restoration action.
                    query = {**base, 'axis': 'observed',
                        'as_of': {'time': '2026-09-10T10:00:31Z'},
                        'budget': {'max_bytes': 24000, 'detail': 'full'}}
                    pages = 0
                    seen = set()
                    while True:
                        result, _ = client.call('kmp_ask', query)
                        results[side].append(result)
                        pages += 1
                        content = result['structuredContent']
                        assert content['because'], 'paged control must cite real evidence'
                        action = content.get('projection', {}).get('next_action')
                        if not action:
                            break
                        assert action['tool'] == 'kmp_ask'
                        query = action['arguments']
                        identity = json.dumps(query, sort_keys=True)
                        assert identity not in seen and pages < 100, 'continuation must advance'
                        seen.add(identity)
                    assert pages > 1, 'control must exercise continuation'
                    page_counts[side] = pages
                finally:
                    client.close()
        for i, (left, right) in enumerate(zip(results['before'], results['after'], strict=True)):
            if left != right:
                dump(out / 'mismatch.json', {'index': i, 'left': left, 'right': right})
                raise AssertionError(f'complete native result differs: {i}')
        assert page_counts['before'] == page_counts['after']
        dump(out / 'summary.json', {'exact_complete_results': len(results['before']), 'continuation_pages_per_side': page_counts['before'], 'queries': cases,
            'baseline_sha256': sha(before), 'candidate_sha256': sha(after), 'runner_sha256': sha(Path(__file__)),
            'timing': 'correctness only, do not interpret timings', 'normalized_fields': []})
        print(json.dumps({'exact_complete_results': len(results['before'])}))
    finally:
        shutil.rmtree(scratch)


if __name__ == '__main__':
    main()
