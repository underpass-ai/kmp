"""Gold of one question (kmp.bench.question.v1 `gold`): what a reader, not a ranker, says.

Structure only: the rules that tie gold to a question type live in question.py.
Every ref is canonical (what `refs.normalize` returns), and set-like ref lists
are stored sorted so equal gold always serializes to equal bytes.
"""
from dataclasses import dataclass

from .errors import QuestionInvalid
from .fields import Fields
from .refs import is_canonical

ANSWERABLE = ('KNOWN', 'UNKNOWN')
SHAPES = ('singular', 'enumerative')
# Expected reason for an UNKNOWN (section 6 "motivo correcto"). Bench vocabulary:
# BT10 maps whatever reason a binary reports onto it.
UNKNOWN_REASONS = ('anchor_not_found', 'attribute_not_found', 'anchor_in_other_about',
                   'not_in_selection', 'no_evidence')
FACET_NAME = r'[a-z0-9][a-z0-9_-]{0,63}'


def _refs(fields, key, ordered=False):
    values = fields.texts(key, unique=True)
    for ref in values:
        if not is_canonical(ref):
            fields.fail(key, f'{ref!r} is not a canonical ref (no entry:/detail: prefix, no spaces)')
    return values if ordered else tuple(sorted(values))


@dataclass(frozen=True)
class Facet:
    name: str
    answers: tuple = ()
    related: tuple = ()
    answerable_facet: bool = False
    words: str | None = None  # the reader's own words for this facet

    @classmethod
    def from_dict(cls, data, where):
        fields = Fields(data, where, QuestionInvalid)
        facet = cls(fields.text('name', pattern=FACET_NAME), _refs(fields, 'answers'),
                    _refs(fields, 'related'), fields.flag('answerable_facet', None),
                    fields.text('words', required=False))
        fields.done()
        if facet.answerable_facet is None:
            fields.fail('answerable_facet', 'required')
        if facet.answers and not facet.answerable_facet:
            fields.fail('answerable_facet', 'a facet with answers is answerable')
        if set(facet.answers) & set(facet.related):
            fields.fail('related', 'a ref cannot both answer and merely relate to a facet')
        return facet

    def as_dict(self):
        return {'name': self.name, 'words': self.words, 'answers': list(self.answers),
                'related': list(self.related), 'answerable_facet': self.answerable_facet}


@dataclass(frozen=True)
class Chain:
    """multihop_why_k: the evidence chain, question side first; full recovery needs all of it."""
    refs: tuple
    ordered: bool = True

    @classmethod
    def from_dict(cls, data, where):
        fields = Fields(data, where, QuestionInvalid)
        chain = cls(_refs(fields, 'refs', ordered=True), fields.flag('ordered', True))
        fields.done()
        if len(chain.refs) < 2:
            fields.fail('refs', 'a chain has at least two refs')
        return chain

    def as_dict(self):
        return {'refs': list(self.refs), 'ordered': self.ordered}


@dataclass(frozen=True)
class PathGold:
    """path_between / path_open: a reference route plus what else a reader accepts."""
    start: str
    end: str | None  # None: open search (path_open)
    steps: tuple = ()  # reference route, start..end, in order; () when only edges are known
    required: tuple = ()  # refs every accepted route passes through
    accept_edges: tuple = ()  # (from, to) pairs a reader accepts as steps (step_precision)
    accept_alternatives: bool = True
    directed: bool = True

    @classmethod
    def from_dict(cls, data, where):
        fields = Fields(data, where, QuestionInvalid)
        start = fields.text('from')
        end = fields.text('to', required=False)
        steps = _refs(fields, 'steps', ordered=True)
        required = _refs(fields, 'required')
        edges = []
        for index, edge in enumerate(fields.objects('accept_edges')):
            if not (isinstance(edge, list) and len(edge) == 2 and all(is_canonical(r) for r in edge)):
                fields.fail(f'accept_edges[{index}]', 'expected [from_ref, to_ref]')
            edges.append(tuple(edge))
        gold = cls(start, end, steps, required, tuple(sorted(set(edges))),
                   fields.flag('accept_alternatives', True), fields.flag('directed', True))
        fields.done()
        for name, ref in (('from', start), ('to', end)):
            if ref is not None and not is_canonical(ref):
                fields.fail(name, f'{ref!r} is not a canonical ref')
        if steps and (steps[0] != start or end is not None and steps[-1] != end):
            fields.fail('steps', 'the reference route must run from `from` to `to`')
        if not steps and not edges:
            fields.fail('steps', 'give the reference route, accepted edges, or both')
        return gold

    def as_dict(self):
        return {'from': self.start, 'to': self.end, 'steps': list(self.steps),
                'required': list(self.required), 'accept_edges': [list(e) for e in self.accept_edges],
                'accept_alternatives': self.accept_alternatives, 'directed': self.directed}


@dataclass(frozen=True)
class Gold:
    answerable: str
    shape: str | None = None
    facets: tuple = ()
    wrong_subject: tuple = ()
    chain: Chain | None = None
    path: PathGold | None = None
    current_ref: str | None = None
    stale_refs: tuple = ()
    nearest_outside: str | None = None
    unknown_reason: str | None = None
    absent_terms: tuple = ()  # what a PARTIAL's `missing` must name to count as abstention
    excluded_refs: tuple = ()  # negated_anchor: refs that must never be cited
    wake_required: tuple = ()  # wake_resume obligations

    @classmethod
    def from_dict(cls, data, where='gold'):
        fields = Fields(data, where, QuestionInvalid)
        facets = tuple(Facet.from_dict(item, f'{where}.facets[{i}]')
                       for i, item in enumerate(fields.objects('facets')))
        chain = fields.mapping('chain')
        path = fields.mapping('path')
        gold = cls(
            answerable=fields.text('answerable', choices=ANSWERABLE),
            shape=fields.text('shape', required=False, choices=SHAPES), facets=facets,
            wrong_subject=_refs(fields, 'wrong_subject'),
            chain=None if chain is None else Chain.from_dict(chain, where + '.chain'),
            path=None if path is None else PathGold.from_dict(path, where + '.path'),
            current_ref=fields.text('current_ref', required=False),
            stale_refs=_refs(fields, 'stale_refs'),
            nearest_outside=fields.text('nearest_outside', required=False),
            unknown_reason=fields.text('unknown_reason', required=False, choices=UNKNOWN_REASONS),
            absent_terms=fields.texts('absent_terms'), excluded_refs=_refs(fields, 'excluded_refs'),
            wake_required=_refs(fields, 'wake_required'))
        fields.done()
        gold._check_structure(fields)
        return gold

    def answer_refs(self):
        return frozenset(ref for facet in self.facets for ref in facet.answers)

    def _check_structure(self, fields):
        names = [facet.name for facet in self.facets]
        if len(set(names)) != len(names):
            fields.fail('facets', 'duplicate facet name')
        for name, ref in (('current_ref', self.current_ref), ('nearest_outside', self.nearest_outside)):
            if ref is not None and not is_canonical(ref):
                fields.fail(name, f'{ref!r} is not a canonical ref')
        answers = self.answer_refs()
        for name in ('wrong_subject', 'excluded_refs', 'stale_refs'):
            if answers & set(getattr(self, name)):
                fields.fail(name, 'overlaps the refs that answer')
        if self.answerable == 'UNKNOWN':
            if answers or any(facet.answerable_facet for facet in self.facets):
                fields.fail('facets', 'an UNKNOWN question has no answering facet')
        elif self.unknown_reason is not None or self.nearest_outside is not None:
            fields.fail('unknown_reason', 'only an UNKNOWN question has an unknown reason or nearest_outside')
        if self.shape == 'singular' and len(self.facets) != 1:
            fields.fail('facets', 'a singular question asks exactly one facet')
        if self.shape == 'enumerative' and len(self.facets) < 2:
            fields.fail('facets', 'an enumerative question asks at least two facets')

    def as_dict(self):
        return {'answerable': self.answerable, 'shape': self.shape,
                'facets': [facet.as_dict() for facet in self.facets],
                'wrong_subject': list(self.wrong_subject),
                'chain': None if self.chain is None else self.chain.as_dict(),
                'path': None if self.path is None else self.path.as_dict(),
                'current_ref': self.current_ref, 'stale_refs': list(self.stale_refs),
                'nearest_outside': self.nearest_outside, 'unknown_reason': self.unknown_reason,
                'absent_terms': list(self.absent_terms), 'excluded_refs': list(self.excluded_refs),
                'wake_required': list(self.wake_required)}
