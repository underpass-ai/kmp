"""LoCoMo as a private evidence-recall corpus (BENCH_SPEC 4.6 and 14, BT17).

LoCoMo (`snap-research/locomo`, `data/locomo10.json`) is CC BY-NC 4.0: by Tirso's
decision it is evaluated locally only and nothing of it enters the repository. The
dataset file lives under `$MEMORY_BENCH_PRIVATE_ROOT/datasets/locomo/`, every
question is `private`, and its stores and runs go to the private cache layout
(`runtime/layout.private_layout`), which refuses any path inside the repository.

Store side: one conversation (`sample_id`) is one about, `locomo:<sample>`; one
dialogue turn is one entry, `<about>:<dia_id>` (`D3:7` -> `d3-7`), text
`<speaker>: <text>` plus the image caption when the turn shared one; sessions in
order, `occurred_at` the session time plus one second per turn.

Question side: categories 1-4 carry their `evidence` dialogue ids as the answering
facet (`evidence_recall`, best_effort: recall of evidence without a reader, the
measure EvalMem reports as 95.6 %). Category 5 (adversarial) is left out unless
`adversarial=True`, where it becomes an UNKNOWN abstention item. Evidence ids that
name no turn are dropped and counted in `notes`.
"""
from datetime import datetime, timedelta, timezone
import json
import re

from .public_corpus import CorpusAbout, CorpusEntry, PublicCorpus, evidence_question, iso, slug
from .public_errors import CorpusError

ADAPTER_VERSION = 'locomo.v1'
DATASET = 'locomo'
FILE = 'locomo10.json'
CORPUS = 'locomo'
SESSION_KEY = re.compile(r'session_(\d+)')
DIA_ID = re.compile(r'D(\d+):(\d+)')
CATEGORIES = {1: 'multi_hop', 2: 'temporal', 3: 'open_domain', 4: 'single_hop', 5: 'adversarial'}


def parse_session_time(raw):
    """`1:56 pm on 8 May, 2023` -> aware datetime (UTC)."""
    try:
        moment = datetime.strptime(raw.strip(), '%I:%M %p on %d %B, %Y')
    except ValueError as failure:
        raise CorpusError(f'unreadable LoCoMo session time {raw!r}') from failure
    return moment.replace(tzinfo=timezone.utc)


def turn_ref(about, dia_id):
    match = DIA_ID.fullmatch(dia_id.strip())
    if not match:
        return None
    return f'{about}:d{int(match.group(1))}-{int(match.group(2))}'


def sessions(conversation):
    """[(number, datetime, turns)] in session order."""
    found = []
    for key, turns in conversation.items():
        match = SESSION_KEY.fullmatch(key)
        if match and isinstance(turns, list) and turns:
            found.append((int(match.group(1)), parse_session_time(conversation[f'{key}_date_time']), turns))
    return sorted(found, key=lambda item: item[0])


def sample_about(sample):
    about = f'locomo:{slug(sample["sample_id"])}'
    entries, last, sequence = [], None, 0
    for number, start, turns in sessions(sample['conversation']):
        for index, turn in enumerate(turns):
            ref = turn_ref(about, turn['dia_id'])
            if ref is None:
                raise CorpusError(f'{about}: unreadable dia_id {turn["dia_id"]!r}')
            moment = start + timedelta(seconds=index)
            if last is not None and moment <= last:
                moment = last + timedelta(seconds=1)
            last = moment
            sequence += 1
            text = f'{turn["speaker"]}: {turn["text"]}'
            if turn.get('blip_caption'):
                text += f' [shares an image: {turn["blip_caption"]}]'
            entries.append(CorpusEntry(ref, 'dialogue_turn', text, sequence, iso(moment),
                                       {'session': str(number), 'speaker': turn['speaker']}))
    return CorpusAbout(about, tuple(entries))


def evidence_refs(about, evidence, known):
    """Refs of the evidence ids that name a stored turn, and how many did not."""
    refs, unknown = [], 0
    for item in evidence or ():
        for part in re.split(r'[;,\s]+', str(item)):
            if not part:
                continue
            ref = turn_ref(about, part)
            if ref is None or ref not in known:
                unknown += 1
            elif ref not in refs:
                refs.append(ref)
    return refs, unknown


def load_samples(path):
    with open(path, encoding='utf-8') as handle:
        samples = json.load(handle)
    if not isinstance(samples, list) or not samples:
        raise CorpusError(f'{path}: not a LoCoMo list')
    return samples


def build(samples, adversarial=False, policy='best_effort'):
    abouts, questions, unknown_ids, skipped = [], [], 0, 0
    for sample in samples:
        about = sample_about(sample)
        abouts.append(about)
        known = {entry.ref for entry in about.entries}
        for index, qa in enumerate(sample['qa']):
            category = qa.get('category')
            is_adversarial = category == 5
            if is_adversarial and not adversarial:
                continue
            refs, missing = evidence_refs(about.about, qa.get('evidence'), known)
            unknown_ids += missing
            if not is_adversarial and not refs:
                skipped += 1
                continue
            questions.append(evidence_question(
                identifier=f'locomo-{slug(sample["sample_id"])}-q{index:03d}', corpus=CORPUS,
                about=about.about, question=qa['question'], answers=[] if is_adversarial else refs,
                policy='evidence_or_unknown' if is_adversarial else policy,
                tags=(f'category:{CATEGORIES.get(category, category)}',), private=True,
                source={'kind': 'dataset', 'dataset': DATASET, 'ref': f'{sample["sample_id"]}#{index}',
                        'rule': 'locomo-evidence.v1'}))
    selection = {'dataset': DATASET, 'samples': [sample['sample_id'] for sample in samples]}
    notes = {'questions': len(questions), 'adversarial': adversarial, 'policy': policy,
             'unknown_evidence_ids': unknown_ids, 'skipped_without_evidence': skipped}
    return PublicCorpus('locomo10', DATASET, ADAPTER_VERSION, selection, tuple(abouts),
                        tuple(questions), private=True, notes=notes).check()
