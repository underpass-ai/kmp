"""The generator's oracle (BENCH_SPEC section 10): invariants checked on every world.

Each check reads the generated records, never the generator's internals, so a bug in a
block builder shows up here as a failed invariant rather than as quietly wrong gold.

| invariant | what holds |
|---|---|
| `refs_about_prefix` | every entry ref is `<about>:e<ordinal>`; relations stay inside their about; writes name their own about; every gold ref is an entry of the world, below the question's `min_level` |
| `unknown_has_no_fact` | an UNKNOWN question's missing fact is stated by no entry at the world's highest level: a perturbed anchor appears nowhere, an anchor never shares an entry with the attribute asked, a cross-about anchor is absent from the asked about, and a not-then interval holds no version of the fact |
| `declared_chains_exist` | every hop of a chain question that the source does not list as undeclared is a relation (either direction), and every listed one is not |
| `valid_until_coherent` | a `valid_until` is after its `valid_from` and equals the start of the version that replaces it; only `valid_until` supersessions carry one; `supersedes` runs from the newer version to the older |
| `nested_ladder` | the level digest of each lower level equals the world digest of that level generated on its own |
| `paraphrase_zero_overlap` | question and gold entry share no search key under English, Spanish or no stemming (kmp_search_probe) |
| `crosslang_zero_overlap` | the same for Spanish/English questions and their gold entry |

The ladder is open upwards: absence is checked up to the highest generated level, and the
namespaces in render.py (anchors, bare numbers, chain and RFC ids) are what make it hold
for any higher level too.
"""
import json
import re

from .probe import ProbeUnavailable

UNKNOWN_TYPES = ('anchor_absent', 'anchor_neighbor_existing', 'singular_anchored_twin',
                 'near_miss_attribute', 'cross_about_anchor', 'interval_scoped')
EXAMPLES = 8


class Report:
    def __init__(self):
        self.checked, self.failures, self.skipped = [], {}, {}

    def check(self, name, messages):
        self.checked.append(name)
        if messages:
            self.failures[name] = {'count': len(messages), 'examples': messages[:EXAMPLES]}

    def skip(self, name, reason):
        self.skipped[name] = reason

    def as_dict(self):
        return {'ok': not self.failures, 'checked': self.checked, 'failures': self.failures,
                'skipped': self.skipped}


def _entries(world):
    return [json.loads(line) for line in world.lines['entries']]


def refs_about_prefix(world, entries, questions):
    messages = []
    by_ref = {entry['ref']: entry for entry in entries}
    for entry in entries:
        if entry['ref'] != f'{entry["about"]}:e{entry["ordinal"]:07d}':
            messages.append(f'entry {entry["ref"]}: ref is not <about>:e<ordinal>')
        if entry['coordinates'][0]['sequence'] != entry['ordinal'] + 1:
            messages.append(f'entry {entry["ref"]}: sequence is not ordinal + 1')
    for relation in map(json.loads, world.lines['relations']):
        for end in ('from', 'to'):
            target = by_ref.get(relation[end])
            if target is None or target['about'] != relation['about']:
                messages.append(f'relation {relation["id"]}: {end} is not an entry of its about')
    for write in map(json.loads, world.lines['writes']):
        arguments = write['arguments']
        if arguments['about'] != write['about'] or not arguments['memories'][0]['ref'].startswith(
                write['about'] + ':'):
            messages.append(f'write {write["id"]}: does not write into its own about')
    for question in questions:
        for ref in gold_refs(question):
            entry = by_ref.get(ref)
            if entry is None:
                messages.append(f'{question.id}: gold ref {ref} is not an entry')
            elif not ref.startswith(entry['about'] + ':') or \
                    entry['block'] >= question.min_level // world.params.block_size:
                messages.append(f'{question.id}: gold ref {ref} sits above min_level')
    return messages


def gold_refs(question):
    gold = question.gold
    refs = set(gold.wrong_subject) | set(gold.stale_refs) | set(gold.excluded_refs)
    refs |= set(gold.wake_required)
    for facet in gold.facets:
        refs |= set(facet.answers) | set(facet.related)
    if gold.chain:
        refs |= set(gold.chain.refs)
    if gold.path:
        refs |= set(gold.path.steps) | set(gold.path.required)
    for ref in (gold.current_ref, gold.nearest_outside):
        if ref:
            refs.add(ref)
    return refs


def _anchor_index(entries, anchors):
    """anchor (lowercase) -> [entry] holding it as a whole token."""
    index = {anchor.lower(): [] for anchor in anchors}
    if not anchors:
        return index
    alternation = '|'.join(re.escape(anchor) for anchor in sorted(anchors, key=len, reverse=True))
    pattern = re.compile(rf'(?<![\w#.])({alternation})(?![\w]|\.\w)', re.IGNORECASE)
    for entry in entries:
        for match in set(m.group(1).lower() for m in pattern.finditer(entry['text'])):
            index[match].append(entry)
    return index


def unknown_has_no_fact(entries, questions, block_size=1000):
    unknown = [q for q in questions if q.gold.answerable == 'UNKNOWN' and q.type in UNKNOWN_TYPES]
    anchored = [q for q in unknown if q.type != 'interval_scoped']
    index = _anchor_index(entries, {anchor for q in anchored for anchor in q.anchors})
    messages = []
    for q in anchored:
        holders = index[q.anchors[0].lower()]
        if q.type == 'anchor_absent' and holders:
            messages.append(f'{q.id}: absent anchor {q.anchors[0]} is in {holders[0]["ref"]}')
        elif q.type == 'cross_about_anchor':
            if any(entry['about'] == q.about for entry in holders):
                messages.append(f'{q.id}: anchor {q.anchors[0]} is in the asked about')
        elif q.type in ('anchor_neighbor_existing', 'singular_anchored_twin'):
            keyword = re.compile(rf'\b{re.escape(q.source["keyword"])}', re.IGNORECASE)
            if not holders:
                messages.append(f'{q.id}: anchor {q.anchors[0]} exists nowhere')
            for entry in holders:
                if keyword.search(entry['text']):
                    messages.append(f'{q.id}: {entry["ref"]} states the attribute with the anchor')
        elif q.type == 'near_miss_attribute':
            value = re.compile(rf'\b{re.escape(q.source["attribute"])}\b', re.IGNORECASE)
            if not holders:
                messages.append(f'{q.id}: anchor {q.anchors[0]} exists nowhere')
            bound = q.min_level // block_size
            if not any(value.search(entry['text']) for entry in entries
                       if entry['about'] == q.about and entry['block'] < bound):
                messages.append(f'{q.id}: attribute has df = 0 in the about')
            for entry in holders:
                if value.search(entry['text']):
                    messages.append(f'{q.id}: {entry["ref"]} co-states anchor and attribute')
    by_ref = {entry['ref']: entry for entry in entries}
    for q in unknown:
        if q.type != 'interval_scoped':
            continue
        nearest = by_ref[q.gold.nearest_outside]['truth']
        start, end = q.interval['start'], q.interval['end']
        for entry in entries:
            truth = entry['truth']
            if (truth['subject'], truth['attribute']) == (nearest['subject'], nearest['attribute']):
                if start <= entry['coordinates'][0]['occurred_at'] < end:
                    messages.append(f'{q.id}: {entry["ref"]} states the fact inside the span')
    return messages


def declared_chains_exist(world, questions):
    pairs = {frozenset((r['from'], r['to'])) for r in map(json.loads, world.lines['relations'])}
    messages = []
    for q in questions:
        if q.type not in ('multihop_why_k', 'path_between', 'path_open'):
            continue
        refs = list(q.gold.path.steps) if q.gold.path else list(reversed(q.gold.chain.refs))
        undeclared = set()
        if q.source.get('undeclared_edges'):
            undeclared = {tuple(edge.split('>')) for edge in q.source['undeclared_edges'].split(',')}
        for a, b in zip(refs, refs[1:]):
            declared = frozenset((a, b)) in pairs
            if (a, b) in undeclared and declared:
                messages.append(f'{q.id}: hop {a}>{b} is listed undeclared but is a relation')
            if (a, b) not in undeclared and q.type != 'multihop_why_k' and not declared:
                messages.append(f'{q.id}: declared hop {a}>{b} has no relation')
    return messages


def valid_until_coherent(world, entries):
    messages = []
    by_ref = {entry['ref']: entry for entry in entries}
    successor = {entry['truth']['supersedes']: entry for entry in entries
                 if entry['truth'].get('supersedes')}
    for entry in entries:
        coordinate = entry['coordinates'][0]
        episode = entry['truth']['episode']
        until = coordinate.get('valid_until')
        if until is None:
            if episode == 'supersession-valid_until' and entry['ref'] in successor:
                messages.append(f'{entry["ref"]}: replaced but has no valid_until')
            continue
        if episode != 'supersession-valid_until':
            messages.append(f'{entry["ref"]}: valid_until outside a valid_until supersession')
        if until <= coordinate['valid_from']:
            messages.append(f'{entry["ref"]}: valid_until is not after valid_from')
        nxt = successor.get(entry['ref'])
        if nxt is None or nxt['coordinates'][0]['valid_from'] != until:
            messages.append(f'{entry["ref"]}: valid_until is not its successor\'s start')
    for relation in map(json.loads, world.lines['relations']):
        if relation['rel'] == 'supersedes':
            newer, older = by_ref[relation['from']], by_ref[relation['to']]
            if newer['ordinal'] <= older['ordinal'] or newer['truth']['supersedes'] != older['ref']:
                messages.append(f'relation {relation["id"]}: supersedes does not run newer -> older')
    return messages


def nested_ladder(world, regenerate):
    """`regenerate(level)` -> the world digest of `level` generated on its own."""
    messages = []
    for level in world.params.levels[:-1]:
        alone = regenerate(level)
        if alone != world.level_digests[str(level)]:
            messages.append(f'level {level}: {world.level_digests[str(level)]} != {alone}')
    return messages


def zero_overlap(probe, questions, entries, types):
    by_ref = {entry['ref']: entry for entry in entries}
    chosen = [q for q in questions if q.type in types]
    pairs = [(q.question, by_ref[q.gold.facets[0].answers[0]]['text']) for q in chosen]
    messages = []
    for q, shared in zip(chosen, probe.overlap(pairs)):
        for morphology, keys in shared.items():
            if keys:
                messages.append(f'{q.id}: shares {keys} under {morphology}')
    return messages


def check_world(world, questions, probe=None, regenerate=None):
    entries = _entries(world)
    report = Report()
    report.check('refs_about_prefix', refs_about_prefix(world, entries, questions))
    report.check('unknown_has_no_fact',
                 unknown_has_no_fact(entries, questions, world.params.block_size))
    report.check('declared_chains_exist', declared_chains_exist(world, questions))
    report.check('valid_until_coherent', valid_until_coherent(world, entries))
    if regenerate is None:
        report.skip('nested_ladder', 'not requested')
    else:
        report.check('nested_ladder', nested_ladder(world, regenerate))
    for name, types in (('paraphrase_zero_overlap', ('paraphrase_zero_overlap',)),
                        ('crosslang_zero_overlap', ('crosslang_es_en', 'crosslang_en_es'))):
        if probe is None:
            report.skip(name, 'kmp_search_probe not available')
            continue
        try:
            report.check(name, zero_overlap(probe, questions, entries, types))
        except ProbeUnavailable as error:
            report.skip(name, str(error))
    return report.as_dict()
