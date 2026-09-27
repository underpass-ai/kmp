"""The synth-v1 sections: one per topology, the lowest level primary, higher levels extra.

The store is shared by content (BENCH_SPEC section 8, `store = 'shared'`): the
baseline's binary writes the world over MCP stdio once (`build = ingest`), and
every arm replays that bundle with its own binary (`build = import`,
`application/build.build_stores`), so both arms read the same `content_digest`
and a candidate's derived indexes are rebuilt by the candidate. Both entries are
cached; with the base cached, a candidate pays one import per level.

Every level asks the same questions, those of the lowest level
(`min_level <= levels[0]`), so the higher levels are extra runs of one question
set: headlines per level and the latency scale exponent. A ladder may leave out
question types (`exclude_types`) and add far trace pairs (`far_trace`,
`application/far_trace.py`); both apply to every level alike. A world missing from
the cache is generated first (with the kmp_search_probe oracles when the probe is
built, otherwise without them, which the world manifest records).
"""
import time

from ..domain.errors import BenchError
from ..generator import world as synth
from ..generator.probe import ProbeUnavailable, SearchProbe
from ..runtime import binaries
from ..runtime.layout import public_layout
from . import build, far_trace, sections
from .run_questions import StoreRef
from .world import build as build_world


def ensure_world(layout, ladder, topology):
    try:
        return build.find_world(layout, ladder.seed, topology, ladder.levels[-1])
    except build.BuildFailed:
        pass
    try:
        probe = SearchProbe.locate(None)
    except ProbeUnavailable:
        probe = None
    world = build_world(synth.WorldParams(ladder.seed, topology, tuple(ladder.levels)), probe)
    directory = layout.require_inside(layout.world(world.world_key))
    synth.write_world(world, directory)
    return directory


def store_ref(cached):
    record = cached.record
    return StoreRef(record.store_key, record.content_digest, record.label, cached.template)


def stores_at(specs, ladder, topology, level, layout):
    """The shared store of `level` as each arm reads it (the baseline writes, each arm imports)."""
    writer = binaries.KmpBinary.locate(specs[0].binary.path)
    request = build.BuildRequest(ladder.seed, topology, level, ladder.batch_size)
    refs, loaded = [], None
    for spec in specs:
        reader = binaries.KmpBinary.locate(spec.binary.path)
        built = build.build_stores(request, writer, reader, layout)
        refs.append(store_ref(built['import']))
        loaded = built['loaded']
    return refs, loaded


def select(questions, ladder):
    """The questions of the lowest level but the excluded types; with `per_type`, the first
    ones (by id) of each type."""
    chosen = sorted((q for q in questions if q.min_level <= ladder.levels[0]
                     and q.type not in ladder.exclude_types), key=lambda q: q.id)
    if not ladder.per_type:
        return tuple(chosen)
    by_type = {}
    for question in chosen:
        by_type.setdefault(question.type, []).append(question)
    return tuple(q for kind in sorted(by_type) for q in by_type[kind][:ladder.per_type])


def run_topology(specs, mode, topology, layout, replica_nonce=None):
    ladder = mode.synth
    name = f'synth-{topology}'
    try:
        ensure_world(layout, ladder, topology)
        stores, loaded = stores_at(specs, ladder, topology, ladder.levels[0], layout)
        questions = select(synth.load_questions(loaded), ladder) + far_trace.questions(loaded, ladder, topology)
        results = sections.measure(specs, stores, questions, mode, layout, replica_nonce=replica_nonce)
        extras = []
        for max_bytes in ladder.max_bytes_sweep:
            extras.append((ladder.levels[0], sections.measure(specs, stores, questions, mode, layout,
                                                              max_bytes, replica_nonce)))
        for level in ladder.levels[1:]:
            level_stores, _ = stores_at(specs, ladder, topology, level, layout)
            extras.append((level, sections.measure(specs, level_stores, questions, mode, layout,
                                                   replica_nonce=replica_nonce)))
        return sections.report_section(name, results, questions, specs, mode, layout, extras,
                                       ladder.levels[0], topology)
    except BenchError as error:
        return sections.failed(name, error)


def run(specs, mode, replica_nonce=None, layout=None):
    layout = layout or public_layout()
    results = []
    for topology in mode.synth.topologies:
        started = time.perf_counter()
        result = run_topology(specs, mode, topology, layout, replica_nonce)
        result.seconds = time.perf_counter() - started
        results.append(result)
    return results
