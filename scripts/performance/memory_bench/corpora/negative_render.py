"""Plans to questions: fill a pre-registered template, build the gold, then verify.

Verification runs the rendered question through the kernel's tokenizer (the
search probe, under the queried about's morphology) and re-checks the plan's
invariant against the index: a question the kernel would read differently
from what the plan assumed (a stemmed attribute that no longer matches, an
anchor the kernel does not read as an identifier) is dropped, never kept.
"""
import re

from ..domain.question import SCHEMA, Question
from .term_index import fold

SHORT_WORDS = (2, 5)


def facet_name(word, taken):
    base = re.sub(r'[^a-z0-9_-]+', '-', fold(word)).strip('-')[:60] or 'attribute'
    name, n = base, 2
    while name in taken:
        name, n = f'{base}-{n}', n + 1
    taken.add(name)
    return name


def join_words(words, conjunction):
    words = list(words)
    return words[0] if len(words) == 1 else ', '.join(words[:-1]) + f' {conjunction} ' + words[-1]


def render_text(plan, templates, rng):
    conjunction = templates['and']
    if plan.type == 'negated_anchor':
        template = rng.choice(templates['negated_short' if plan.form == 'short' else 'negated_long'])
    elif plan.type == 'singular_anchored_twin':
        template = rng.choice(templates['short' if plan.form == 'short' else 'twin_long'])
    else:
        template = rng.choice(templates[plan.form])
    return template.format(anchor=plan.anchor.display, attr=plan.attrs[0], excluded=plan.anchor.display,
                           attrs=join_words(plan.attrs, conjunction))


def gold_of(plan):
    taken, facets = set(), []
    for position, word in enumerate(plan.attrs):
        answers = list(plan.answers[position]) if plan.answers else []
        related = [] if answers else list(plan.related)
        facets.append({'name': facet_name(word, taken), 'words': word, 'answers': answers,
                       'related': related, 'answerable_facet': bool(answers)})
    known = plan.type == 'negated_anchor'
    enumerative = len(facets) > 1
    return {'answerable': 'KNOWN' if known else 'UNKNOWN',
            'shape': 'enumerative' if enumerative else 'singular', 'facets': facets,
            'wrong_subject': list(plan.wrong_subject), 'chain': None, 'path': None,
            'current_ref': None, 'stale_refs': [], 'nearest_outside': None,
            'unknown_reason': None if known else plan.reason,
            'absent_terms': [] if known else list(plan.absent_terms),
            'excluded_refs': list(plan.excluded), 'wake_required': []}


def build_question(plan, identifier, text, language, answer_policy, provenance):
    source = {'kind': 'generator', 'rule': f'{plan.type}.{provenance["rules_version"]}', **provenance}
    source.pop('rules_version')
    source.update({name: value for name, value in plan.facts})
    tags = sorted({f'form:{plan.form}', f'lang:{language}', *plan.tags})
    record = {'schema': SCHEMA, 'id': identifier, 'corpus': 'hard-negatives', 'type': plan.type,
              'about': plan.about, 'tool': 'kmp_ask', 'question': text, 'asked_as': None,
              'answer_policy': answer_policy, 'as_of': None, 'interval': None, 'axis': None,
              'dimensions': None, 'arguments': {}, 'anchors': [plan.anchor.display],
              'gold': gold_of(plan), 'tags': tags, 'private': True, 'min_level': None,
              'source': source}
    return Question.from_dict(record, identifier)


def still_negative(plan, context):
    """The plan's defining invariant, re-read from the indexes (BENCH_SPEC 4.2 table)."""
    index = context.indexes[plan.about]
    hops = context.rules['attributes']['neighbor_hops']
    zone = index.around(index.mentions(plan.anchor) | index.strict(plan.anchor), hops)
    keys = set(plan.twin_keys or plan.attr_keys)
    if plan.type in ('near_miss_attribute', 'singular_anchored_twin'):
        return bool(index.strict(plan.anchor)) and not keys & index.keys_of(zone)
    if plan.type == 'anchor_neighbor_existing':
        trap = set().union(*(index.docs_with_key(k) for k in keys))
        return bool(index.strict(plan.anchor)) and bool(trap) and not trap & zone
    if plan.type == 'anchor_absent':
        required = context.rules['anchor_absent']['require_keys_unmentioned']
        unmentioned = not required or not index.mentions(plan.anchor)
        return unmentioned and not any(other.strict(plan.anchor) for other in context.indexes.values())
    if plan.type == 'cross_about_anchor':
        owner = context.indexes[dict(plan.facts)['owner_about']]
        return not index.mentions(plan.anchor) and not index.strict(plan.anchor) and bool(owner.strict(plan.anchor))
    if plan.type == 'negated_anchor':
        excluded = index.mentions(plan.anchor) | index.strict(plan.anchor)
        return (bool(plan.excluded) and all(plan.answers) and set(plan.excluded) <= excluded
                and not any(set(answers) & excluded for answers in plan.answers))
    return False


def verification(plan, text, terms, context):
    """(ok, checks) for one rendered question and the probe's reading of it."""
    words = len(text.split())
    checks = {'anchor_is_identifier': plan.anchor.folded in terms.identifiers,
              'attributes_keep_keys': set(plan.attr_keys) <= terms.search_keys,
              'invariant_holds': still_negative(plan, context),
              'short_has_2_to_5_words': plan.form != 'short' or SHORT_WORDS[0] <= words <= SHORT_WORDS[1]}
    return all(checks.values()), checks
