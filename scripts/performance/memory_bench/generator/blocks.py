"""One synth-v1 block: 1000 entries, their relations and writes, and the facts questions need.

A block is a pure function of `(seed, topology, k)`: its stream is
`SplitMix64(derive_seed(seed, 'synth-v1/<topology>/block/<k>'))`, forked by purpose so a
change in one part never shifts the draws of another. Nothing in block k reads another
block's output: hubs sit at fixed ordinals of block 0 (0..3), hub degrees follow a fixed
schedule, and every identifier a block writes lives in a namespace derived from k.

Episodes a block holds (per 1000 entries; counts in `Composition`):

- **anchor groups**: a subject with 2-3 families, each family one anchor (`C6.4`, `#10052`,
  `v0.7.2`) and 3-7 facet entries; families of one group share their subject and about.
- **chains**: causal/motivational chains of 2-6 nodes (1-5 hops). Each hop is declared as
  a relation with probability 7/10; an undeclared hop is only written in the text (the
  later node names the earlier node's id), which is what a proposer must find.
- **supersessions**: 2-4 dated versions of one attribute, in three modes: `declared`
  (a `supersedes` relation), `valid_until` (only the old coordinate's validity end),
  `date_only` (neither: only the dates say which is current).
- **hubs** (block 0 only, ordinals 0..3) and **hub notes** that lean on them; degrees grow
  with the level, which is what `hub_adjacent` measures latency against.
- **distractor pairs**, **rare-identifier notes**, and, in question blocks only,
  **paraphrase** and **Spanish/English** entries.
- **filler notes**: the rest, some linked in short `follows` runs, most isolated.
"""
from dataclasses import dataclass, field

from ..domain.prng import SplitMix64, derive_seed
from . import render
from .render import FACETS

GENERATOR = 'synth-v1'
# Blocks whose questions are generated, and how many of each type (factor over the base
# quota in questions.py). Block 0 is the dense question block of level 10^3; one block
# inside each further decade adds questions whose gold sits deep in the ladder.
QUESTION_BLOCKS = {0: 2, 5: 1, 50: 1, 500: 1}
HUB_COUNT = len(render.HUBS)


@dataclass(frozen=True)
class Composition:
    groups: int = 22
    chains: int = 10
    supersessions: int = 9
    distractor_pairs: int = 6
    hub_notes: int = 32
    rare_notes: int = 10
    paraphrases: int = 12      # question blocks only
    crosslang: int = 12        # question blocks only
    echoes: int = 2            # multi only: entries linked across abouts by a write
    grouped_family_relations: int = 1  # of 2 families carry relations
    filler_linked_percent: int = 37    # filler notes that sit in a `follows` run


COMPOSITION = Composition()
CHAIN_LENGTHS = (2, 3, 4, 5, 6)
SUPERSESSION_MODES = ('declared', 'valid_until', 'date_only')
# Relation types inside filler runs, weighted toward the real store's mix
# (uses_background 132, follows 68, verified_by 26, answers 23 of 308).
FILLER_RELATIONS = ((('uses_background', 'evidential'), 5), (('follows', 'procedural'), 2),
                    (('answers', 'evidential'), 1), (('verified_by', 'evidential'), 1))
FILLER_KINDS = (('observation', 52), ('success_path', 22), ('instruction', 8),
                ('error_path', 5), ('reference', 5), ('concept', 3), ('decision', 5))


def block_stream(seed, topology, k):
    return SplitMix64(derive_seed(seed, f'{GENERATOR}/{topology}/block/{k}'))


def weighted(rng, pairs):
    total = sum(weight for _, weight in pairs)
    point = rng.below(total)
    for item, weight in pairs:
        if point < weight:
            return item
        point -= weight
    raise AssertionError('unreachable')


@dataclass
class Member:
    """A planned entry: everything but its ordinal and final text."""
    key: int
    about: str
    kind: str
    episode: str
    core: object              # str, or callable(minutes) -> str for dated sentences
    truth: dict
    pad: bool = True
    ordinal: int | None = None
    text: str | None = None
    valid_until_of: int | None = None  # key of the successor whose start ends this entry

    @property
    def ref(self):
        return f'{self.about}:e{self.ordinal:07d}'


@dataclass(frozen=True)
class PlannedRelation:
    source: int   # member key
    target: int   # member key
    rel: str
    cls: str
    why: str
    evidence: str


@dataclass(frozen=True)
class ResolvedRelation:
    about: str
    source: str   # ref
    target: str   # ref
    later: int    # the later endpoint's ordinal: the relation travels with it
    rel: str
    cls: str
    why: str
    evidence: str


def hub_ref(topology, hub_index):
    """Hubs sit at ordinals 0..3 of block 0, so any block can name them."""
    return f'{topology.hub_about(hub_index)}:e{hub_index:07d}'


def hub_degree(hub_index, blocks, composition=COMPOSITION):
    """Relations touching hub `hub_index` in the first `blocks` blocks (fixed schedule)."""
    per_block = sum(1 for j in range(composition.hub_notes) if j % HUB_COUNT == hub_index)
    return per_block * blocks


@dataclass
class Family:
    anchor: str
    absent: str
    style: str
    index: int
    members: dict = field(default_factory=dict)  # facet name -> Member


@dataclass
class Group:
    index: int                # global group index
    about: str
    subject: str
    style: str
    families: list = field(default_factory=list)


@dataclass
class Chain:
    name: str
    about: str
    host: str
    nodes: list               # Members, root (earliest) first
    ids: list                 # node ids (INC-...), root first
    declared: list            # declared[i]: hop nodes[i] -> nodes[i+1] is a relation
    questions: list           # (statement, question form) per node


@dataclass
class Supersession:
    about: str
    subject: str
    changing: object          # render.Changing
    mode: str
    versions: list            # Members, oldest first


@dataclass
class Block:
    k: int
    members: list                                   # by ordinal after placement
    relations: list                                 # ResolvedRelation, by later endpoint
    echoes: list = field(default_factory=list)      # (echo Member, target Member)
    groups: list = field(default_factory=list)
    chains: list = field(default_factory=list)
    supersessions: list = field(default_factory=list)
    hub_notes: list = field(default_factory=list)   # (Member, hub index, question)
    hubs: list = field(default_factory=list)        # hub Members (block 0 only)
    distractors: list = field(default_factory=list)  # (target, distractor, question)
    rare: list = field(default_factory=list)        # (Member, identifier)
    paraphrases: list = field(default_factory=list)  # (Member, question text, combo)
    crosslang: list = field(default_factory=list)    # (Member, question, lang, direction, combo)


class BlockBuilder:
    def __init__(self, seed, topology, k, block_size, reference_lengths, composition=COMPOSITION):
        self.seed, self.topology, self.k, self.block_size = seed, topology, k, block_size
        self.reference_lengths = reference_lengths
        self.composition = composition
        root = block_stream(seed, topology.name, k)
        self.plan = root.fork('plan')
        self.text = root.fork('text')
        self.lengths = root.fork('lengths')
        self.slots = root.fork('slots')
        self.members, self.relations, self.hub_links = [], [], []
        self.block = Block(k=k, members=[], relations=[])
        self.ordered = []   # member-key lists whose ordinals must follow list order

    # --- planning ------------------------------------------------------------------------

    def member(self, about, kind, episode, core, truth, pad=True):
        truth = {'subject': None, 'attribute': None, 'value': None, 'anchors': [],
                 'chain': None, 'lang': 'en', 'paraphrase_of': None, 'supersedes': None,
                 'episode': episode, 'group': None, **truth}
        item = Member(len(self.members), about, kind, episode, core, truth, pad)
        self.members.append(item)
        return item

    def relate(self, source, target, rel, cls, why, evidence):
        self.relations.append(PlannedRelation(source.key, target.key, rel, cls, why, evidence))

    def build(self):
        k, c = self.k, self.composition
        if k == 0:
            self._hubs()
        for j in range(c.groups):
            self._group(j)
        for j in range(c.chains):
            self._chain(j)
        for j in range(c.supersessions):
            self._supersession(j)
        for j in range(c.distractor_pairs):
            self._distractor(j)
        for j in range(c.hub_notes):
            self._hub_note(j)
        for j in range(c.rare_notes):
            self._rare(j)
        if k in QUESTION_BLOCKS:
            self._paraphrases()
            self._crosslang()
        if self.topology.size > 1:
            for j in range(min(c.echoes, len(self.block.chains))):
                self._echo(j)
        self._filler(self.block_size - len(self.members))
        self._place()
        self._render()
        self.block.members = sorted(self.members, key=lambda m: m.ordinal)
        self.block.relations = self._resolved_relations()
        return self.block

    def _resolved_relations(self):
        """Relations as `(later ordinal, record)` with refs, sorted by the later endpoint."""
        out = []
        for planned in self.relations:
            source, target = self.members[planned.source], self.members[planned.target]
            out.append(ResolvedRelation(source.about, source.ref, target.ref,
                                        max(source.ordinal, target.ordinal), planned.rel,
                                        planned.cls, planned.why, planned.evidence))
        for note, hub_index in self.hub_links:
            hub_id = render.HUBS[hub_index][0]
            out.append(ResolvedRelation(note.about, note.ref, hub_ref(self.topology, hub_index),
                                        note.ordinal, 'uses_background', 'evidential',
                                        f'{note.truth["subject"]} depends on the shared {hub_id}.',
                                        f'Architecture notes of {note.truth["subject"]}.'))
        out.sort(key=lambda relation: (relation.later, relation.source, relation.target))
        return out

    def _hubs(self):
        for index, (hub_id, sentence) in enumerate(render.HUBS):
            hub = self.member(self.topology.hub_about(index), 'reference', 'hub', sentence,
                              {'subject': hub_id, 'attribute': 'hub'})
            hub.ordinal = index   # fixed: later blocks link here without reading block 0
            self.block.hubs.append(hub)

    def _group(self, j):
        rng = self.plan
        index = self.k * 100 + j
        about = self.topology.draw_about(rng)
        style = render.ANCHOR_STYLES[j % len(render.ANCHOR_STYLES)]
        group = Group(index, about, f'svc-{index:03d}', style)
        families = 2 + rng.below(2)
        facets = rng.shuffled(range(len(FACETS)))
        cursor = 0
        for f in range(families):
            size = 3 + rng.below(5)
            family = Family(render.anchor(style, index, f), render.absent_anchor(style, index, f),
                            style, f)
            # Later families repeat one facet of an earlier sibling (the same attribute of the
            # same subject under another anchor: the neighbour case); the rest are new.
            fresh = facets[cursor:cursor + size - (1 if f else 0)]
            cursor += len(fresh)
            chosen = ([facets[rng.below(cursor - len(fresh))]] if f else []) + fresh
            for facet_index in chosen:
                facet = FACETS[facet_index]
                value = rng.choice(facet.values)
                entry = self.member(about, facet.kind, 'family',
                                    render.facet_sentence(facet, family.anchor, group.subject, value),
                                    {'subject': group.subject, 'attribute': facet.name,
                                     'value': value, 'anchors': [family.anchor],
                                     'group': f'g{index}-f{f}'})
                family.members[facet.name] = entry
            group.families.append(family)
            if rng.below(2) < self.composition.grouped_family_relations:
                self._family_relations(group, family)
        if len(group.families) > 1 and rng.below(2) == 0:
            first, second = (next(iter(fam.members.values())) for fam in group.families[:2])
            self.relate(second, first, 'follows', 'procedural',
                        f'The second decision record for {group.subject} follows the first.',
                        f'Both records are part of the {group.subject} review.')
        self.block.groups.append(group)

    def _family_relations(self, group, family):
        entries = list(family.members.values())
        root = entries[0]
        for entry in entries[1:]:
            rel, cls = weighted(self.plan, ((('uses_background', 'evidential'), 5),
                                            (('follows', 'procedural'), 2),
                                            (('answers', 'evidential'), 1),
                                            (('verified_by', 'evidential'), 1)))
            self.relate(entry, root, rel, cls,
                        f'Recorded in the same decision record for {group.subject}.',
                        f'Written during the same review of {group.subject}.')

    def _chain(self, j):
        rng = self.plan
        about = self.topology.draw_about(rng)
        length = CHAIN_LENGTHS[j % len(CHAIN_LENGTHS)]
        host = f'db-{self.k * 100 + j:03d}'
        prefix = render.CHAIN_ID_PREFIXES[j % len(render.CHAIN_ID_PREFIXES)]
        ids = [f'{prefix}-{200000 + self.k * 100 + j * 10 + i}' for i in range(length)]
        events = rng.sample(range(len(render.CHAIN_EVENTS)), length)
        name = f'b{self.k}-c{j}'
        nodes, declared, forms = [], [], []
        for i in range(length):
            decision = i > 0 and rng.below(3) == 0
            pool = render.CHAIN_DECISIONS if decision else render.CHAIN_EVENTS
            statement, form = pool[rng.below(len(pool))] if decision else pool[events[i]]
            hop_declared = i > 0 and rng.below(10) < 7
            mention = i > 0 and (not hop_declared or rng.below(2) == 0)
            link = render.CHAIN_LINKS[rng.below(len(render.CHAIN_LINKS))]
            text = render.chain_sentence(ids[i], statement, host,
                                         ids[i - 1] if mention else None, link)
            node = self.member(about, 'decision' if decision else 'observation', 'chain', text,
                               {'subject': host, 'attribute': 'event', 'value': ids[i],
                                'chain': {'id': name, 'position': i, 'length': length,
                                          'declared_prev': hop_declared,
                                          'mentions_prev': mention}})
            if hop_declared:
                previous = nodes[-1]
                if decision:
                    self.relate(node, previous, 'chosen_because', 'motivational',
                                f'{ids[i]} was decided because of {ids[i - 1]}.',
                                f'Timeline of {host}.')
                else:
                    self.relate(previous, node, 'triggers', 'causal',
                                f'{ids[i - 1]} led to {ids[i]}.', f'Timeline of {host}.')
            if i > 0:
                declared.append(hop_declared)
            nodes.append(node)
            forms.append(form.format(H=host))
        self.ordered.append([node.key for node in nodes])
        self.block.chains.append(Chain(name, about, host, nodes, ids, declared, forms))

    def _supersession(self, j):
        rng = self.plan
        about = self.topology.draw_about(rng)
        changing = render.CHANGING[j % len(render.CHANGING)]
        mode = SUPERSESSION_MODES[j % len(SUPERSESSION_MODES)]
        subject = f'pool-{self.k * 100 + j:03d}'
        count = 2 + rng.below(3)
        values = rng.sample(changing.values, count)
        versions = []
        for i, value in enumerate(values):
            template = changing.statement
            core = (lambda minutes, t=template, v=value:
                    t.format(D=render.stamp_text(minutes), S=subject, V=v))
            version = self.member(about, 'decision', f'supersession-{mode}', core,
                                  {'subject': subject, 'attribute': changing.name, 'value': value})
            if versions:
                version.truth['supersedes'] = versions[-1].key  # resolved to a ref in _render
                if mode == 'declared':
                    self.relate(version, versions[-1], 'supersedes', 'evidential',
                                f'The newer statement replaces the older one for {subject}.',
                                f'Dated statements about {subject}.')
                if mode == 'valid_until':
                    versions[-1].valid_until_of = version.key
            versions.append(version)
        self.ordered.append([version.key for version in versions])
        self.block.supersessions.append(Supersession(about, subject, changing, mode, versions))

    def _distractor(self, j):
        rng = self.plan
        about = self.topology.draw_about(rng)
        template, question, values = render.DISTRACTORS[j % len(render.DISTRACTORS)]
        first, second = rng.sample(values, 2)
        base = self.k * 100 + 2 * j
        subjects = (f'app-{base:03d}', f'app-{base + 1:03d}')
        target = self.member(about, 'observation', 'distractor',
                             template.format(S=subjects[0], V=first),
                             {'subject': subjects[0], 'attribute': 'distractor', 'value': first})
        twin = self.member(about, 'observation', 'distractor',
                           template.format(S=subjects[1], V=second),
                           {'subject': subjects[1], 'attribute': 'distractor', 'value': second})
        self.block.distractors.append((target, twin, question.format(S=subjects[0])))

    def _hub_note(self, j):
        hub_index = j % HUB_COUNT
        hub_id = render.HUBS[hub_index][0]
        about = self.topology.hub_about(hub_index)
        subject = f'web-{self.k * 100 + j:03d}'
        template, question = render.HUB_NOTES[hub_index]
        word = render.pseudo_word(render.zipf_rank(self.plan))
        note = self.member(about, 'observation', 'hub_note',
                           template.format(S=subject, P=word, H=hub_id),
                           {'subject': subject, 'attribute': 'hub', 'value': hub_id})
        self.hub_links.append((note, hub_index))
        self.block.hub_notes.append((note, hub_index, question.format(S=subject, H=hub_id)))

    def _rare(self, j):
        rng = self.plan
        about = self.topology.draw_about(rng)
        identifier = f'RFC-{300000 + self.k * 100 + j}'
        template = render.RARE_NOTES[j % len(render.RARE_NOTES)]
        text = template.format(R=identifier, name=rng.choice(render.NAMES),
                               other=rng.choice(render.NAMES),
                               p=render.pseudo_word(render.zipf_rank(rng)))
        note = self.member(about, 'reference', 'rare', text,
                           {'subject': identifier, 'attribute': 'rare', 'value': identifier})
        self.block.rare.append((note, identifier))

    def _combos(self, label, rows, cols, count):
        """`count` (row, col) pairs no other question block uses: a slice of one permutation."""
        order = SplitMix64(derive_seed(self.seed, f'{GENERATOR}/{label}')).shuffled(
            [(r, c) for r in range(rows) for c in range(cols)])
        slot = sorted(QUESTION_BLOCKS).index(self.k)
        return order[slot * count:(slot + 1) * count]

    def _paraphrases(self):
        subjects, predicates = render.paraphrase_table()
        combos = self._combos('paraphrase', len(subjects), len(predicates),
                              self.composition.paraphrases)
        for n, (s, p) in enumerate(combos):
            about = self.topology.draw_about(self.plan)
            value = render.PARAPHRASE_VALUES[n % len(render.PARAPHRASE_VALUES)]
            combo = f'{subjects[s]["id"]}+{predicates[p]["id"]}'
            entry = self.member(about, 'observation', 'paraphrase',
                                render.paraphrase_entry(subjects[s], predicates[p], value),
                                {'subject': subjects[s]['id'], 'attribute': predicates[p]['id'],
                                 'value': value, 'paraphrase_of': combo}, pad=False)
            self.block.paraphrases.append(
                (entry, render.paraphrase_question(subjects[s], predicates[p]), combo))

    def _crosslang(self):
        subjects, facts = render.lexicon_table()
        combos = self._combos('crosslang', len(subjects), len(facts), self.composition.crosslang)
        for n, (s, f) in enumerate(combos):
            about = self.topology.draw_about(self.plan)
            direction = 'es_en' if n % 2 == 0 else 'en_es'
            stored, asked = ('en', 'es') if direction == 'es_en' else ('es', 'en')
            value = render.CROSSLANG_VALUES[n % len(render.CROSSLANG_VALUES)]
            combo = f'{subjects[s]["id"]}+{facts[f]["id"]}'
            entry = self.member(about, 'observation', 'crosslang',
                                render.crosslang_entry(subjects[s], facts[f], stored, value),
                                {'subject': subjects[s]['id'], 'attribute': facts[f]['id'],
                                 'value': value, 'lang': stored}, pad=False)
            self.block.crosslang.append(
                (entry, render.crosslang_question(subjects[s], facts[f], asked), asked,
                 direction, combo))

    def _echo(self, j):
        chain = self.block.chains[j]
        others = self.topology.other_abouts(chain.about)
        about = others[self.plan.below(len(others))]
        word = render.pseudo_word(render.zipf_rank(self.plan))
        echo = self.member(about, 'observation', 'echo',
                           f'{chain.ids[0]} also delayed the {word} report here.',
                           {'subject': chain.ids[0], 'attribute': 'echo', 'value': chain.about})
        self.block.echoes.append((echo, chain.nodes[0]))

    def _filler(self, count):
        if count < 0:
            raise ValueError(f'block {self.k}: the composition plans more than '
                             f'{self.block_size} entries')
        rng = self.plan
        linked = count * self.composition.filler_linked_percent // 100
        run = []
        for n in range(count):
            about = run[-1].about if run else self.topology.draw_about(rng)
            kind = weighted(rng, FILLER_KINDS)
            note = self.member(about, kind, 'filler', None, {'attribute': 'note'})
            if n < linked:
                if run:
                    rel, cls = weighted(rng, FILLER_RELATIONS)
                    self.relate(note, run[-1], rel, cls,
                                'Written right after the previous note, on the same topic.',
                                'Same working session.')
                run.append(note)
                if len(run) >= 2 + rng.below(3):
                    self.ordered.append([member.key for member in run])
                    run = []
            else:
                if run:
                    self.ordered.append([member.key for member in run])
                    run = []
        if run:
            self.ordered.append([member.key for member in run])

    # --- placement and rendering --------------------------------------------------------

    def _place(self):
        """Ordinals: a shuffled permutation of the block, then every ordered episode sorted."""
        base = self.k * self.block_size
        fixed = {m.ordinal for m in self.members if m.ordinal is not None}
        free = [base + i for i in range(self.block_size) if base + i not in fixed]
        slots = iter(self.slots.shuffled(free))
        for item in self.members:
            if item.ordinal is None:
                item.ordinal = next(slots)
        by_key = self.members
        for keys in self.ordered:
            ordinals = sorted(by_key[key].ordinal for key in keys)
            for key, ordinal in zip(keys, ordinals):
                by_key[key].ordinal = ordinal

    def _render(self):
        for item in self.members:
            minutes = item.ordinal
            core = item.core(minutes) if callable(item.core) else item.core
            if core is None:
                core = render.filler_sentence(self.text, minutes)
            target = render.sample_length(self.lengths, self.reference_lengths)
            item.text = render.pad(self.text, core, target, minutes) if item.pad else core
        for item in self.members:
            if isinstance(item.truth.get('supersedes'), int):
                item.truth['supersedes'] = self.members[item.truth['supersedes']].ref
