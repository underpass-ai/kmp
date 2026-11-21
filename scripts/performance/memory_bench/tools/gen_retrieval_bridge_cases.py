"""Writes the shipped-bridge stress cases for `retrieval_kmp_scorecard`.

The judged collection measures the lexical-bridge mechanism with a small judged
table. These cases measure what the table `kmp-mcp setup` installs does to the
promise that `kmp_ask` never answers what is not stored, and they are scored
with that table (`scripts/ci/retrieval-baseline.sh`).

Every case comes from a synth-v1 mono world at level 1000 (seeds 7 11 13 17 19
23; public, generated, `memory_bench world`):

- the four anchored negative types (`anchor_absent`, `near_miss_attribute`,
  `anchor_neighbor_existing`, `singular_anchored_twin`): the store holds no
  answer, so any answer is false;
- `crosslang`: a question in one language whose gold memory is in the other,
  gold kept; scored as a positive (recall, core precision, false UNKNOWN);
- `crosslang_gold_out` and `paraphrase_gold_out`: the same questions, and the
  zero-overlap paraphrases, with every gold memory removed: negatives.

A case's memory is its neighbourhood in the whole world: every entry that names
one of the question's anchors as a whole token, and every entry the reference
binary's proof returned for the question on the whole store, with the shipped
table and without one (`--neighbourhood`, the JSON `stress_negatives.py`
writes). The texts are the world's own; nothing private enters.

Which cases are kept (`--with-table`, `--without-table`: the per-case results of
scoring every generated case on the reference binary with the shipped table and with
none). A case the reference answers falsely without any table is dropped: it says
nothing about the table. Kept are every negative the reference answered with the
table (the regression), the first `--sample` negatives of each kind besides, and every
cross-language positive the reference or this checkout answered.

Usage:
  python3 -m scripts.performance.memory_bench.tools.gen_retrieval_bridge_cases \\
      --worlds ROOT/worlds --neighbourhood stress-base.json \\
      --with-table cases-base.json --without-table cases-none.json \\
      [--answered-here cases-fix.json] --out FILE
"""
import argparse
import json
import re
from pathlib import Path

NEGATIVES = ('anchor_absent', 'near_miss_attribute', 'anchor_neighbor_existing',
             'singular_anchored_twin')
CROSSLANG = ('crosslang_en_es', 'crosslang_es_en')
PARAPHRASE = 'paraphrase_zero_overlap'
# The neighbourhood a proof returned is capped, so a case stays a small store.
MAX_NEIGHBOURS = 24


def names_token(text, token):
    """Whether `text` carries `token` as a whole token (not inside `svc-0091`)."""
    return re.search(rf'(?<![\w.-]){re.escape(token)}(?![\w]|[.-]\w)', text) is not None


def entry(record, sequence):
    return {
        'id': record['ref'],
        'kind': record['kind'],
        'text': record['text'],
        'coordinates': [dict(record['coordinates'][0], sequence=sequence)],
    }


def memory(records):
    ordered = sorted(records, key=lambda record: record['ordinal'])
    return {
        'dimensions': [{'id': 'work:main', 'kind': 'work'}],
        'entries': [entry(record, index + 1) for index, record in enumerate(ordered)],
    }


def case(question, kind, records, judged, probes):
    return {
        'id': question['id'] + ('' if kind in NEGATIVES or kind == 'crosslang' else '-' + kind),
        'kind': kind,
        'probes': probes,
        'about': question['about'],
        'question': question['question'],
        'answer_policy': 'evidence_or_unknown',
        'judged': list(judged),
        'absent': [],
        'memory': memory(records),
    }


def _results(path):
    if not path:
        return {}
    return {case['id']: case for case in json.loads(Path(path).read_text())['cases']}


def select(cases, args):
    with_table = _results(args.with_table)
    without_table = _results(args.without_table)
    here = _results(args.answered_here)
    kept, sampled = [], {}
    for case in cases:
        if without_table.get(case['id'], {}).get('false_answer'):
            continue
        if case['judged']:
            answered = any(not results.get(case['id'], {}).get('false_unknown', True)
                           for results in (with_table, here))
            if answered:
                kept.append(case)
        elif with_table.get(case['id'], {}).get('false_answer'):
            kept.append(case)
        elif sampled.get(case['kind'], 0) < args.sample:
            sampled[case['kind']] = sampled.get(case['kind'], 0) + 1
            kept.append(case)
    return kept


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--worlds', required=True)
    parser.add_argument('--neighbourhood', required=True)
    parser.add_argument('--with-table')
    parser.add_argument('--without-table')
    parser.add_argument('--answered-here')
    parser.add_argument('--sample', type=int, default=4)
    parser.add_argument('--out', required=True)
    args = parser.parse_args()
    proofs = {}
    for row in json.loads(Path(args.neighbourhood).read_text())['rows']:
        refs = proofs.setdefault(row['id'], [])
        refs.extend(ref for ref in row['evidence'] if ref and ref not in refs)
    cases = []
    for world in sorted(Path(args.worlds).iterdir(), key=lambda path: path.name):
        seed = json.loads((world / 'manifest.json').read_text())['seed']
        records = {}
        for line in (world / 'entries.jsonl').open():
            record = json.loads(line)
            if record['block'] == 0:
                records[record['ref']] = record
        for line in (world / 'questions.jsonl').open():
            question = json.loads(line)
            kind = question['type']
            if question['min_level'] != 1000 or kind not in NEGATIVES + CROSSLANG + (PARAPHRASE,):
                continue
            gold = sorted({ref for facet in question['gold']['facets'] for ref in facet['answers']})
            refs = [ref for ref in proofs.get(question['id'], []) if ref in records]
            refs = refs[:MAX_NEIGHBOURS]
            for anchor in question.get('anchors') or []:
                refs.extend(ref for ref, record in records.items()
                            if names_token(record['text'], anchor) and ref not in refs)
            with_gold = [records[ref] for ref in dict.fromkeys(refs + gold)]
            without_gold = [records[ref] for ref in dict.fromkeys(refs) if ref not in gold]
            where = f'synth-v1 seed {seed}, {question["id"]}'
            if kind in NEGATIVES:
                cases.append(case(question, kind, with_gold, [],
                                  f'{where}: no memory states what is asked of the anchor.'))
            elif kind in CROSSLANG:
                cases.append(case(question, 'crosslang', with_gold, gold,
                                  f'{where}: the gold memory is in the other language.'))
                cases.append(case(question, 'crosslang_gold_out', without_gold, [],
                                  f'{where}: the gold memory removed; nothing else answers.'))
            else:
                cases.append(case(question, 'paraphrase_gold_out', without_gold, [],
                                  f'{where}: the paraphrased memory removed; nothing else answers.'))
    if args.with_table:
        cases = select(cases, args)
    note = ('Shipped-bridge stress cases, generated by '
            'scripts/performance/memory_bench/tools/gen_retrieval_bridge_cases.py from the '
            'synth-v1 mono 1000 worlds of seeds 7 11 13 17 19 23. Scored with '
            'distribution/lexical-bridge/kmp-lexical-bridge.kmpb against '
            'docs/development/retrieval-bridge-stress-baseline.tsv, and asked by '
            'crates/kmp-mcp/tests/shipped_lexical_bridge.rs.')
    Path(args.out).write_text(json.dumps({'note': note, 'cases': cases}, indent=1,
                                         ensure_ascii=False) + '\n')
    print(len(cases), 'cases')


if __name__ == '__main__':
    main()
