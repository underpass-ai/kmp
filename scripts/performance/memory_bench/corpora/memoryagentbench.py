"""MemoryAgentBench FactConsolidation-SH/MH as a recall corpus (BENCH_SPEC 4.6, BT17).

Source: `Conflict_Resolution` of ai-hyz/MemoryAgentBench (MIT, datasets.lock.json),
eight rows: `factconsolidation_{sh,mh}_{6k,32k,64k,262k}`. SH and MH rows of one size
share the same numbered fact list, so one about per size serves both.

Store side (what `kmp_ingest` receives), as the spec demands:
- one fact per entry (`<about>:fact:<serial>`), text verbatim without its number;
- `sequence` = list position (from 1) and `occurred_at` one minute apart, both increasing in
  list order: "later facts override earlier ones" stays readable from order alone;
- **no relation and no supersedes**, nothing derived from the benchmark: the kernel
  has to find the current fact by itself (`test_public_corpora` checks it).

Question side (gold only, never in the store), rule `fc-triples.v1`:
- every fact is parsed into (subject, relation, object) with the dataset's own
  sentence templates (`TEMPLATES`); the effective knowledge base keeps the latest
  fact per (subject, relation);
- SH: the subject is the longest known subject named in the question; gold is the
  effective fact whose object is the answer (`current_ref`), and `stale_refs` the
  earlier facts of the same (subject, relation). With stale facts the question is
  `current_after_supersession`, otherwise `evidence_recall`;
- MH: the shortest chain of effective facts from a subject named in the question
  to the answer (at most `MAX_HOPS`); it must be unique. Gold is the chain
  (`multihop_why_k`, ordered, question side first).
A question whose gold cannot be derived under the rule is dropped and counted in
`notes['dropped']`, never guessed.

Declared variant (`build(..., declared=True)`, corpus `factconsolidation-<size>-declared`):
the same entries plus the `supersedes` a writer following the dataset's instruction ("later
facts override earlier ones") would declare: each parsed fact supersedes the previous fact
of the same (subject, relation) template. The rule reads the fact list only, never the
questions or their answers; it is the (subject, relation) key MemoryAgentBench and
MemStrata use, the key the kernel's write-time proposal (DISENO 4e) will offer a writer to
accept. It measures what ask does once supersession is declared, and does not replace the
undeclared corpus, where the kernel must find the current fact alone.
"""
from collections import defaultdict
import re

from ..domain.gold import Chain
from .parquet import read_columns
from .public_corpus import (CorpusAbout, CorpusEntry, CorpusRelation, PublicCorpus, evidence_question,
                            synthetic_time)
from .public_errors import CorpusError

ADAPTER_VERSION = 'memoryagentbench-fc.v1'
DECLARED_SUFFIX = '-declared'
DATASET = 'memoryagentbench-cr'
PARQUET = 'Conflict_Resolution-00000-of-00001.parquet'
SIZES = ('6k', '32k', '64k', '262k')
VARIANTS = {'sh': 'factconsolidation-sh', 'mh': 'factconsolidation-mh'}
MAX_HOPS = 4
GOLD_RULE = 'fc-triples.v1'
LEAVES = (('context',), ('questions', 'list', 'element'),
          ('answers', 'list', 'element', 'list', 'element'), ('metadata', 'source'),
          ('metadata', 'qa_pair_ids', 'list', 'element'))
FACT_LINE = re.compile(r'(\d+)\. (.+)')
# The dataset's sentence templates (MQuAKE relations), most specific first.
TEMPLATES = tuple((name, re.compile(pattern)) for name, pattern in (
    ('author', r'The author of (?P<s>.+) is (?P<o>.+)'),
    ('producer', r'The company that produced (?P<s>.+) is (?P<o>.+)'),
    ('headquarters', r'The headquarters of (?P<s>.+) is located in the city of (?P<o>.+)'),
    ('capital', r'The capital of (?P<s>.+) is (?P<o>.+)'),
    ('educated_at', r'The univeristy where (?P<s>.+) was educated is (?P<o>.+)'),
    ('official_language', r'The official language of (?P<s>.+) is (?P<o>.+)'),
    ('broadcaster', r'The origianl broadcaster of (?P<s>.+) is (?P<o>.+)'),
    ('head_of_government', r'The name of the current head of the (?P<s>.+) government is (?P<o>.+)'),
    ('ceo', r'The chief executive officer of (?P<s>.+) is (?P<o>.+)'),
    ('chairperson', r'The chairperson of (?P<s>.+) is (?P<o>.+)'),
    ('head_of_state', r'The name of the current head of state in (?P<s>.+) is (?P<o>.+)'),
    ('genre', r'The type of music that (?P<s>.+) plays is (?P<o>.+)'),
    ('head_coach', r'The head coach of (?P<s>.+) is (?P<o>.+)'),
    ('director', r'The director of (?P<s>.+) is (?P<o>.+)'),
    ('original_language', r'The original language of (?P<s>.+) is (?P<o>.+)'),
    ('citizenship', r'(?P<s>.+) is a citizen of (?P<o>.+)'),
    ('religion', r'(?P<s>.+) is affiliated with the religion of (?P<o>.+)'),
    ('sport', r'(?P<s>.+) is associated with the sport of (?P<o>.+)'),
    ('language', r'(?P<s>.+) speaks the language of (?P<o>.+)'),
    ('spouse', r'(?P<s>.+) is married to (?P<o>.+)'),
    ('creator', r'(?P<s>.+) was created by (?P<o>.+)'),
    ('developer', r'(?P<s>.+) was developed by (?P<o>.+)'),
    ('birthplace', r'(?P<s>.+) was born in the city of (?P<o>.+)'),
    ('founder', r'(?P<s>.+) was founded by (?P<o>.+)'),
    ('deathplace', r'(?P<s>.+) died in the city of (?P<o>.+)'),
    ('continent', r'(?P<s>.+) is located in the continent of (?P<o>.+)'),
    ('performer', r'(?P<s>.+) was performed by (?P<o>.+)'),
    ('work_city', r'(?P<s>.+) worked in the city of (?P<o>.+)'),
    ('origin_country', r'(?P<s>.+) was created in the country of (?P<o>.+)'),
    ('employer', r'(?P<s>.+) is employed by (?P<o>.+)'),
    ('famous_for', r'(?P<s>.+) is famous for (?P<o>.+)'),
    ('founded_in', r'(?P<s>.+) was founded in the city of (?P<o>.+)'),
    ('written_in', r'(?P<s>.+) was written in the language of (?P<o>.+)'),
    ('position', r'(?P<s>.+) plays the position of (?P<o>.+)'),
    ('child', r"(?P<s>.+)'s child is (?P<o>.+)"),
    ('field', r'(?P<s>.+) works in the field of (?P<o>.+)'),
    ('office_holder', r'The (?P<s>.+) is (?P<o>.+)'),
))


def parse_facts(context):
    """[(serial, sentence)] of the numbered lines of a context, in list order."""
    facts = []
    for line in context.split('\n'):
        match = FACT_LINE.fullmatch(line.strip())
        if match:
            facts.append((int(match.group(1)), match.group(2).strip()))
    if len(facts) < 2:
        raise CorpusError('a FactConsolidation context has at least two numbered facts')
    serials = [serial for serial, _ in facts]
    if serials != sorted(set(serials)):
        raise CorpusError('fact serials must strictly increase')
    return facts


def triple(sentence):
    """(subject, relation, object) of a fact sentence, or None when no template fits."""
    body = sentence[:-1] if sentence.endswith('.') else sentence
    for name, pattern in TEMPLATES:
        match = pattern.fullmatch(body)
        if match:
            return match.group('s').strip(), name, match.group('o').strip()
    return None


class KnowledgeBase:
    """Parsed facts; `effective` keeps the latest fact per (subject, relation)."""

    def __init__(self, facts):
        self.facts = []  # (serial, subject, relation, object)
        self.unparsed = 0
        for serial, sentence in facts:
            parsed = triple(sentence)
            if parsed is None:
                self.unparsed += 1
                continue
            self.facts.append((serial, *parsed))
        self.history = defaultdict(list)
        for fact in self.facts:
            self.history[(fact[1], fact[2])].append(fact)
        self.effective = {key: rows[-1] for key, rows in self.history.items()}
        self.by_subject = defaultdict(list)
        for fact in self.effective.values():
            self.by_subject[fact[1]].append(fact)
        self.subjects = sorted(self.by_subject, key=len, reverse=True)

    def subjects_in(self, question):
        """Known subjects named in the question (word-bounded), longest first."""
        found = []
        for subject in self.subjects:
            if len(subject) >= 2 and subject in question and re.search(r'(?<!\w)' + re.escape(subject) + r'(?!\w)', question):
                found.append(subject)
        return found


def _same(value, answers):
    return value.casefold() in {answer.casefold() for answer in answers}


def sh_gold(kb, question, answers):
    """(current fact, stale facts) or None: the effective fact of a named subject whose object answers."""
    for subject in kb.subjects_in(question):
        hits = [fact for fact in kb.by_subject[subject] if _same(fact[3], answers)]
        if len(hits) == 1:
            current = hits[0]
            stale = [fact for fact in kb.history[(current[1], current[2])]
                     if fact[0] < current[0] and not _same(fact[3], answers)]
            return current, stale
        if len(hits) > 1:
            return None
    return None


def mh_gold(kb, question, answers, max_hops=MAX_HOPS):
    """The unique shortest chain of effective facts from a named subject to the answer, or None."""
    starts = kb.subjects_in(question)
    frontier = [(subject, ()) for subject in starts]
    for _ in range(max_hops):
        found, following = [], []
        for entity, path in frontier:
            for fact in kb.by_subject.get(entity, ()):
                if fact in path:
                    continue
                step = path + (fact,)
                if _same(fact[3], answers) and len(step) >= 2:
                    found.append(step)
                following.append((fact[3], step))
        if found:
            unique = {tuple(fact[0] for fact in chain) for chain in found}
            return found[0] if len(unique) == 1 else None
        frontier = following
    return None


def fact_ref(about, serial):
    return f'{about}:fact:{serial}'


def load_rows(path):
    columns = read_columns(path, LEAVES)
    rows = []
    for index in range(len(columns[LEAVES[0]])):
        rows.append({'context': columns[LEAVES[0]][index], 'questions': columns[LEAVES[1]][index],
                     'answers': columns[LEAVES[2]][index], 'source': columns[LEAVES[3]][index],
                     'qa_pair_ids': columns[LEAVES[4]][index]})
    return rows


def _row(rows, variant, size):
    name = f'factconsolidation_{variant}_{size}'
    matches = [row for row in rows if row['source'] == name]
    if len(matches) != 1:
        raise CorpusError(f'expected one {name} row, found {len(matches)}')
    return matches[0]


def about_of(size):
    return f'factconsolidation:{size}'


def declared_supersessions(kb, about):
    """(CorpusRelation, ...): each fact supersedes the previous one of its (subject, relation)."""
    relations = []
    for (subject, relation), facts in sorted(kb.history.items()):
        for older, newer in zip(facts, facts[1:]):
            relations.append(CorpusRelation(
                fact_ref(about, newer[0]), fact_ref(about, older[0]), 'supersedes', 'evidential',
                f'A later statement of the {relation} of {subject} replaces the earlier one.',
                'FactConsolidation fact list: later facts override earlier ones.'))
    return tuple(relations)


def build(rows, size, variants=('sh', 'mh'), limit=None, policy='best_effort', declared=False):
    """One PublicCorpus: the `size` fact list and the SH/MH questions over it; with `declared`,
    the writer-declared variant (module docstring)."""
    if size not in SIZES:
        raise CorpusError(f'size {size!r} is not one of {", ".join(SIZES)}')
    about = about_of(size)
    contexts = {_row(rows, variant, size)['context'] for variant in variants}
    if len(contexts) != 1:
        raise CorpusError(f'SH and MH rows of {size} do not share their fact list')
    facts = parse_facts(contexts.pop())
    entries = tuple(CorpusEntry(fact_ref(about, serial), 'fact', sentence, index + 1,
                                synthetic_time(index + 1), {'serial': str(serial)})
                    for index, (serial, sentence) in enumerate(facts))
    kb = KnowledgeBase(facts)
    questions, dropped, kinds = [], {}, {}
    for variant in variants:
        row = _row(rows, variant, size)
        corpus = VARIANTS[variant]
        kept = lost = 0
        for index, (text, answers) in enumerate(zip(row['questions'], row['answers'])):
            if limit is not None and kept >= limit:
                break
            answers = [a for a in (answers or []) if isinstance(a, str) and a.strip()]
            qa_id = row['qa_pair_ids'][index] if index < len(row['qa_pair_ids'] or []) else f'no{index}'
            source = {'kind': 'dataset', 'dataset': DATASET, 'ref': qa_id, 'rule': GOLD_RULE,
                      'answer': ' | '.join(answers)}
            identifier = f'fc-{variant}-{size}-q{index:03d}'
            if variant == 'sh':
                gold = sh_gold(kb, text, answers) if answers else None
                if gold is None:
                    lost += 1
                    continue
                current, stale = gold
                ref = fact_ref(about, current[0])
                stale_refs = [fact_ref(about, fact[0]) for fact in stale]
                kind = 'current_after_supersession' if stale_refs else 'evidence_recall'
                question = evidence_question(
                    identifier=identifier, corpus=corpus, about=about, question=text, answers=[ref],
                    kind=kind, policy=policy, current_ref=ref if stale_refs else None,
                    stale_refs=stale_refs, tags=(f'size:{size}', 'hops:1'), source=source)
            else:
                chain = mh_gold(kb, text, answers) if answers else None
                if chain is None:
                    lost += 1
                    continue
                refs = [fact_ref(about, fact[0]) for fact in chain]
                question = evidence_question(
                    identifier=identifier, corpus=corpus, about=about, question=text, answers=refs,
                    kind='multihop_why_k', policy=policy, chain=Chain(tuple(refs), True),
                    tags=(f'size:{size}', f'hops:{len(refs)}'), source=source)
            kinds[question.type] = kinds.get(question.type, 0) + 1
            questions.append(question)
            kept += 1
        dropped[variant] = lost
    selection = {'dataset': DATASET, 'size': size, 'facts': len(facts)}
    relations = declared_supersessions(kb, about) if declared else ()
    if declared:
        selection['declared'] = 'supersedes by (subject, relation), fact order'
    notes = {'variants': list(variants), 'limit': limit, 'gold_rule': GOLD_RULE, 'policy': policy,
             'unparsed_facts': kb.unparsed, 'dropped': dropped, 'types': dict(sorted(kinds.items())),
             'declared_relations': len(relations)}
    suffix = DECLARED_SUFFIX if declared else ''
    return PublicCorpus(f'factconsolidation-{size}{suffix}', DATASET, ADAPTER_VERSION + suffix, selection,
                        (CorpusAbout(about, entries, relations),), tuple(questions), notes=notes).check()
