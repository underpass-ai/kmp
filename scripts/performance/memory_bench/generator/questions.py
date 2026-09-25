"""synth-v1 questions (kmp.bench.question.v1, corpus `synth-v1`) built from one block's facts.

Only question blocks (`blocks.QUESTION_BLOCKS`) ask anything, so the question set of a
level is fixed by the blocks below it and the ladder stays nested. Every question is
validated by `Question.from_dict` before it is kept, and its gold refs all live in its own
block, so `min_level = (k + 1) * block_size`.

Gold is the generator's truth, never "entries that contain the anchor": an anchored
question's answers are the entries whose facet the question asks, its related refs are the
same subject's entries that do not answer, and negatives are UNKNOWN by construction (an
anchor no block renders, or an attribute no entry of the anchor states, at any level).
The world invariants re-check each of those claims on the generated records.
"""
from ..domain.question import Question
from . import bridge, render
from .blocks import QUESTION_BLOCKS, block_stream, hub_degree
from .render import FACET_BY_NAME, FACETS

# Questions per type in a question block of factor 1 (block 0 has factor 2).
QUOTA = {
    'enumerative_anchored': 8, 'singular_anchored': 8, 'anchor_neighbor_existing': 8,
    'anchor_absent': 8, 'near_miss_attribute': 8, 'negated_anchor': 6,
    'singular_anchored_twin': 6, 'cross_about_anchor': 6, 'lookup_exact': 6,
    'rare_identifier': 5, 'paraphrase_zero_overlap': 6, 'crosslang': 6, 'hard_distractor': 3,
    'multihop_why_k': 4, 'path_between': 4, 'path_open': 3, 'current_after_supersession': 3,
    'as_of_historical': 3, 'interval_scoped': 4, 'hub_adjacent': 4, 'multi_about': 4,
    'wake_resume': 3, 'repeat_and_determinism': 3,
}
NEGATIONS = ('excluding', 'except', 'without')
ABOUT_SPREADS = (2, 4, 8, 12)


def gold(answerable, shape=None, facets=(), **extra):
    record = {'answerable': answerable, 'shape': shape, 'facets': list(facets),
              'wrong_subject': [], 'chain': None, 'path': None, 'current_ref': None,
              'stale_refs': [], 'nearest_outside': None, 'unknown_reason': None,
              'absent_terms': [], 'excluded_refs': [], 'wake_required': []}
    record.update(extra)
    for key in ('wrong_subject', 'stale_refs', 'excluded_refs', 'wake_required'):
        record[key] = sorted(set(record[key]))
    return record


def facet(name, words, answers=(), related=()):
    answers = sorted(set(answers))
    return {'name': name, 'words': words, 'answers': answers,
            'related': sorted(set(related) - set(answers)), 'answerable_facet': bool(answers)}


def reader_words(item):
    return item.words[4:] if item.words.startswith('the ') else item.words


class QuestionBuilder:
    def __init__(self, params, topology, block, bridge_table):
        self.params, self.topology, self.block = params, topology, block
        self.bridge_table = bridge_table
        self.k = block.k
        self.factor = QUESTION_BLOCKS[self.k]
        self.rng = block_stream(params.seed, topology.name, self.k).fork('questions')
        self.min_level = (self.k + 1) * params.block_size
        self.out = []
        self.counters = {}
        families = [(group, family) for group in block.groups for family in group.families]
        self.families = families

    def quota(self, kind):
        return QUOTA[kind] * self.factor

    def add(self, kind, about, question, gold_record, tool='kmp_ask', anchors=(), tags=(),
            source=None, **fields):
        number = self.counters.get(kind, 0) + 1
        self.counters[kind] = number
        record = {'schema': 'kmp.bench.question.v1',
                  'id': f'synth{self.params.seed}-{self.topology.name}-b{self.k:03d}-{kind}-{number:02d}',
                  'corpus': 'synth-v1', 'type': kind, 'about': about, 'tool': tool,
                  'question': question,
                  'answer_policy': 'evidence_or_unknown' if tool == 'kmp_ask' else None,
                  'anchors': list(anchors), 'gold': gold_record, 'tags': sorted(set(tags)),
                  'private': False, 'min_level': self.min_level,
                  'source': {'kind': 'generator', 'rule': f'{kind}.v1', **(source or {})}}
        record.update(fields)
        self.out.append(Question.from_dict(record))

    def pick(self, items, count):
        return self.rng.sample(items, min(count, len(items)))

    # --- anchored -----------------------------------------------------------------------

    def enumerative_anchored(self):
        for group, family in self.pick(self.families, self.quota('enumerative_anchored')):
            held = list(family.members)
            others = [f.name for f in FACETS if f.name not in family.members]
            total = 8 + self.rng.below(5)
            asked = held + self.rng.sample(others, max(0, min(len(others), total - len(held))))
            asked = self.rng.shuffled(asked)
            facets = [facet(name, reader_words(FACET_BY_NAME[name]),
                            [family.members[name].ref] if name in family.members else [],
                            self._siblings(group, family, name)) for name in asked]
            words = [reader_words(FACET_BY_NAME[name]) for name in asked]
            text = (f'What do we know about {family.anchor}: '
                    f'{", ".join(words[:-1])} and {words[-1]}?')
            self.add('enumerative_anchored', group.about, text,
                     gold('KNOWN', 'enumerative', facets), anchors=[family.anchor],
                     tags=[f'style:{family.style}', f'facets:{len(asked)}'])

    def _siblings(self, group, family, name):
        return [other.members[name].ref for other in group.families
                if other is not family and name in other.members]

    def _held(self, family):
        return list(family.members)

    def singular_anchored(self, kind='singular_anchored', count=None):
        chosen = []
        for group, family in self.pick(self.families, count or self.quota(kind)):
            name = self.rng.choice(self._held(family))
            item = FACET_BY_NAME[name]
            wrong = self._same_facet_elsewhere(group, name)
            self.add(kind, group.about, item.question.format(A=family.anchor, S=group.subject),
                     gold('KNOWN', 'singular',
                          [facet(name, reader_words(item), [family.members[name].ref],
                                 self._siblings(group, family, name))],
                          wrong_subject=wrong),
                     anchors=[family.anchor], tags=[f'style:{family.style}'])
            chosen.append((group, family, name))
        return chosen

    def _same_facet_elsewhere(self, group, name):
        for other in self.block.groups:
            if other is not group and other.about == group.about:
                for family in other.families:
                    if name in family.members:
                        return [family.members[name].ref]
        return []

    def anchor_neighbor_existing(self):
        cases = []
        for group, family in self.families:
            for sibling in group.families:
                if sibling is family:
                    continue
                for name in sibling.members:
                    if name not in family.members:
                        cases.append((group, family, sibling, name))
        for group, family, sibling, name in self.pick(cases, self.quota('anchor_neighbor_existing')):
            item = FACET_BY_NAME[name]
            self.add('anchor_neighbor_existing', group.about,
                     item.question.format(A=family.anchor, S=group.subject),
                     gold('UNKNOWN', 'singular',
                          [facet(name, reader_words(item), [], [sibling.members[name].ref])],
                          unknown_reason='attribute_not_found', absent_terms=[reader_words(item)]),
                     anchors=[family.anchor],
                     tags=[f'style:{family.style}'],
                     source={'neighbour': sibling.anchor, 'keyword': item.keyword})

    def anchor_absent(self):
        for n, (group, family) in enumerate(self.pick(self.families, self.quota('anchor_absent'))):
            held = self._held(family)
            if n % 2 == 0:
                name = self.rng.choice(held)
                item = FACET_BY_NAME[name]
                text = f'{family.absent} {reader_words(item)}?'
                facets = [facet(name, reader_words(item), [], [family.members[name].ref])]
                shape, form = 'singular', 'short'
            else:
                asked = self.rng.sample(held, min(len(held), 3)) + self.rng.sample(
                    [f.name for f in FACETS if f.name not in family.members], 5)
                words = [reader_words(FACET_BY_NAME[name]) for name in asked]
                text = f'What do we know about {family.absent}: {", ".join(words[:-1])} and {words[-1]}?'
                facets = [facet(name, reader_words(FACET_BY_NAME[name]), [],
                                [family.members[name].ref] if name in family.members else [])
                          for name in asked]
                shape, form = 'enumerative', 'long'
            self.add('anchor_absent', group.about, text,
                     gold('UNKNOWN', shape, facets, unknown_reason='anchor_not_found',
                          absent_terms=[family.absent]),
                     anchors=[family.absent], tags=[f'form:{form}', f'style:{family.style}'],
                     source={'perturbed_from': family.anchor})

    def near_miss_attribute(self):
        present = {}   # about -> [(facet name, value)] stated somewhere in the block
        for group, family in self.families:
            for name, member in family.members.items():
                value = member.truth['value']
                if any(char.isalpha() for char in value) and len(value) >= 4 and ' ' not in value:
                    present.setdefault(group.about, []).append((name, value))
        cases = []
        for group, family in self.families:
            texts = ' '.join(member.text.lower() for member in family.members.values())
            for name, value in present.get(group.about, ()):
                if name not in family.members and value.lower() not in texts:
                    cases.append((group, family, name, value))
        seen = set()
        for group, family, name, value in self.rng.shuffled(cases):
            if len(seen) >= self.quota('near_miss_attribute'):
                break
            if (family.anchor, name) in seen:
                continue
            seen.add((family.anchor, name))
            self.add('near_miss_attribute', group.about,
                     f'What did {family.anchor} decide about {value} for {group.subject}?',
                     gold('UNKNOWN', 'singular',
                          [facet(name, reader_words(FACET_BY_NAME[name]), [],
                                 self._siblings(group, family, name))],
                          unknown_reason='attribute_not_found', absent_terms=[value]),
                     anchors=[family.anchor], tags=[f'style:{family.style}'],
                     source={'attribute': value})

    def negated_anchor(self):
        cases = [(group, family) for group in self.block.groups if len(group.families) > 1
                 for family in group.families]
        for n, (group, excluded) in enumerate(self.pick(cases, self.quota('negated_anchor'))):
            answers = {}
            for family in group.families:
                if family is excluded:
                    continue
                for name, member in family.members.items():
                    answers.setdefault(name, []).append(member.ref)
            excluded_refs = [member.ref for member in excluded.members.values()]
            facets = [facet(name, reader_words(FACET_BY_NAME[name]), refs)
                      for name, refs in sorted(answers.items())]
            negation = NEGATIONS[n % len(NEGATIONS)]
            self.add('negated_anchor', group.about,
                     f'What was decided for {group.subject}, {negation} {excluded.anchor}?',
                     gold('KNOWN', 'enumerative' if len(facets) > 1 else 'singular', facets,
                          excluded_refs=excluded_refs),
                     anchors=[excluded.anchor], tags=[f'negation:{negation}'])

    def singular_anchored_twin(self):
        cases = []
        for group, family in self.families:
            in_group = {name for other in group.families for name in other.members}
            for item in FACETS:
                if item.name not in in_group:
                    cases.append((group, family, item))
        for group, family, item in self.pick(cases, self.quota('singular_anchored_twin')):
            self.add('singular_anchored_twin', group.about,
                     item.question.format(A=family.anchor, S=group.subject),
                     gold('UNKNOWN', 'singular', [facet(item.name, reader_words(item))],
                          unknown_reason='attribute_not_found', absent_terms=[reader_words(item)]),
                     anchors=[family.anchor], tags=[f'style:{family.style}'],
                     source={'keyword': item.keyword})

    def cross_about_anchor(self):
        if self.topology.size == 1:
            return
        for group, family in self.pick(self.families, self.quota('cross_about_anchor')):
            name = self.rng.choice(self._held(family))
            item = FACET_BY_NAME[name]
            others = self.topology.other_abouts(group.about)
            about = others[self.rng.below(len(others))]
            self.add('cross_about_anchor', about,
                     item.question.format(A=family.anchor, S=group.subject),
                     gold('UNKNOWN', 'singular', [facet(name, reader_words(item))],
                          unknown_reason='anchor_in_other_about', absent_terms=[family.anchor]),
                     anchors=[family.anchor], tags=[f'style:{family.style}'],
                     source={'anchor_about': group.about})

    # --- lexical controls ---------------------------------------------------------------

    def lookup_exact(self):
        for group, family in self.pick(self.families, self.quota('lookup_exact')):
            name = self.rng.choice(self._held(family))
            member = family.members[name]
            item = FACET_BY_NAME[name]
            core = render.facet_sentence(item, family.anchor, group.subject, member.truth['value'])
            text = core.replace(member.truth['value'], 'what', 1).rstrip('.') + '?'
            self.add('lookup_exact', group.about, text,
                     gold('KNOWN', 'singular', [facet(name, reader_words(item), [member.ref])]),
                     tags=[f'style:{family.style}'])

    def rare_identifier(self):
        for member, identifier in self.pick(self.block.rare, self.quota('rare_identifier')):
            self.add('rare_identifier', member.about, render.RARE_QUESTION.format(R=identifier),
                     gold('KNOWN', 'singular', [facet('record', 'what it records', [member.ref])]),
                     anchors=[identifier])

    def paraphrase_zero_overlap(self):
        for member, text, combo in self.pick(self.block.paraphrases,
                                             self.quota('paraphrase_zero_overlap')):
            self.add('paraphrase_zero_overlap', member.about, text,
                     gold('KNOWN', 'singular', [facet('paraphrase', text, [member.ref])]),
                     source={'combo': combo})

    def crosslang(self):
        table = self.bridge_table
        for member, text, lang, direction, combo in self.pick(self.block.crosslang,
                                                              self.quota('crosslang')):
            state, bridged = bridge.coverage(table, text, member.text)
            self.add(f'crosslang_{direction}', member.about, text,
                     gold('KNOWN', 'singular', [facet('fact', text, [member.ref])]),
                     tags=[f'bridge:{state}', f'lang:{lang}'],
                     source={'combo': combo, 'bridged_terms': ' '.join(bridged)})

    def hard_distractor(self):
        for target, twin, text in self.pick(self.block.distractors, self.quota('hard_distractor')):
            self.add('hard_distractor', target.about, text,
                     gold('KNOWN', 'singular', [facet('fact', text, [target.ref])],
                          wrong_subject=[twin.ref]))

    # --- chains and paths ---------------------------------------------------------------

    def multihop_why_k(self):
        chains = [chain for chain in self.block.chains if len(chain.nodes) <= 5]
        for chain in self.pick(chains, self.quota('multihop_why_k')):
            refs = [node.ref for node in reversed(chain.nodes)]
            text = f'Why {chain.questions[-1]}?'
            self.add('multihop_why_k', chain.about, text,
                     gold('KNOWN', 'singular', [facet('cause', 'why', refs[1:], refs[:1])],
                          chain={'refs': refs, 'ordered': True}),
                     tags=[f'k:{len(refs)}', f'undeclared:{chain.declared.count(False)}'],
                     source={'chain': chain.name})

    def _path_source(self, chain):
        undeclared = [f'{a.ref}>{b.ref}' for (a, b), declared
                      in zip(zip(chain.nodes, chain.nodes[1:]), chain.declared) if not declared]
        return {'chain': chain.name, 'undeclared_edges': ','.join(undeclared)}

    def _path_gold(self, chain, open_ended):
        refs = [node.ref for node in chain.nodes]
        return gold('KNOWN', path={
            'from': refs[0], 'to': None if open_ended else refs[-1], 'steps': refs,
            'required': refs[1:-1], 'accept_edges': [list(pair) for pair in zip(refs, refs[1:])],
            'accept_alternatives': True, 'directed': False})

    def path_between(self):
        chains = [chain for chain in self.block.chains if len(chain.nodes) >= 3]
        for n, chain in enumerate(self.pick(chains, self.quota('path_between'))):
            refs = [node.ref for node in chain.nodes]
            tool = 'kmp_curate' if n % 2 == 0 else 'kmp_trace'
            arguments = {'from': refs[0], 'to': refs[-1]}
            if tool == 'kmp_curate':
                arguments.update({'mode': 'paths', 'max_hops': 6})
            self.add('path_between', chain.about,
                     f'How does {chain.ids[0]} lead to {chain.ids[-1]}?',
                     self._path_gold(chain, False), tool=tool,
                     tags=[f'hops:{len(refs) - 1}', f'undeclared:{chain.declared.count(False)}'],
                     source=self._path_source(chain), arguments=arguments)

    def path_open(self):
        chains = [chain for chain in self.block.chains if len(chain.nodes) >= 3]
        for chain in self.pick(chains, self.quota('path_open')):
            refs = [node.ref for node in chain.nodes]
            self.add('path_open', chain.about, f'What follows from {chain.ids[0]}?',
                     self._path_gold(chain, True), tool='kmp_curate',
                     tags=[f'hops:{len(refs) - 1}', f'undeclared:{chain.declared.count(False)}'],
                     source=self._path_source(chain),
                     arguments={'mode': 'paths', 'from': refs[0], 'max_hops': 6})

    # --- time ---------------------------------------------------------------------------

    def current_after_supersession(self):
        for story in self.pick(self.block.supersessions, self.quota('current_after_supersession')):
            refs = [version.ref for version in story.versions]
            words = reader_words(story.changing)
            self.add('current_after_supersession', story.about,
                     story.changing.now.format(S=story.subject),
                     gold('KNOWN', 'singular', [facet(story.changing.name, words, refs[-1:])],
                          current_ref=refs[-1], stale_refs=refs[:-1]),
                     tags=[f'supersession:{story.mode}', f'versions:{len(refs)}'])

    def as_of_historical(self):
        for story in self.pick(self.block.supersessions, self.quota('as_of_historical')):
            versions = story.versions
            i = self.rng.below(len(versions) - 1)
            start, end = versions[i].ordinal, versions[i + 1].ordinal
            others = [version.ref for version in versions if version is not versions[i]]
            self.add('as_of_historical', story.about, story.changing.then.format(S=story.subject),
                     gold('KNOWN', 'singular',
                          [facet(story.changing.name, reader_words(story.changing),
                                 [versions[i].ref], others)]),
                     as_of={'time': render.instant(start + (end - start) // 2)},
                     tags=[f'supersession:{story.mode}'],
                     source={'future_refs': ','.join(v.ref for v in versions[i + 1:])})

    def interval_scoped(self):
        stories = self.pick(self.block.supersessions, self.quota('interval_scoped'))
        for n, story in enumerate(stories):
            versions = story.versions
            name, words = story.changing.name, reader_words(story.changing)
            question = story.changing.then.format(S=story.subject)
            first = versions[0].ordinal
            if n % 2 == 1 and first - self.k * self.params.block_size >= 10:
                span = {'start': render.instant(max(0, first - 240)), 'end': render.instant(first)}
                self.add('interval_scoped', story.about, question,
                         gold('UNKNOWN', 'singular', [facet(name, words, [], [versions[0].ref])],
                              unknown_reason='not_in_selection', nearest_outside=versions[0].ref),
                         interval=span, tags=['interval:not-then', f'supersession:{story.mode}'])
                continue
            i = self.rng.below(len(versions))
            start = versions[i].ordinal
            end = versions[i + 1].ordinal if i + 1 < len(versions) else start + 60
            self.add('interval_scoped', story.about, question,
                     gold('KNOWN', 'singular',
                          [facet(name, words, [versions[i].ref],
                                 [v.ref for v in versions if v is not versions[i]])]),
                     interval={'start': render.instant(start), 'end': render.instant(end)},
                     tags=['interval:known', f'supersession:{story.mode}'],
                     source={'future_refs': ','.join(v.ref for v in versions[i + 1:])})

    # --- graph and scope ----------------------------------------------------------------

    def hub_adjacent(self):
        for note, hub_index, text in self.pick(self.block.hub_notes, self.quota('hub_adjacent')):
            degree = hub_degree(hub_index, self.k + 1)
            self.add('hub_adjacent', note.about, text,
                     gold('KNOWN', 'singular', [facet('fact', text, [note.ref])]),
                     tags=[f'degree:{degree}', f'hub:{render.HUBS[hub_index][0]}'])

    def multi_about(self):
        if self.topology.size == 1:
            return
        for n, (group, family) in enumerate(self.pick(self.families, self.quota('multi_about'))):
            spread = ABOUT_SPREADS[n % len(ABOUT_SPREADS)]
            others = self.rng.shuffled(self.topology.other_abouts(group.about))
            abouts = others[:spread - 1] + [group.about]
            name = self.rng.choice(self._held(family))
            item = FACET_BY_NAME[name]
            self.add('multi_about', abouts[0], item.question.format(A=family.anchor, S=group.subject),
                     gold('KNOWN', 'singular',
                          [facet(name, reader_words(item), [family.members[name].ref])]),
                     anchors=[family.anchor], dimensions={'scope': 'abouts', 'abouts': abouts},
                     tags=[f'abouts:{spread}'])

    def wake_resume(self):
        for group in self.pick(self.block.groups, self.quota('wake_resume')):
            required = [next(iter(family.members.values())).ref for family in group.families]
            anchors = ', '.join(family.anchor for family in group.families)
            intent = f'Resume the {group.subject} work: decisions {anchors}.'
            self.add('wake_resume', group.about, intent, gold('KNOWN', wake_required=required),
                     tool='kmp_wake', arguments={'intent': intent})

    def repeat_and_determinism(self, singular):
        for group, family, name in singular[:self.quota('repeat_and_determinism')]:
            item = FACET_BY_NAME[name]
            self.add('repeat_and_determinism', group.about,
                     item.question.format(A=family.anchor, S=group.subject),
                     gold('KNOWN', 'singular',
                          [facet(name, reader_words(item), [family.members[name].ref])]),
                     anchors=[family.anchor])

    def build(self):
        self.enumerative_anchored()
        singular = self.singular_anchored()
        self.anchor_neighbor_existing()
        self.anchor_absent()
        self.near_miss_attribute()
        self.negated_anchor()
        self.singular_anchored_twin()
        self.cross_about_anchor()
        self.lookup_exact()
        self.rare_identifier()
        self.paraphrase_zero_overlap()
        self.crosslang()
        self.hard_distractor()
        self.multihop_why_k()
        self.path_between()
        self.path_open()
        self.current_after_supersession()
        self.as_of_historical()
        self.interval_scoped()
        self.hub_adjacent()
        self.multi_about()
        self.wake_resume()
        self.repeat_and_determinism(singular)
        return self.out


def block_questions(params, topology, block, bridge_table):
    """The questions of `block` (none unless it is a question block)."""
    if block.k not in QUESTION_BLOCKS:
        return []
    return QuestionBuilder(params, topology, block, bridge_table).build()
