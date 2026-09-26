"""Manual audit of generated negatives and guards (BENCH_SPEC 4.2: 20 % by hand).

`audit-sample` draws ceil(fraction x n) questions of every type and form (guards:
every family and polarity) with the rules' seed, and writes a private sheet
with each question, its gold and the stored text of every entry the gold
names, plus the anchor's own entries. A reviewer writes one verdict per
sampled id in `audit-verdicts.jsonl`: `{"id", "verdict": "correct"|"error",
"note"}`. `audit-score` checks the verdicts cover the sample exactly and
reports the error rate per group and overall with Wilson 95 % intervals.
"""
import json
import math
from pathlib import Path

from ..domain import jsonl
from ..domain.cachekey import canonical_json
from ..domain.prng import SplitMix64
from ..domain.question import load_questions
from ..domain.stats import wilson
from .anchor_forms import Anchor
from .errors import AuditInvalid
from .negative_rules import load_rules
from .store_snapshot import read_store

SHEET, VERDICTS, SUMMARY = 'audit-sheet.jsonl', 'audit-verdicts.jsonl', 'audit-summary.json'
FILES = ('hard-negatives.jsonl', 'identifier-guards.jsonl')
VERDICT_VALUES = ('correct', 'error')
EXCERPT = 700


def group_of(question):
    tags = dict(tag.split(':', 1) for tag in question.tags if ':' in tag)
    if question.type == 'identifier_guard':
        return f'identifier_guard/{tags["family"]}/{tags["polarity"]}'
    return f'{question.type}/{tags["form"]}'


def sample(questions, fraction, seed):
    """Sampled question ids per group, in a fixed order."""
    groups = {}
    for question in questions:
        groups.setdefault(group_of(question), []).append(question.id)
    rng, chosen = SplitMix64(seed).fork('audit'), {}
    for group in sorted(groups):
        ids = sorted(groups[group])
        chosen[group] = sorted(rng.fork(group).sample(ids, math.ceil(fraction * len(ids))))
    return chosen


def _excerpt(text):
    return text if len(text) <= EXCERPT else text[:EXCERPT] + ' [...]'


def sheet_rows(questions, chosen, snapshot):
    docs = {e.ref: e for about in snapshot.abouts for e in about.entries}
    wanted = {i for ids in chosen.values() for i in ids}
    rows = []
    for question in questions:
        if question.id not in wanted:
            continue
        gold = question.gold
        named = set(gold.wrong_subject) | set(gold.excluded_refs)
        for facet in gold.facets:
            named |= set(facet.answers) | set(facet.related)
        anchor_entries = []
        about = snapshot.about(question.about)
        for anchor in (Anchor.parse(a.lower()) for a in question.anchors):
            if anchor is not None:
                pattern = anchor.strict_pattern()
                anchor_entries += [e.ref for e in about.entries if pattern.search(e.joined())]
        rows.append({'id': question.id, 'group': group_of(question), 'about': question.about,
                     'question': question.question, 'gold': gold.as_dict(), 'source': question.source,
                     'entries': {ref: _excerpt(docs[ref].joined()) for ref in sorted(named | set(anchor_entries))
                                 if ref in docs}})
    return rows


def score(chosen, verdicts):
    by_id = {}
    for row in verdicts:
        if row.get('verdict') not in VERDICT_VALUES or row.get('id') in by_id:
            raise AuditInvalid(f'bad or repeated verdict: {row}')
        by_id[row['id']] = row['verdict']
    expected = {i for ids in chosen.values() for i in ids}
    if set(by_id) != expected:
        raise AuditInvalid(f'verdicts do not match the sample: missing {sorted(expected - set(by_id))[:5]}, '
                           f'extra {sorted(set(by_id) - expected)[:5]}')

    def cell(ids):
        errors = sum(by_id[i] == 'error' for i in ids)
        low, high = wilson(errors, len(ids))
        return {'audited': len(ids), 'errors': errors, 'error_rate': errors / len(ids),
                'wilson95': [low, high]}
    summary = {group: cell(ids) for group, ids in sorted(chosen.items())}
    negatives = [i for g, ids in chosen.items() if not g.startswith('identifier_guard/') for i in ids]
    guards = [i for g, ids in chosen.items() if g.startswith('identifier_guard/') for i in ids]
    return {'groups': summary, 'hard_negatives': cell(negatives) if negatives else None,
            'identifier_guards': cell(guards) if guards else None}


def _questions(out):
    return [q for name in FILES if (Path(out) / name).exists() for q in load_questions(Path(out) / name)]


def run_sample(args):
    out, rules = jsonl.guard_private_path(args.out), load_rules()
    questions = _questions(out)
    chosen = sample(questions, rules['audit_fraction'], rules['seed'])
    rows = sheet_rows(questions, chosen, read_store(args.store))
    jsonl.write_text(out / SHEET, jsonl.dump_lines(rows), overwrite=True)
    print(json.dumps({g: len(ids) for g, ids in chosen.items()}, indent=2, sort_keys=True))
    return 0


def run_score(args):
    out, rules = jsonl.guard_private_path(args.out), load_rules()
    chosen = sample(_questions(out), rules['audit_fraction'], rules['seed'])
    verdicts = [row for _, row in jsonl.parse_lines((out / VERDICTS).read_text(encoding='utf-8'),
                                                    str(out / VERDICTS), AuditInvalid)]
    summary = score(chosen, verdicts)
    jsonl.write_text(out / SUMMARY, canonical_json(summary) + '\n', overwrite=True)
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


def add_commands(commands):
    sample_command = commands.add_parser('audit-sample', help='draw the audit sample and write the sheet')
    sample_command.add_argument('--out', type=Path, required=True)
    sample_command.add_argument('--store', type=Path, required=True)
    sample_command.set_defaults(action=run_sample)
    score_command = commands.add_parser('audit-score', help='error rate from audit-verdicts.jsonl')
    score_command.add_argument('--out', type=Path, required=True)
    score_command.set_defaults(action=run_score)

