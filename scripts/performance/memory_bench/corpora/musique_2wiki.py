"""MuSiQue and 2WikiMultihopQA as multi-hop evidence-recall corpora (BENCH_SPEC 4.6, BT17).

Two variants, both rebuilt from upstream data:

- `full`: HippoRAG 2's setup, the only one comparable with published figures. Its
  1,000 questions per dataset are the ones listed in the HippoRAG GitHub repository
  (MIT; only their ids are read, `datasets.lock.json` entry `hipporag-question-ids`),
  and question text, passages and gold come from upstream: MuSiQue-Ans v1.0 dev
  (CC BY 4.0) and 2Wiki `data_ids_april7` dev (Apache-2.0). The corpus is every
  passage of those questions, de-duplicated by text: 11,656 for MuSiQue and 6,119 for
  2Wiki, which `build` checks. (The Hugging Face `osunlp/HippoRAG_v2` copy declares no
  licence and is never a source.)
- `subset`: the first `limit` (200) of those questions and only their passages. It is
  nested in the full setup and only ever reports internal deltas.

Store side: one passage per entry, `<about>:p<ordinal>`, text `<title>\\n<paragraph>`,
in first-seen order with an increasing clock. Nothing about support reaches the store.

Question side: `multihop_why_k` with the chain of supporting passages (MuSiQue:
decomposition order; 2Wiki: supporting-fact order, unordered for comparisons) and
every supporting passage as the answering facet; R@2, R@5 and full-chain recovery
read it (`application/public_run.py`).
"""
import json
import zipfile

from ..domain.gold import Chain
from .public_corpus import CorpusAbout, CorpusEntry, PublicCorpus, evidence_question, synthetic_time
from .public_errors import CorpusError

ADAPTER_VERSION = 'multihop-passages.v1'
DATASETS = {
    'musique': {'archive': 'musique_v1.0.zip', 'member': 'data/musique_ans_v1.0_dev.jsonl',
                'ids': 'musique.json', 'id_field': 'id', 'passages': 11656, 'lock': 'musique'},
    '2wiki': {'archive': 'data_ids_april7.zip', 'member': 'dev.json',
              'ids': '2wikimultihopqa.json', 'id_field': '_id', 'passages': 6119, 'lock': '2wiki'},
}
FULL_QUESTIONS = 1000
SUBSET = 200
UNORDERED_2WIKI = ('comparison', 'bridge_comparison')


def read_archive_rows(archive, member):
    """Records of a zip member: a JSON list, or JSON Lines."""
    with zipfile.ZipFile(archive) as bundle:
        text = bundle.read(member).decode('utf-8')
    if text.lstrip().startswith('['):
        return json.loads(text)
    return [json.loads(line) for line in text.splitlines() if line.strip()]


def read_selection(path, id_field):
    """The ordered question ids of HippoRAG's reproduce/dataset file (nothing else is read)."""
    with open(path, encoding='utf-8') as handle:
        records = json.load(handle)
    ids = [record[id_field] for record in records]
    if len(ids) != len(set(ids)):
        raise CorpusError(f'{path}: duplicate question ids')
    return ids


def _musique_passages(row):
    return [(p['title'], p['paragraph_text']) for p in row['paragraphs']]


def _musique_gold(row):
    """(chain passage indices in hop order, supporting indices, ordered, tags)."""
    chain = [step['paragraph_support_idx'] for step in row['question_decomposition']]
    support = [p['idx'] for p in row['paragraphs'] if p['is_supporting']]
    by_idx = {p['idx']: position for position, p in enumerate(row['paragraphs'])}
    return ([by_idx[i] for i in chain if i is not None], [by_idx[i] for i in support], True,
            (f'hops:{len(chain)}',))


def _2wiki_passages(row):
    return [(title, ''.join(sentences)) for title, sentences in row['context']]


def _2wiki_gold(row):
    titles = []
    for title, _ in row['supporting_facts']:
        if title not in titles:
            titles.append(title)
    positions = {}
    for position, (title, _) in enumerate(row['context']):
        positions.setdefault(title, position)
    missing = [title for title in titles if title not in positions]
    if missing:
        raise CorpusError(f'2wiki {row["_id"]}: supporting title not in context: {missing[0]!r}')
    chain = [positions[title] for title in titles]
    return chain, chain, row['type'] not in UNORDERED_2WIKI, (f'hops:{len(chain)}', f'qtype:{row["type"]}')


READERS = {'musique': (_musique_passages, _musique_gold, 'id'),
           '2wiki': (_2wiki_passages, _2wiki_gold, '_id')}


def build(dataset, rows, selection_ids, limit=SUBSET, policy='best_effort'):
    """One PublicCorpus over `selection_ids[:limit]` (limit None: the full HippoRAG 2 setup)."""
    if dataset not in DATASETS:
        raise CorpusError(f'unknown multi-hop dataset {dataset!r}')
    passages_of, gold_of, id_field = READERS[dataset]
    spec = DATASETS[dataset]
    by_id = {row[id_field]: row for row in rows}
    chosen = selection_ids if limit is None else selection_ids[:limit]
    missing = [qid for qid in chosen if qid not in by_id]
    if missing:
        raise CorpusError(f'{dataset}: {len(missing)} selected ids are not upstream, e.g. {missing[0]}')
    variant = f'hipporag{len(chosen)}'
    about = f'{dataset}:{variant}'
    refs, entries, questions = {}, [], []
    for qid in chosen:
        row = by_id[qid]
        local = []
        for title, text in passages_of(row):
            if text not in refs:
                refs[text] = f'{about}:p{len(refs):05d}'
                entries.append(CorpusEntry(refs[text], 'passage', f'{title}\n{text}', len(refs),
                                           synthetic_time(len(refs)), {'title': title}))
            local.append(refs[text])
        chain_at, support_at, ordered, tags = gold_of(row)
        chain = []
        for position in chain_at:
            if local[position] not in chain:
                chain.append(local[position])
        answers = sorted({local[position] for position in support_at} | set(chain))
        kind = 'multihop_why_k' if len(chain) >= 2 else 'evidence_recall'
        questions.append(evidence_question(
            identifier=f'{dataset}-{qid}', corpus=dataset, about=about,
            question=row['question'], answers=answers, kind=kind, policy=policy,
            chain=Chain(tuple(chain), ordered) if len(chain) >= 2 else None,
            tags=tags + (f'setup:{variant}',),
            source={'kind': 'dataset', 'dataset': spec['lock'], 'ref': qid, 'rule': 'upstream-support.v1',
                    'answer': str(row.get('answer', ''))}))
    if limit is None and len(entries) != spec['passages']:
        raise CorpusError(f'{dataset}: rebuilt {len(entries)} passages, HippoRAG 2 reports {spec["passages"]}')
    selection = {'dataset': spec['lock'], 'variant': variant, 'questions': len(chosen),
                 'passages': len(entries), 'first_id': chosen[0], 'last_id': chosen[-1]}
    notes = {'policy': policy, 'comparable_with_published': limit is None,
             'types': {kind: sum(1 for q in questions if q.type == kind)
                       for kind in sorted({q.type for q in questions})}}
    return PublicCorpus(f'{dataset}-{variant}', spec['lock'], ADAPTER_VERSION, selection,
                        (CorpusAbout(about, tuple(entries)),), tuple(questions), notes=notes).check()
