"""LongMemEval-S as an evidence-recall corpus (BENCH_SPEC 4.6, BT17).

Source: `longmemeval_s_cleaned.json` of xiaowu0162/longmemeval-cleaned (MIT), the
authors' replacement of the original LongMemEval-S. Each question comes with its
own haystack (about 50 sessions, about 500 turns), so each question is one about,
`longmemeval:<question id>`, as in `longmemeval_kmp_adapter.rs`.

Store side: one turn per entry, `<about>:s<session index>:t<n>` (haystack position:
some haystacks repeat a session id), text verbatim, role and session id in metadata;
empty turns are skipped. Sessions load in date order (the haystack is not
chronological in 211 of 500 items), turns in dialogue order; `occurred_at` is the
session time plus one second per turn, so it strictly increases. Unlike the Rust
adapter, nothing from the gold reaches the store: no `evidence`, no
`supports_answer` relation, no `has_answer` flag.

Question side: the evidence is every turn LongMemEval marks `has_answer`; the
question is `evidence_recall` asked with `evidence_or_unknown` (the policy the Rust
runner used). The 30 `_abs` items are UNKNOWN (`no_evidence`): the abstention set.
`classify_evidence` reproduces `classify_evidence_hit` of
`longmemeval_kmp_runner.rs:532-551` (Full / Partial / Missing / not applicable),
and `session_of` maps a turn ref to its session for session-level recall.

Subset: `per_type` questions of each question type in file order, plus
`abstention` `_abs` items (all 30 by default).
"""
from datetime import datetime, timedelta, timezone
import json
import re

from .public_corpus import CorpusAbout, CorpusEntry, PublicCorpus, evidence_question, iso, slug
from .public_errors import CorpusError

ADAPTER_VERSION = 'longmemeval-s.v1'
DATASET = 'longmemeval-s'
FILE = 'longmemeval_s_cleaned.json'
CORPUS = 'longmemeval-s'
DATE = re.compile(r'(\d{4})[/-](\d{2})[/-](\d{2})(?:\s*\([A-Za-z]{3}\))?(?:\s+(\d{2}):(\d{2}))?')
TURN_REF = re.compile(r'(?P<about>.+):s(?P<session>\d+):t(?P<turn>\d+)')
FULL, PARTIAL, MISSING, NOT_APPLICABLE = 'full', 'partial', 'missing', 'not_applicable'


def parse_date(raw):
    """`2023/05/20 (Sat) 02:21` -> aware datetime (UTC, as the Rust adapter normalises it)."""
    match = DATE.fullmatch(str(raw).strip())
    if not match:
        raise CorpusError(f'unreadable LongMemEval date {raw!r}')
    year, month, day, hour, minute = match.groups()
    return datetime(int(year), int(month), int(day), int(hour or 0), int(minute or 0),
                    tzinfo=timezone.utc)


def is_abstention(item):
    return item['question_id'].endswith('_abs')


def about_of(item):
    return f'longmemeval:{slug(item["question_id"])}'


def session_ref(about, session_index):
    """Sessions are named by haystack position: some haystacks repeat a session id."""
    return f'{about}:s{session_index}'


def turn_ref(about, session_index, one_based):
    return f'{session_ref(about, session_index)}:t{one_based}'


def session_of(ref):
    """`<about>:s<index>` of a turn ref, or None for anything else."""
    match = TURN_REF.fullmatch(ref)
    return None if match is None else session_ref(match.group('about'), match.group('session'))


def classify_evidence(expected, observed, abstention=False):
    """`classify_evidence_hit` (longmemeval_kmp_runner.rs:532-551)."""
    expected = list(expected)
    if abstention or not expected:
        return NOT_APPLICABLE
    observed = set(observed)
    if all(ref in observed for ref in expected):
        return FULL
    if any(ref in observed for ref in expected):
        return PARTIAL
    return MISSING


def item_about(item):
    """(CorpusAbout, answer turn refs) of one LongMemEval item."""
    about = about_of(item)
    sessions = list(zip(item['haystack_session_ids'], item['haystack_dates'], item['haystack_sessions']))
    if not sessions:
        raise CorpusError(f'{item["question_id"]}: empty haystack')
    ordered = sorted(enumerate(sessions), key=lambda pair: (parse_date(pair[1][1]), pair[0]))
    entries, answers, sequence, last = [], [], 0, None
    for original_index, (session_id, raw_date, turns) in ordered:
        start = parse_date(raw_date)
        for index, turn in enumerate(turns):
            if not str(turn.get('content') or '').strip():
                continue  # a few haystack turns are empty; their position keeps its number
            moment = start + timedelta(seconds=index)
            if last is not None and moment <= last:
                moment = last + timedelta(seconds=1)
            last = moment
            sequence += 1
            ref = turn_ref(about, original_index, index + 1)
            role = str(turn.get('role', '')).strip().lower() or 'unknown'
            entries.append(CorpusEntry(ref, f'{role}_message', turn['content'], sequence, iso(moment),
                                       {'session_id': session_id, 'session_date': raw_date,
                                        'session_index': str(original_index), 'turn_index': str(index),
                                        'role': role}))
            if turn.get('has_answer'):
                answers.append(ref)
    return CorpusAbout(about, tuple(entries)), answers


def answer_sessions(item, about):
    """Session refs of the haystack sessions LongMemEval names as evidence sessions."""
    wanted = set(item.get('answer_session_ids') or ())
    return [session_ref(about, index) for index, session_id in enumerate(item['haystack_session_ids'])
            if session_id in wanted]


def select(items, per_type=10, abstention=30):
    """Items in file order: `per_type` of each non-abstention type, then `abstention` `_abs`."""
    counts, chosen, abs_chosen = {}, [], []
    for item in items:
        if is_abstention(item):
            if len(abs_chosen) < abstention:
                abs_chosen.append(item)
            continue
        kind = item['question_type']
        if per_type is None or counts.get(kind, 0) < per_type:
            counts[kind] = counts.get(kind, 0) + 1
            chosen.append(item)
    return chosen + abs_chosen


def load_items(path):
    with open(path, encoding='utf-8') as handle:
        items = json.load(handle)
    if not isinstance(items, list) or not items:
        raise CorpusError(f'{path}: not a LongMemEval list')
    return items


def build(items, per_type=10, abstention=30, policy='evidence_or_unknown'):
    chosen = select(items, per_type, abstention)
    abouts, questions, turns, dropped = [], [], 0, []
    for item in chosen:
        about, answers = item_about(item)
        abstain = is_abstention(item)
        if not abstain and not answers:
            dropped.append(item['question_id'])  # its only evidence turn was empty
            continue
        abouts.append(about)
        turns += len(about.entries)
        answer = item['answer'] if isinstance(item['answer'], str) else json.dumps(item['answer'])
        questions.append(evidence_question(
            identifier=f'lme-{slug(item["question_id"])}', corpus=CORPUS, about=about.about,
            question=item['question'], answers=[] if abstain else answers, policy=policy,
            tags=(f'qtype:{item["question_type"]}',) + (('abstention',) if abstain else ()),
            source={'kind': 'dataset', 'dataset': DATASET, 'ref': item['question_id'],
                    'rule': 'has_answer-turns.v1', 'answer': answer,
                    'question_date': item['question_date'],
                    'answer_sessions': ','.join(answer_sessions(item, about.about))}))
    selection = {'dataset': DATASET, 'per_type': per_type, 'abstention': abstention,
                 'question_ids': [item['question_id'] for item in chosen
                                  if item['question_id'] not in dropped]}
    notes = {'questions': len(questions), 'turns': turns, 'policy': policy,
             'abstention_items': sum(1 for item in chosen if is_abstention(item)), 'dropped': dropped}
    return PublicCorpus(f'longmemeval-s-{per_type}x-abs{abstention}', DATASET, ADAPTER_VERSION,
                        selection, tuple(abouts), tuple(questions), notes=notes).check()
