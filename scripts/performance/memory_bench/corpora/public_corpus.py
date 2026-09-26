"""What every public-dataset adapter produces (BENCH_SPEC section 4.6, BT17).

A `PublicCorpus` is the store side (abouts holding entries, in load order) and the
question side (`kmp.bench.question.v1` records) of one public benchmark. Adapters
only translate a dataset into this shape; loading it through the binary, caching the
store and running the questions is `application/public_build.py` and
`application/public_run.py`.

Rules every adapter keeps, and that `check()` enforces:

- one dataset record is one entry: a fact, a turn, a passage; its ref is owned by
  the about (`<about>:...`) and canonical;
- inside an about, `sequence` and `occurred_at` strictly increase in load order;
- the store carries text and order only: no relation, no validity window, nothing
  derived from the benchmark's gold (answers, evidence labels, which fact replaced
  which). Gold lives in the questions and never reaches the store.
"""
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone

from ..domain import cachekey
from ..domain.gold import Facet, Gold
from ..domain.question import Question, questions_digest
from ..domain.refs import is_canonical, owned_by
from .public_errors import CorpusError

DIMENSION = {'id': 'record:main', 'kind': 'record'}
DEFAULT_BATCH = 5000
EPOCH = datetime(2026, 1, 1, tzinfo=timezone.utc)


def iso(moment):
    return moment.astimezone(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')


def synthetic_time(sequence, start=EPOCH, step_seconds=60):
    """A strictly increasing clock for datasets without one (one step per record)."""
    return iso(start + timedelta(seconds=step_seconds * sequence))


def slug(text):
    """A ref segment: lowercase ASCII letters, digits, `_`, `.`, `-` (the Rust adapters' rule)."""
    out, previous = [], False
    for char in str(text).strip():
        if char.isascii() and char.isalnum():
            out.append(char.lower())
            previous = False
        elif char in '_.-':
            out.append(char)
            previous = False
        elif not previous:
            out.append('-')
            previous = True
    return ''.join(out).strip('-')


@dataclass(frozen=True)
class CorpusEntry:
    ref: str
    kind: str
    text: str
    sequence: int
    occurred_at: str
    metadata: dict = field(default_factory=dict)  # str -> str, provenance only

    def payload(self):
        coordinate = {'dimension': DIMENSION['kind'], 'scope_id': DIMENSION['id'],
                      'sequence': self.sequence, 'occurred_at': self.occurred_at}
        payload = {'id': self.ref, 'kind': self.kind, 'text': self.text, 'coordinates': [coordinate]}
        if self.metadata:
            payload['metadata'] = dict(self.metadata)
        return payload

    def identity(self):
        return [self.ref, self.kind, self.text, self.sequence, self.occurred_at,
                sorted(self.metadata.items())]


@dataclass(frozen=True)
class CorpusAbout:
    about: str
    entries: tuple


@dataclass(frozen=True)
class PublicCorpus:
    """One loadable store plus its questions.

    `name` names the store (`factconsolidation-32k`, `musique-hipporag200`); `dataset`
    is the lock entry it came from; `selection` says which subset of the store side,
    in words a reader can reproduce (it is part of the store key); `notes` carries
    question-side counts, which never change the store.
    """
    name: str
    dataset: str
    adapter_version: str
    selection: dict
    abouts: tuple
    questions: tuple
    private: bool = False
    notes: dict = field(default_factory=dict)  # question-side counts (dropped, types); not keyed

    def entry_count(self):
        return sum(len(about.entries) for about in self.abouts)

    def refs(self):
        return {entry.ref for about in self.abouts for entry in about.entries}

    def content_digest(self):
        """Digest of what the store is loaded with (order included), independent of questions."""
        return cachekey.digest({'abouts': [[about.about, [e.identity() for e in about.entries]]
                                           for about in self.abouts]})

    def selection_digest(self):
        return cachekey.digest({'selection': self.selection, 'content': self.content_digest()})

    def questions_digest(self):
        return questions_digest(self.questions)

    def check(self):
        """Structural promises of section 4.6; raises CorpusError on the first broken one."""
        seen = set()
        for about in self.abouts:
            last_sequence, last_time = 0, ''
            for entry in about.entries:
                if not is_canonical(entry.ref) or not owned_by(entry.ref, about.about):
                    raise CorpusError(f'{entry.ref!r} is not a canonical ref owned by {about.about}')
                if entry.ref in seen:
                    raise CorpusError(f'duplicate ref {entry.ref}')
                seen.add(entry.ref)
                if entry.sequence <= last_sequence or entry.occurred_at <= last_time:
                    raise CorpusError(f'{entry.ref}: sequence/occurred_at must strictly increase')
                last_sequence, last_time = entry.sequence, entry.occurred_at
                if not entry.text.strip():
                    raise CorpusError(f'{entry.ref}: empty text')
        abouts = {about.about for about in self.abouts}
        for question in self.questions:
            if question.about not in abouts:
                raise CorpusError(f'{question.id} asks {question.about}, which the corpus does not load')
            gold = question.gold
            named = set(gold.answer_refs()) | set(gold.stale_refs) | set(gold.chain.refs if gold.chain else ())
            missing = sorted(named - seen)
            if missing:
                raise CorpusError(f'{question.id}: gold names refs the store does not hold: {missing[:3]}')
            if question.private != self.private:
                raise CorpusError(f'{question.id}: private flag differs from its corpus')
        questions_digest(self.questions)
        return self


def ingest_calls(corpus, batch_size=DEFAULT_BATCH, key_prefix=None):
    """[(about, kmp_ingest arguments)]: entries only, in load order, `batch_size` per call."""
    if batch_size < 1:
        raise CorpusError('batch_size must be positive')
    prefix = key_prefix or f'{corpus.name}:{corpus.selection_digest()[:16]}'
    calls = []
    for about in corpus.abouts:
        for index, start in enumerate(range(0, len(about.entries), batch_size)):
            batch = about.entries[start:start + batch_size]
            calls.append((about.about, {
                'about': about.about,
                'idempotency_key': f'{prefix}:{about.about}:B{batch_size}:{index}',
                'memory': {'dimensions': [dict(DIMENSION)] if index == 0 else [],
                           'entries': [entry.payload() for entry in batch],
                           'relations': []}}))
    return calls


def evidence_question(*, identifier, corpus, about, question, answers, kind='evidence_recall',
                      policy='best_effort', chain=None, current_ref=None, stale_refs=(),
                      tags=(), private=False, source=None, words=None):
    """A kmp.bench.question.v1 record for a recall question: one facet `evidence`.

    With no `answers` the question is UNKNOWN (an abstention item, `no_evidence`).
    """
    known = bool(answers)
    facet = Facet('evidence', tuple(sorted(set(answers))), (), known, words)
    gold = Gold('KNOWN' if known else 'UNKNOWN', 'singular', (facet,),
                chain=chain, current_ref=current_ref, stale_refs=tuple(sorted(set(stale_refs))),
                unknown_reason=None if known else 'no_evidence')
    record = Question(id=identifier, corpus=corpus, type=kind, about=about, tool='kmp_ask',
                      gold=gold, question=question, answer_policy=policy, arguments={},
                      tags=tuple(sorted(set(tags))), private=private, source=source)
    # Round-trip through the strict reader: every contract rule applies to adapter output.
    return Question.from_dict(record.as_dict(), identifier)
