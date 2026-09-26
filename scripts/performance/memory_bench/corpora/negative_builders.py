"""Candidate hard negatives (BENCH_SPEC 4.2), one builder per type.

A builder reads the per-about term indexes and returns `Plan`s: what the
question names, which stored words it asks for, and the gold facts that make
it a negative. Nothing here renders text or touches the store; everything is a
function of the indexes, the rules and a counted SplitMix64 stream.
"""
from dataclasses import dataclass, field

from .anchor_forms import Anchor
from .term_index import WORD, fold

FORMS = ('short', 'long')
TYPES = ('anchor_neighbor_existing', 'anchor_absent', 'near_miss_attribute', 'negated_anchor',
         'singular_anchored_twin', 'cross_about_anchor')


@dataclass(frozen=True)
class Plan:
    type: str
    form: str
    about: str
    anchor: Anchor
    attrs: tuple  # stored surface words the question asks for
    attr_keys: tuple  # the search key of each word in the queried about
    related: tuple = ()
    wrong_subject: tuple = ()
    answers: tuple = ()  # negated_anchor: one tuple of refs per attribute
    excluded: tuple = ()
    reason: str | None = None
    absent_terms: tuple = ()
    facts: tuple = ()  # (name, scalar) provenance, stamped into source
    tags: tuple = ()
    twin_keys: tuple = field(default=())  # every key of the twin's attribute group

    def identity(self):
        return (self.type, self.form, self.about, self.anchor, self.attrs)


@dataclass(frozen=True)
class Context:
    rules: object  # NegativeRules
    indexes: dict  # about -> AboutIndex (every about of the store)
    queried: tuple  # abouts questions are asked in
    twin_words: dict  # (about, word) -> frozenset of search keys

    def language(self, about):
        return 'es' if self.indexes[about].language == 'spanish' else 'en'


def template_words(rules, language):
    words = set()
    for value in rules.templates(language).values():
        for text in (value if isinstance(value, list) else [value]):
            words |= {w.lower() for w in WORD.findall(text)}
    return words


def reads_as_thing(word, index, rules):
    """Proper/technical name or noun-suffixed word; verb endings refused (negatives.toml)."""
    if any(word.endswith(suffix) for suffix in rules['verb_suffixes']):
        return False
    return (index.proper_ratio(word) >= rules['proper_ratio']
            or any(word.endswith(suffix) for suffix in rules['noun_suffixes']))


def attribute_pool(context, about, min_df=None):
    """key -> surface word of the stored words eligible as attributes in `about`."""
    rules, index = context.rules['attributes'], context.indexes[about]
    stop = set(rules['stoplist']) | template_words(context.rules, context.language(about))
    stop |= {fold(word) for word in stop}
    lowest = rules['min_df'] if min_df is None else min_df
    highest = max(rules['min_df'], int(rules['max_df_fraction'] * len(index.docs)))
    pool = {}
    for key, word in index.key_word.items():
        if not rules['min_length'] <= len(word) <= rules['max_length'] or word in stop or fold(word) in stop:
            continue
        if not reads_as_thing(word, index, rules):
            continue
        if lowest <= index.df(key) <= highest:
            pool[key] = word
    return pool


def _anchors(context, about):
    index, limit = context.indexes[about], context.rules['anchors']['max_strict_df']
    families = set(context.rules['anchors']['families'])
    return [a for a in index.anchors() if a.family in families and 1 <= len(index.strict(a)) <= limit]


def _zone(index, anchor, hops):
    return index.around(index.mentions(anchor) | index.strict(anchor), hops)


def _pick(rng, keys, pool, index, count):
    """`count` attribute keys, favouring frequent words (harder), drawn with `rng`."""
    ranked = sorted(keys, key=lambda k: (-index.df(k), k))[:2 * count + 4]
    if len(ranked) < count:
        return None
    return tuple(sorted(rng.sample(ranked, count), key=lambda k: pool[k]))


def _count(context, form):
    attributes = context.rules['attributes']
    return 1 if form == 'short' else attributes['long_count']


def _unknown(plan_type, form, about, anchor, keys, pool, **extra):
    words = tuple(pool[k] for k in keys)
    return Plan(plan_type, form, about, anchor, words, keys, absent_terms=extra.pop('absent', words),
                **extra)


def near_miss_attribute(context, about, form, rng):
    index, hops = context.indexes[about], context.rules['attributes']['neighbor_hops']
    pool, plans = attribute_pool(context, about), []
    for anchor in _anchors(context, about):
        zone = _zone(index, anchor, hops)
        blocked = index.keys_of(zone)
        keys = _pick(rng.fork(f'{anchor.folded}'), [k for k in pool if k not in blocked], pool, index,
                     _count(context, form))
        if keys:
            strict = index.strict(anchor)
            plans.append(_unknown('near_miss_attribute', form, about, anchor, keys, pool,
                                  related=tuple(sorted(index.strict_own(anchor))), reason='attribute_not_found',
                                  facts=(('anchor_strict_df', len(strict)),
                                         ('attribute_min_df', min(index.df(k) for k in keys)),
                                         ('cooccurrence', 0), ('zone_size', len(zone)))))
    return plans


def anchor_absent(context, about, form, rng):
    index, pool, plans = context.indexes[about], attribute_pool(context, about), []
    require_unmentioned = context.rules['anchor_absent']['require_keys_unmentioned']
    for anchor in _anchors(context, about):
        for twin in anchor.perturbations():
            if any(other.strict(twin) for other in context.indexes.values()):
                continue
            if require_unmentioned and index.mentions(twin):
                continue
            strict = index.strict(anchor)
            keys = _pick(rng.fork(twin.folded), [k for k in index.keys_of(strict) if k in pool], pool,
                         index, _count(context, form))
            if keys:
                plans.append(_unknown('anchor_absent', form, about, twin, keys, pool,
                                      absent=(twin.display,), wrong_subject=tuple(sorted(index.strict_own(anchor))),
                                      reason='anchor_not_found',
                                      facts=(('anchor_strict_df', 0), ('anchor_mentions_df', 0),
                                             ('original_anchor', anchor.display),
                                             ('original_strict_df', len(strict)))))
            break
    return plans


def anchor_neighbor_existing(context, about, form, rng):
    index, hops = context.indexes[about], context.rules['attributes']['neighbor_hops']
    rules = context.rules['anchor_neighbor_existing']
    pool, plans = attribute_pool(context, about, min_df=1), []
    anchors = _anchors(context, about)
    for anchor in anchors:
        zone = _zone(index, anchor, hops)
        for neighbor in anchors:
            gap = abs(neighbor.parts[-1] - anchor.parts[-1])
            if neighbor.family_prefix() != anchor.family_prefix() or not 1 <= gap <= rules['max_gap']:
                continue
            around_neighbor = index.mentions(neighbor) | index.strict(neighbor)
            eligible = []
            for key in pool:
                docs = index.docs_with_key(key)
                if docs & zone or not docs & index.strict(neighbor):
                    continue
                if rules['strict_only_neighbor'] and not docs <= around_neighbor:
                    continue
                eligible.append(key)
            count = _count(context, form)
            if form == 'long' and len(eligible) < count:
                count = max(context.rules['attributes']['long_min_count'], len(eligible))
            keys = _pick(rng.fork(f'{anchor.folded}>{neighbor.folded}'), eligible, pool, index, count)
            if keys:
                trap = frozenset().union(*(index.docs_with_key(k) for k in keys))
                plans.append(_unknown('anchor_neighbor_existing', form, about, anchor, keys, pool,
                                      related=tuple(sorted(index.strict_own(anchor))),
                                      wrong_subject=tuple(sorted(trap)), reason='attribute_not_found',
                                      facts=(('neighbor_anchor', neighbor.display),
                                             ('anchor_strict_df', len(index.strict(anchor))),
                                             ('neighbor_strict_df', len(index.strict(neighbor))),
                                             ('trap_entries', len(trap)), ('cooccurrence', 0))))
    return plans


def negated_anchor(context, about, form, rng):
    index, rules = context.indexes[about], context.rules['negated_anchor']
    pool, plans = attribute_pool(context, about), []
    for anchor in _anchors(context, about):
        zone = index.mentions(anchor) | index.strict(anchor)
        eligible = {}
        for key in pool:
            docs = index.docs_with_key(key)
            answers = frozenset(r for r in (docs - zone) & index.own_docs_with_key(key) if index.by_ref[r].active)
            if docs & zone and rules['min_answers'] <= len(answers) <= rules['max_answers']:
                eligible[key] = (answers, docs & zone)
        count = 1 if form == 'short' else context.rules['attributes']['long_min_count']
        keys = _pick(rng.fork(anchor.folded), list(eligible), pool, index, count)
        if keys:
            excluded = frozenset().union(*(eligible[k][1] for k in keys))
            answers = tuple(tuple(sorted(eligible[k][0] - excluded)) for k in keys)
            if all(answers):
                plans.append(Plan('negated_anchor', form, about, anchor, tuple(pool[k] for k in keys), keys,
                                  answers=answers, excluded=tuple(sorted(excluded)),
                                  facts=(('excluded_entries', len(excluded)),
                                         ('answer_entries', sum(len(a) for a in answers)))))
    return plans


def singular_anchored_twin(context, about, form, rng):
    index, hops = context.indexes[about], context.rules['attributes']['neighbor_hops']
    groups = context.rules['singular_anchored_twin'][context.language(about)]
    plans = []
    for anchor in _anchors(context, about):
        blocked = index.keys_of(_zone(index, anchor, hops))
        fitting = []
        for group in groups:
            keys = frozenset().union(*(context.twin_words[(about, w)] for w in group))
            if keys and not keys & blocked and context.twin_words[(about, group[0])]:
                fitting.append((group, keys))
        if not fitting:
            continue
        group, keys = rng.fork(anchor.folded).choice(fitting)
        in_about = sum(index.df(k) for k in keys)
        head_key = min(context.twin_words[(about, group[0])])
        plans.append(Plan('singular_anchored_twin', form, about, anchor, (group[0],), (head_key,),
                          related=tuple(sorted(index.strict_own(anchor))), reason='attribute_not_found',
                          absent_terms=(group[0],), twin_keys=tuple(sorted(keys)),
                          tags=('attr_df:present' if in_about else 'attr_df:zero',),
                          facts=(('anchor_strict_df', len(index.strict(anchor))),
                                 ('attribute_about_df', in_about), ('cooccurrence', 0))))
    return plans


def cross_about_anchor(context, about, form, rng):
    """The attribute is a word of the queried about; only the anchor is foreign."""
    queried = context.indexes[about]
    pool, plans = attribute_pool(context, about), []
    word_pool = {word: key for key, word in pool.items()}
    for owner in sorted(context.indexes):
        if owner == about:
            continue
        home = context.indexes[owner]
        for anchor in _anchors(context, owner):
            if queried.mentions(anchor) or queried.strict(anchor):
                continue
            home_words = {home.key_word.get(k) for k in home.keys_of(home.strict(anchor))}
            keys = [word_pool[w] for w in sorted(w for w in home_words if w in word_pool)]
            keys = _pick(rng.fork(f'{owner}|{anchor.folded}'), keys, pool, queried, _count(context, form))
            if keys:
                plans.append(_unknown('cross_about_anchor', form, about, anchor, keys, pool,
                                      absent=(anchor.display,), reason='anchor_in_other_about',
                                      facts=(('owner_about', owner),
                                             ('owner_strict_df', len(home.strict(anchor))),
                                             ('queried_mentions_df', 0))))
    return plans


BUILDERS = {'anchor_neighbor_existing': anchor_neighbor_existing, 'anchor_absent': anchor_absent,
            'near_miss_attribute': near_miss_attribute, 'negated_anchor': negated_anchor,
            'singular_anchored_twin': singular_anchored_twin, 'cross_about_anchor': cross_about_anchor}
