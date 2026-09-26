"""A synth-v1 world (kmp.bench.world.v1, SCHEMAS.md section 1): blocks, records, digests.

`generate(params)` builds every block up to the highest level, turns them into the five
record sections (abouts, entries, relations, writes, questions) and computes the level
digests. `write_world(world, directory)` writes the files and `manifest.json`.

Entry `truth` (never sent to KMP) holds the generator's ground truth:

| key | meaning |
|---|---|
| `subject` | who the entry is about (`svc-017`, `db-004`, `pool-012`, a paraphrase row id...) |
| `attribute` | facet name, `event`, a changing attribute, `hub`, `note`, ... |
| `value` | what the entry states about it (the answer a reader would quote) |
| `anchors` | anchors the entry carries (family entries only) |
| `chain` | `{id, position, length, declared_prev, mentions_prev}` for chain nodes |
| `lang` | `en` or `es` |
| `paraphrase_of` | paraphrase combo (`s03+p07`) of a paraphrase entry |
| `supersedes` | the ref this version replaces (whatever the supersession mode) |
| `episode` | `family`, `chain`, `supersession-<mode>`, `hub`, `hub_note`, `distractor`, `rare`, `paraphrase`, `crosslang`, `echo`, `filler` |
| `group` | `g<group>-f<family>` for family entries |

Digests: `level_digests[L]` is SHA-256 over, per section in the order abouts, entries,
relations, writes, questions, the bytes `<section>\\n` followed by every record line of that
section at level L, each line canonical JSON plus `\\n` (the file bytes of the section).
"""
from dataclasses import dataclass, field
import hashlib
import json
from pathlib import Path
import time

from .. import BENCH_VERSION
from ..domain import cachekey, jsonl
from ..domain.question import Question
from . import render
from .blocks import GENERATOR, BlockBuilder
from .bridge import BridgeTable
from .calibration import REFERENCE
from .questions import block_questions
from .topology import Topology

GENERATOR_VERSION = '1.2.0'
BLOCK_SIZE = 1000
SECTIONS = ('abouts', 'entries', 'relations', 'writes', 'questions')
WORLD_SCHEMA = 'kmp.bench.world.v1'


class WorldError(ValueError):
    pass


@dataclass(frozen=True)
class WorldParams:
    seed: int
    topology: str
    levels: tuple
    block_size: int = BLOCK_SIZE

    def __post_init__(self):
        Topology(self.topology)
        if not self.levels or list(self.levels) != sorted(set(self.levels)):
            raise WorldError('levels must be strictly increasing')
        for level in self.levels:
            if level <= 0 or level % self.block_size:
                raise WorldError(f'level {level} is not a positive multiple of {self.block_size}')

    @property
    def max_level(self):
        return self.levels[-1]

    @property
    def blocks(self):
        return self.max_level // self.block_size

    def world_key(self):
        return cachekey.world_key(generator=GENERATOR, generator_version=GENERATOR_VERSION,
                                  seed=self.seed, topology=self.topology,
                                  max_level=self.max_level, block_size=self.block_size)


@dataclass
class World:
    params: WorldParams
    world_key: str
    lines: dict                 # section -> [canonical line]
    blocks_of: dict             # section -> [block of each line] (min_level for questions)
    level_digests: dict = field(default_factory=dict)
    invariants: dict | None = None
    calibration: dict | None = None
    timings_ms: dict = field(default_factory=dict)

    def records(self, section):
        return [json.loads(line) for line in self.lines[section]]

    def count_at(self, section, level):
        """Records of `section` at `level`: a prefix, since every section is in ladder order."""
        marks = self.blocks_of[section]
        if section == 'abouts':
            return len(marks)
        if section == 'questions':
            return sum(1 for mark in marks if mark <= level)
        return sum(1 for mark in marks if mark < level // self.params.block_size)


def entry_record(member, block_size):
    coordinate = {'dimension': 'work', 'scope_id': 'work:main',
                  'occurred_at': render.instant(member.ordinal),
                  'valid_from': render.instant(member.ordinal), 'sequence': member.ordinal + 1}
    return {'ref': member.ref, 'about': member.about, 'ordinal': member.ordinal,
            'block': member.ordinal // block_size, 'kind': member.kind, 'text': member.text,
            'coordinates': [coordinate], 'metadata': {}, 'truth': member.truth}


def generate(params, with_questions=True):
    """Every block up to the highest level, as record lines plus level digests."""
    started = time.perf_counter()
    topology = Topology(params.topology)
    lines = {section: [] for section in SECTIONS}
    marks = {section: [] for section in SECTIONS}
    dump = cachekey.canonical_json
    for record in topology.about_records():
        lines['abouts'].append(dump(record))
        marks['abouts'].append(0)
    relation_count = write_count = 0
    questions = []
    table = BridgeTable.load() if with_questions else None
    for k in range(params.blocks):
        block = BlockBuilder(params.seed, topology, k, params.block_size,
                             REFERENCE['length_percentiles']).build()
        successors = _valid_until(block)
        for member in block.members:
            record = entry_record(member, params.block_size)
            if member.key in successors:
                record['coordinates'][0]['valid_until'] = render.instant(successors[member.key])
            lines['entries'].append(dump(record))
            marks['entries'].append(k)
        for relation in block.relations:
            relation_count += 1
            lines['relations'].append(dump({
                'id': f'r{relation_count:07d}', 'about': relation.about,
                'block': relation.later // params.block_size, 'from': relation.source,
                'to': relation.target, 'rel': relation.rel, 'class': relation.cls,
                'why': relation.why, 'evidence': relation.evidence, 'confidence': 'high',
                'clocks': None}))
            marks['relations'].append(relation.later // params.block_size)
        for echo, target in block.echoes:
            write_count += 1
            lines['writes'].append(dump(write_record(write_count, echo, target, params.block_size)))
            marks['writes'].append(k)
        if with_questions:
            for question in block_questions(params, topology, block, table):
                questions.append(question)
    for question in questions:
        lines['questions'].append(dump(question.as_dict()))
        marks['questions'].append(question.min_level)
    world = World(params, params.world_key(), lines, marks)
    world.level_digests = {str(level): level_digest(world, level) for level in params.levels}
    world.timings_ms['generate'] = round((time.perf_counter() - started) * 1000)
    return world


def _valid_until(block):
    """member key -> successor start (minutes) for `valid_until` supersessions."""
    by_key = {member.key: member for member in block.members}
    return {member.key: by_key[member.valid_until_of].ordinal
            for member in block.members if member.valid_until_of is not None}


def write_record(number, echo, target, block_size):
    """A cross-about `same_event_as` link, as the kmp_write_memory call that makes it.

    The link goes in `relations`, the tool's shape for connecting memories that already
    exist: the kernel commits only the link and its evidence and leaves both entries
    untouched. Resubmitting the stored echo under `memories` would write a second
    revision of it (writer metadata, a label coordinate, a new content hash), and v0.23.0
    refuses that shape without `labels` anyway.
    """
    identifier = echo.truth['subject']
    return {'id': f'w{number:07d}', 'block': echo.ordinal // block_size, 'about': echo.about,
            'arguments': {
                'about': echo.about, 'actor': 'synth-writer',
                'observed_at': render.instant(echo.ordinal),
                'read_context': {'inspected_refs': [echo.ref],
                                 'relate_proposals': [{'from': echo.ref, 'to': target.ref,
                                                       'proposed_by': ['identifier']}]},
                'options': {'strict': True},
                'relations': [{'from': echo.ref, 'to': target.ref, 'rel': 'same_event_as',
                               'class': 'evidential', 'why': f'Both record {identifier}.',
                               'evidence': f'Identifier {identifier} in both.',
                               'confidence': 'high'}]}}


def level_digest(world, level):
    hasher = hashlib.sha256()
    for section in SECTIONS:
        hasher.update(section.encode('utf-8') + b'\n')
        count = world.count_at(section, level)
        for line in world.lines[section][:count]:
            hasher.update(line.encode('utf-8') + b'\n')
    return hasher.hexdigest()


def load_questions(world):
    return tuple(Question.from_dict(record) for record in world.records('questions'))


def manifest(world):
    params = world.params
    counts = {section: len(world.lines[section]) for section in SECTIONS if section != 'abouts'}
    level_counts = {str(level): {section: world.count_at(section, level)
                                 for section in SECTIONS if section != 'abouts'}
                    for level in params.levels}
    return {'schema': WORLD_SCHEMA, 'generator': GENERATOR, 'generator_version': GENERATOR_VERSION,
            'bench_version': BENCH_VERSION, 'world_key': world.world_key, 'seed': params.seed,
            'topology': params.topology, 'block_size': params.block_size,
            'levels': list(params.levels), 'zipf_s': Topology(params.topology).zipf_s,
            'time_origin': render.TIME_ORIGIN_TEXT, 'abouts': len(world.lines['abouts']),
            'counts': counts, 'level_counts': level_counts, 'files': {},
            'level_digests': dict(world.level_digests),
            'world_digest': world.level_digests[str(params.max_level)],
            'invariants': world.invariants, 'calibration': world.calibration,
            'timings_ms': dict(world.timings_ms)}


def write_world(world, directory):
    """Writes the five JSONL files and manifest.json; returns the manifest."""
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    files = {}
    for section in SECTIONS:
        data = ''.join(line + '\n' for line in world.lines[section]).encode('utf-8')
        path = directory / f'{section}.jsonl'
        path.write_bytes(data)
        files[path.name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    record = manifest(world)
    record['files'] = files
    jsonl.write_text(directory / 'manifest.json',
                     json.dumps(record, indent=1, sort_keys=True, ensure_ascii=False) + '\n',
                     overwrite=True)
    return record
