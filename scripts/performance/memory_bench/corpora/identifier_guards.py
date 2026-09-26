"""Identifier-structure guards (BENCH_SPEC 4.3) from a copy of the real store.

Each family pairs positives — an identifier the store holds, asked in a form a
user types (`C6.8+C6.9`, `issue 185` for `#185`, `corte 5` for `C5`, `c6-4`) —
with negatives: the same shape around an identifier the store never wrote.
A positive's gold is every entry that writes the identifier in any of the
family's forms; a negative is UNKNOWN with `anchor_not_found`. They are
margin-0 guards for P3 and P4 (type `identifier_guard`).
"""
from dataclasses import dataclass
import re

from ..domain.prng import SplitMix64
from ..domain.question import SCHEMA, Question
from .anchor_forms import Anchor

FAMILIES = ('compound_plus', 'range', 'version', 'hash', 'corte_c', 'issue_hash', 'lower_hyphen')
COMPOUND = re.compile(r'(?i)(?<![\w.])(c\d{1,2}(?:\.\d{1,3})+)([+-])(c\d{1,2}(?:\.\d{1,3})+)(?![\w]|\.\d)')
CORTE = re.compile(r'(?i)\bcorte\s+(\d{1,2})\b')


@dataclass(frozen=True)
class Guard:
    family: str
    positive: bool
    about: str
    surface: str  # the form written in the question
    matches: object  # raw entry text -> bool: the entry writes this identifier
    wide: tuple  # key sets; a negative needs no entry holding any of them

    def key(self):
        return (self.family, not self.positive, self.surface)


def _corte_pattern(n):
    return re.compile(rf'(?i)\bcorte\s+{n}\b|(?<![\w.])C{n}(?!\d)')


def _compound_matcher(members, literal):
    patterns = [m.strict_pattern() for m in members]
    lowered = literal.lower()
    return lambda raw: lowered in raw.lower() or all(p.search(raw) for p in patterns)


def _range_matcher(first, last, literal):
    members = [Anchor('dotted', first.parts[:-1] + (n,)) for n in range(first.parts[-1], last.parts[-1] + 1)]
    patterns = [m.strict_pattern() for m in members]
    lowered = literal.lower()
    return lambda raw: lowered in raw.lower() or any(p.search(raw) for p in patterns)


def _absent_twin(anchor, indexes, about_index):
    for twin in anchor.perturbations():
        if not any(i.strict(twin) for i in indexes.values()) and not about_index.mentions(twin):
            return twin
    return None


def _anchor_guards(about, index, indexes, family, anchors):
    guards = []
    for anchor in anchors:
        twin = _absent_twin(anchor, indexes, index)
        if family == 'issue_hash':
            surface, twin_surface = f'issue {anchor.parts[0]}', twin and f'issue {twin.parts[0]}'
        elif family == 'lower_hyphen':
            surface = f'c{anchor.parts[0]}-' + '-'.join(str(p) for p in anchor.parts[1:])
            twin_surface = twin and f'c{twin.parts[0]}-' + '-'.join(str(p) for p in twin.parts[1:])
        else:
            surface, twin_surface = anchor.display, twin and twin.display
        guards.append(Guard(family, True, about, surface, anchor.strict_pattern().search, anchor.keysets()))
        if twin is not None:
            guards.append(Guard(family, False, about, twin_surface, twin.strict_pattern().search, twin.keysets()))
    return guards


def _compound_guards(about, index, indexes, family):
    seen, guards = set(), []
    for doc in index.docs:
        for left, joiner, right in COMPOUND.findall(doc.raw):
            first, last = Anchor.parse(left.lower()), Anchor.parse(right.lower())
            if not first or not last or first.parts[:-1] != last.parts[:-1] or last.parts[-1] <= first.parts[-1]:
                continue
            is_range = joiner == '-' and last.parts[-1] - first.parts[-1] >= 2
            if (family == 'range') != is_range or (first, last) in seen:
                continue
            seen.add((first, last))
            glue = '-' if family == 'range' else '+'
            literal = f'{first.display}{glue}{last.display}'
            matcher = _range_matcher(first, last, literal) if is_range else _compound_matcher((first, last), literal)
            guards.append(Guard(family, True, about, literal, matcher, first.keysets() + last.keysets()))
            twins = [_absent_twin(a, indexes, index) for a in (first, last)]
            if all(twins) and twins[0].parts[:-1] == twins[1].parts[:-1] and twins[1].parts[-1] > twins[0].parts[-1]:
                twin_literal = f'{twins[0].display}{glue}{twins[1].display}'
                twin_matcher = (_range_matcher(*twins, twin_literal) if is_range
                                else _compound_matcher(twins, twin_literal))
                guards.append(Guard(family, False, about, twin_literal, twin_matcher,
                                    twins[0].keysets() + twins[1].keysets()))
    return guards


def _corte_guards(about, index, indexes):
    numbers = set()
    for doc in index.docs:
        numbers |= {int(n) for n in CORTE.findall(doc.raw)}
    guards = []
    for n in sorted(numbers):
        pattern = _corte_pattern(n).search
        for surface in (f'corte {n}', f'C{n}'):
            guards.append(Guard('corte_c', True, about, surface, pattern, (frozenset({f'c{n}'}),)))
    for n in range(max(numbers, default=0) + 2, 40):
        pattern = _corte_pattern(n).search
        if not any(pattern(d.raw) for i in indexes.values() for d in i.docs) and not index.docs_with_key(f'c{n}'):
            guards += [Guard('corte_c', False, about, f'corte {n}', pattern, (frozenset({f'c{n}'}),)),
                       Guard('corte_c', False, about, f'C{n}', pattern, (frozenset({f'c{n}'}),))]
            break
    return guards


def candidates(context, about):
    index, indexes = context.indexes[about], context.indexes
    anchors = index.anchors()
    by_family = {'hash': [a for a in anchors if a.family == 'hash'],
                 'issue_hash': [a for a in anchors if a.family == 'hash'],
                 'version': [a for a in anchors if a.family == 'version'],
                 'lower_hyphen': [a for a in anchors if a.family == 'dotted']}
    guards = []
    for family in FAMILIES:
        if family in by_family:
            guards += _anchor_guards(about, index, indexes, family, by_family[family])
        elif family in ('compound_plus', 'range'):
            guards += _compound_guards(about, index, indexes, family)
        else:
            guards += _corte_guards(about, index, indexes)
    return guards


def _selected(context, rng):
    rules = context.rules['identifier_guards']
    pinned = [p.lower() for p in rules['pinned']]
    pools = {}
    for about in context.queried:
        for guard in candidates(context, about):
            pools.setdefault((guard.family, guard.positive), []).append(guard)
    chosen = []
    for family in rules['families']:
        for positive in (True, False):
            pool = sorted(pools.get((family, positive), []), key=lambda g: (g.about, g.surface))
            pool = rng.fork(f'guards/{family}/{positive}').shuffled(pool)
            pool.sort(key=lambda g: g.surface.lower() not in pinned)  # stable: pinned first
            chosen += pool[:rules['per_family']]
    return chosen


def _record(guard, identifier, text, answers, language, provenance, policy):
    facet = {'name': 'anchor', 'words': guard.surface, 'answers': answers, 'related': [],
             'answerable_facet': bool(answers)}
    gold = {'answerable': 'KNOWN' if guard.positive else 'UNKNOWN', 'shape': 'singular', 'facets': [facet],
            'wrong_subject': [], 'chain': None, 'path': None, 'current_ref': None, 'stale_refs': [],
            'nearest_outside': None, 'unknown_reason': None if guard.positive else 'anchor_not_found',
            'absent_terms': [] if guard.positive else [guard.surface], 'excluded_refs': [],
            'wake_required': []}
    source = {'kind': 'generator', 'rule': f'identifier_guard.{guard.family}.{provenance["rules_version"]}',
              **{k: v for k, v in provenance.items() if k != 'rules_version'}, 'guard_entries': len(answers)}
    tags = sorted({f'family:{guard.family}', f'polarity:{"positive" if guard.positive else "negative"}',
                   f'lang:{language}'})
    return Question.from_dict({'schema': SCHEMA, 'id': identifier, 'corpus': 'identifier-guards',
                               'type': 'identifier_guard', 'about': guard.about, 'tool': 'kmp_ask',
                               'question': text, 'asked_as': None,
                               'answer_policy': policy, 'as_of': None,
                               'interval': None, 'axis': None, 'dimensions': None, 'arguments': {},
                               'anchors': [guard.surface], 'gold': gold, 'tags': tags, 'private': True,
                               'min_level': None, 'source': source}, identifier)


def _holds(guard, context):
    index = context.indexes[guard.about]
    answers = sorted(d.ref for d in index.docs if d.active and guard.matches(d.own_raw))
    if guard.positive:
        return bool(answers), answers
    written = any(guard.matches(d.raw) for i in context.indexes.values() for d in i.docs)
    wide = any(all(k in d.keys for k in keyset) for keyset in guard.wide for d in index.docs)
    return not written and not wide, []


def generate_guards(context, texts, probe, provenance):
    """(questions, verification rows) for the families of the rules."""
    rng = SplitMix64(context.rules['seed']).fork('identifier_guards')
    templates = context.rules['identifier_guards']['templates']
    questions, rows, counter = [], [], {}
    for guard in _selected(context, rng):
        language = context.language(guard.about)
        template = rng.fork(f'text/{guard.about}/{guard.surface}').choice(templates[language])
        text = template.format(anchor=guard.surface)
        question_terms, surface_terms = probe.probe(texts[guard.about], [text, guard.surface])
        ok_invariant, answers = _holds(guard, context)
        single = ' ' not in guard.surface
        checks = {'surface_keys_kept': surface_terms.search_keys <= question_terms.search_keys,
                  'surface_is_identifier': not single or guard.surface.lower() in question_terms.identifiers,
                  'invariant_holds': ok_invariant}
        ok = all(checks.values())
        identifier = None
        if ok:
            polarity = 'pos' if guard.positive else 'neg'
            counter[(guard.family, polarity)] = counter.get((guard.family, polarity), 0) + 1
            identifier = f'ig-{guard.family.replace("_", "-")}-{polarity}-{counter[(guard.family, polarity)]:03d}'
            questions.append(_record(guard, identifier, text, answers, language, provenance,
                                     context.rules['answer_policy']))
        rows.append({'id': identifier, 'type': 'identifier_guard', 'form': guard.family, 'about': guard.about,
                     'question': text, 'verified': ok, 'kept': ok, 'checks': checks})
    return questions, rows


def summarize(questions):
    counts = {}
    for question in questions:
        tags = dict(tag.split(':', 1) for tag in question.tags)
        name = f'{tags["family"]}/{tags["polarity"]}'
        counts[name] = counts.get(name, 0) + 1
    return dict(sorted(counts.items()))
