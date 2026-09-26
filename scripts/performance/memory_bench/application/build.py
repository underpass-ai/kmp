"""`memory_bench build`: synth-v1 stores built by the operator's path, then cached.

  python3 -m scripts.performance.memory_bench build --seed 7 --levels 1e3,1e4 \\
      --topology mono,multi [--batch-size 1000,5000 (default 5000)] [--binary P] [--reader P] \\
      [--cpus 15-19|none] [--check-answers] [--estimate 1e5]

Per (topology, level, batch size) two cache entries (SCHEMAS.md section 7):

1. `build = ingest`: the writer binary loads the world over MCP stdio, one
   `kmp_ingest` per batch and then every cross-about `kmp_write_memory`
   (SCHEMAS.md 1.3; a `needs_review` answer is resolved by executing
   `next_actions[0]` verbatim, as a fixture resolving its own review). Every
   receipt must pass `receipt.is_accepted`, else the build stops. The store is
   exported with `kmp-mcp export`; the entry keeps the bundle and the template.
2. `build = import`: the reader binary replays that bundle into an empty store
   with `kmp-mcp import`, re-exports it and must get the same content digest.

Keys carry the generator, its version, seed, topology, N, the batch size, the
bundle format and the reader's and writer's SHA-256 (cachekey.store_key). Timings
of ingest, export, import and copy are in each store.json, and the ingest entry
keeps the per-batch profile that `ingest_growth` fits. `--check-answers` asks the
world's questions on both templates of a level and compares every response byte for
byte (determinism.run_once + parity), with an A/A run on the ingest template as the
control: the only values allowed to differ are handles each process mints
(`continuation`, `next_cursor`), which differ in the A/A too. Nothing is written
outside the cache root.
"""
from dataclasses import dataclass
import itertools
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time

from ...token_harness.native.isolation import create_store
from ...token_harness.native.transport import StdioSession
from ..domain import cachekey, ingest_growth
from ..domain.errors import BenchError
from ..domain.receipt import is_accepted, needs_review, review_action
from ..generator import to_ingest, world as synth
from ..generator.blocks import GENERATOR
from ..runtime import binaries, probes, server_log
from ..runtime.layout import public_layout
from ..runtime.store_cache import (BUNDLE, TEMPLATE, StoreCache, StoreRecord, bundle_summary,
                                   copy_template, now, template_summary)
from . import determinism

INGEST_FORMAT = 'stdio-ingest'  # the "format" of a store loaded over MCP, not from a bundle
BUILD_CPUS = '15-19'  # the other five Cortex-X925 cores: builds stay off the latency cores
BUILD_TIMEOUT_S = 3600.0
# A batch costs c_B*B + c_H*H (H = entries the about holds, BT14 measurement at 10^4), so a
# build is O(N^2/B); 5000 keeps 10^5 mono near 8 minutes (BENCH_SPEC 4.5).
DEFAULT_BATCH = 5000
MAX_REVIEW_ROUNDS = 3
CLIENT_NAME = 'memory-bench-build'


class BuildFailed(BenchError):
    code = 'STORE_BUILD_FAILED'


@dataclass(frozen=True)
class BuildRequest:
    seed: int
    topology: str
    level: int
    batch_size: int = DEFAULT_BATCH
    cpus: str | None = BUILD_CPUS
    timeout_s: float = BUILD_TIMEOUT_S

    def label(self):
        return f'{GENERATOR} seed {self.seed} {self.topology} {self.level} B{self.batch_size}'


class _Discard:
    """A TraceRecorder that keeps nothing: a build's frames are the world's own lines."""

    def pair(self, session, raw_request, raw_response):
        return None

    def notification(self, session, raw_request):
        return None

    def marker(self, event):
        return None


# -- worlds -------------------------------------------------------------------------------

def find_world(layout, seed, topology, level):
    """The cached world of this generator version holding `level` (smallest one first)."""
    folder = layout.root / 'worlds'
    found = []
    for manifest in sorted(folder.glob('*/manifest.json')) if folder.is_dir() else ():
        record = json.loads(manifest.read_text(encoding='utf-8'))
        if (record.get('generator'), record.get('generator_version'), record.get('seed'),
                record.get('topology')) == (GENERATOR, synth.GENERATOR_VERSION, seed, topology) \
                and level in record.get('levels', ()):
            found.append((record['levels'][-1], manifest.parent))
    if not found:
        raise BuildFailed(f'no {GENERATOR} {synth.GENERATOR_VERSION} world for seed {seed} '
                          f'{topology} at {level}: run memory_bench world first')
    return min(found)[1]


def load_world(directory, level):
    """A World read back from its files, each checked against the manifest, `level` re-digested."""
    directory = Path(directory)
    manifest = json.loads((directory / 'manifest.json').read_text(encoding='utf-8'))
    params = synth.WorldParams(manifest['seed'], manifest['topology'], tuple(manifest['levels']),
                               manifest['block_size'])
    lines, marks = {}, {}
    for section in synth.SECTIONS:
        data = (directory / f'{section}.jsonl').read_bytes()
        if cachekey.sha256_hex(data) != manifest['files'][f'{section}.jsonl']['sha256']:
            raise BuildFailed(f'{directory}: {section}.jsonl differs from its manifest')
        lines[section] = data.decode('utf-8').splitlines()
        if section == 'abouts':
            marks[section] = [0] * len(lines[section])
        else:
            field = 'min_level' if section == 'questions' else 'block'
            marks[section] = [json.loads(line)[field] for line in lines[section]]
    loaded = synth.World(params, manifest['world_key'], lines, marks, dict(manifest['level_digests']))
    if synth.level_digest(loaded, level) != manifest['level_digests'][str(level)]:
        raise BuildFailed(f'{directory}: level {level} does not reproduce its digest')
    return loaded


# -- ingest over MCP ----------------------------------------------------------------------

def _structured(response, what):
    if 'error' in response:
        raise BuildFailed(f'{what}: rpc error {str(response["error"])[:300]}')
    result = response.get('result') or {}
    structured = result.get('structuredContent') or {}
    if result.get('isError'):
        raise BuildFailed(f'{what}: tool error {json.dumps(structured.get("error"))[:300]}')
    return structured


def _write(session, arguments, what, log):
    """One kmp_write_memory, its review resolved verbatim; returns the review rounds used."""
    structured = _structured(session.call('kmp_write_memory', arguments), what)
    rounds = 0
    while needs_review(structured):
        rounds += 1
        action = review_action(structured)
        if rounds > MAX_REVIEW_ROUNDS or action is None:
            raise BuildFailed(f'{what}: still under review after {rounds - 1} round(s)')
        print(f'memory_bench build: {what}: resolving needs_review with next_actions[0] verbatim',
              file=log)
        structured = _structured(session.call(*action), what)
    if not is_accepted(structured):
        raise BuildFailed(f'{what}: write not accepted (status {structured.get("status")!r})')
    return rounds


def _server_ms(cursor):
    """duration_ms of every kmp_mcp_tool line the build process logged, in call order."""
    return [call.tool.duration_ms for call in server_log.parse(cursor.read_new()).calls]


def ingest_world(binary, loaded, request, work_dir, log=sys.stderr):
    """Load `loaded` up to the level into a new disposable store; (IsolatedStore, profile)."""
    store = create_store(work_dir, f'build-{request.topology}-{request.level}')
    try:
        cursor = server_log.LogCursor(store.data_dir)
        pinning = probes.resolve_pinning(request.cpus)
        with pinning.for_children():
            session = StdioSession(binary.path, store, _Discard(), 'build', itertools.count(1),
                                   timeout_seconds=request.timeout_s, probe_resources=False)
        try:
            protocol = session.handshake(CLIENT_NAME)
            cpus_allowed = probes.allowed_cpus(session.process.pid)
            rows, held, abouts = [], {}, {}
            started = time.perf_counter_ns()
            for about, arguments in to_ingest.ingest_calls(loaded, request.level, request.batch_size):
                memory = arguments['memory']
                call_started = time.perf_counter_ns()
                structured = _structured(session.call('kmp_ingest', arguments), f'ingest {about}')
                wall_ms = (time.perf_counter_ns() - call_started) / 1e6
                if not is_accepted(structured):
                    raise BuildFailed(f'ingest {about}: receipt not accepted')
                index = abouts.setdefault(about, len(abouts))
                rows.append([index, held.get(about, 0), len(memory['entries']),
                             len(memory['relations']), round(wall_ms, 3), None])
                held[about] = held.get(about, 0) + len(memory['entries'])
            ingest_ms = (time.perf_counter_ns() - started) / 1e6
            write_started, reviews = time.perf_counter_ns(), 0
            writes = to_ingest.write_calls(loaded, request.level)
            write_ms = []
            for number, (about, arguments) in enumerate(writes):
                call_started = time.perf_counter_ns()
                reviews += _write(session, arguments, f'write {number} into {about}', log)
                write_ms.append(round((time.perf_counter_ns() - call_started) / 1e6, 3))
            writes_ms = (time.perf_counter_ns() - write_started) / 1e6
        finally:
            session.close()
        durations = _server_ms(cursor)
        if len(durations) >= len(rows):
            for row, duration in zip(rows, durations):
                row[-1] = duration
        return store, {
            'batch_size': request.batch_size, 'columns': list(ingest_growth.COLUMNS),
            'batches': rows, 'abouts': sorted(abouts, key=abouts.get),
            'entries': sum(row[2] for row in rows), 'relations': sum(row[3] for row in rows),
            'writes': len(writes), 'reviews': reviews, 'write_ms': write_ms,
            'ingest_ms': round(ingest_ms, 3), 'writes_ms': round(writes_ms, 3),
            'server': protocol.get('server'), 'cpus_allowed': cpus_allowed,
            'pinning': pinning.as_dict(), 'loadavg': probes.loadavg()}
    except BaseException:
        shutil.rmtree(store.root, ignore_errors=True)
        raise


# -- cache entries ------------------------------------------------------------------------

def _source(request, loaded):
    return cachekey.synth_source(generator=GENERATOR, generator_version=synth.GENERATOR_VERSION,
                                 seed=request.seed, topology=request.topology,
                                 world_digest=loaded.level_digests[str(request.level)])


def ingest_material(request, loaded, writer):
    return {'source': _source(request, loaded), 'n': request.level, 'bundle_format': INGEST_FORMAT,
            'reader_sha256': writer.sha256, 'build': 'ingest', 'writer_sha256': writer.sha256,
            'batch_size': request.batch_size, 'store_mode': 'shared'}


def import_material(request, loaded, writer, reader, header):
    return {'source': _source(request, loaded), 'n': request.level,
            'bundle_format': binaries.bundle_format(header), 'reader_sha256': reader.sha256,
            'build': 'import', 'writer_sha256': writer.sha256, 'batch_size': request.batch_size,
            'store_mode': 'shared'}


def _copy_ms(cache, template):
    """What every run pays: one copy of the template into work/, then discarded."""
    target = cache.stage('copy-probe')
    try:
        return copy_template(template, target / TEMPLATE)
    finally:
        shutil.rmtree(target, ignore_errors=True)


def _record(material, label, header, staging, timings, absent, ingest, checks):
    return StoreRecord(cachekey.store_key(**material), material, label, header['content_digest'],
                       header['event_count'], bundle_summary(staging / BUNDLE, header),
                       template_summary(staging / TEMPLATE),
                       {name: None if value is None else round(value, 3) for name, value in timings.items()},
                       absent, ingest, checks, now())


def build_ingest(cache, request, loaded, writer, log=sys.stderr):
    material = ingest_material(request, loaded, writer)
    staging = cache.stage(cachekey.store_key(**material))
    store = None
    try:
        store, profile = ingest_world(writer, loaded, request, cache.layout.scratch(), log)
        summary, exported = binaries.export_bundle(writer, store, staging / BUNDLE, request.timeout_s)
        header = binaries.bundle_header(staging / BUNDLE)
        if header['content_digest'] != summary['content_digest']:
            raise BuildFailed('export summary and bundle header disagree')
        copy_template(store.data_dir, staging / TEMPLATE)
        timings = {'ingest': profile['ingest_ms'] + profile['writes_ms'], 'export': exported.wall_ms,
                   'import': None, 'copy': _copy_ms(cache, staging / TEMPLATE)}
        checks = {'receipts_accepted': len(profile['batches']) + profile['writes'],
                  'export_data_dir': 'confirmed', 'template_excludes': ['logs']}
        record = _record(material, request.label(), header, staging, timings,
                         {'import': 'built by ingest over MCP, not imported'}, profile, checks)
        return cache.commit(staging, record)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    finally:
        if store is not None:
            shutil.rmtree(store.root, ignore_errors=True)


def build_import(cache, request, loaded, writer, reader, source):
    header = binaries.bundle_header(source.bundle)
    material = import_material(request, loaded, writer, reader, header)
    staging = cache.stage(cachekey.store_key(**material))
    store = create_store(cache.layout.scratch(), f'import-{request.topology}-{request.level}')
    try:
        report, imported = binaries.import_bundle(reader, store, source.bundle, request.timeout_s)
        again, exported = binaries.export_bundle(reader, store, store.root / 'reexport.jsonl',
                                                 request.timeout_s)
        if again['content_digest'] != header['content_digest'] or \
                again['event_count'] != header['event_count']:
            raise BuildFailed('the imported store does not re-export the bundle it was built from')
        try:
            os.link(source.bundle, staging / BUNDLE)
        except OSError:
            shutil.copyfile(source.bundle, staging / BUNDLE)
        copy_template(store.data_dir, staging / TEMPLATE)
        timings = {'ingest': None, 'export': exported.wall_ms, 'import': imported.wall_ms,
                   'copy': _copy_ms(cache, staging / TEMPLATE)}
        checks = {'source_store_key': source.record.store_key, 'reexport_content_digest': 'equal',
                  'events_imported': report['events_imported'],
                  'mutations_applied': report.get('mutations_applied'), 'template_excludes': ['logs']}
        record = _record(material, request.label() + ' imported', header, staging, timings,
                         {'ingest': f'imported from the bundle of store {source.record.store_key}'},
                         None, checks)
        return cache.commit(staging, record)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    finally:
        shutil.rmtree(store.root, ignore_errors=True)


def build_stores(request, writer, reader=None, layout=None, log=sys.stderr):
    """{'ingest': CachedStore, 'import': CachedStore, 'world': Path}, built or found in the cache."""
    layout = layout or public_layout()
    cache = StoreCache(layout)
    reader = reader or writer
    directory = find_world(layout, request.seed, request.topology, request.level)
    loaded = load_world(directory, request.level)
    ingested = cache.get(cachekey.store_key(**ingest_material(request, loaded, writer)))
    if ingested is None:
        ingested = build_ingest(cache, request, loaded, writer, log)
    header = binaries.bundle_header(ingested.bundle)
    imported = cache.get(cachekey.store_key(**import_material(request, loaded, writer, reader, header)))
    if imported is None:
        imported = build_import(cache, request, loaded, writer, reader, ingested)
    return {'ingest': ingested, 'import': imported, 'world': directory, 'loaded': loaded}


# -- checks and summaries -----------------------------------------------------------------

# Handles a process mints for itself (parity.VOLATILE_FIELDS): two processes on one
# template already differ here, so they say nothing about the store.
PROCESS_MINTED = ('continuation', 'next_cursor')


def _pair(left, right):
    _, pairs = determinism.compare_runs([left, right])
    pair = pairs[0]
    reconciled = {name: count for name, count in pair['volatile_reconciled'].items() if count}
    return {'calls': pair['calls'], 'identical': pair['identical'], 'normalized': pair['normalized'],
            'different': pair['different'], 'byte_identical_rate': pair['byte_identical_rate'],
            'parity_rate': pair['parity_rate'], 'volatile_reconciled': reconciled,
            'path_counts': pair['path_counts']}


def compare_answers(binary, left, right, questions, root, control=True):
    """Ask `questions` on two templates (fresh copies each) and compare every response.

    The stores agree when no call differs and every normalized value is a handle the
    process mints (`PROCESS_MINTED`); with `control`, a second run on `left` shows the
    same handles differing between two processes on one template (A/A).
    """
    root = Path(root)
    items = determinism.question_items(questions)
    runs = {'left': left, 'right': right, **({'left-again': left} if control else {})}
    for name, template in runs.items():
        determinism.run_once(binary.path, items, root / name, root / 'work', template)
    result = _pair(root / 'left', root / 'right')
    result['questions'] = len(items)
    result['stores_agree'] = (result['different'] == 0
                              and set(result['volatile_reconciled']) <= set(PROCESS_MINTED))
    result['control_aa'] = _pair(root / 'left', root / 'left-again') if control else None
    return result


def _estimates(fit, abouts, batch_sizes, levels):
    """Predicted ingest ms of a mono build at `levels` for each batch size; None for multi."""
    if fit is None or not levels or len(abouts) != 1:
        return None  # only mono has one about whose size is the level
    return {f'{level}@B{batch}': round(fit.estimate_ms([level], batch))
            for level in levels for batch in batch_sizes}


def summary(built, estimate_levels=()):
    ingest, imported = built['ingest'].record, built['import'].record
    profile = ingest.ingest
    fit = ingest_growth.fit(profile['batches'])
    writes = profile.get('write_ms') or []  # absent from entries built before it was kept
    return {'label': ingest.label, 'ingest_key': ingest.store_key, 'import_key': imported.store_key,
            'content_digest': ingest.content_digest, 'event_count': ingest.event_count,
            'bundle_bytes': ingest.bundle['bytes'], 'template_bytes': imported.template['bytes'],
            'timings_ms': {'ingest': ingest.timings_ms['ingest'], 'ingest_batches': profile['ingest_ms'],
                           'writes': profile['writes_ms'], 'export': ingest.timings_ms['export'],
                           'import': imported.timings_ms['import'],
                           'reexport': imported.timings_ms['export'], 'copy': imported.timings_ms['copy']},
            'writes': {'count': profile['writes'], 'reviews': profile['reviews'],
                       'max_ms': max(writes) if writes else None},
            'growth': fit.as_dict() if fit else None,
            'estimate_ingest_ms': _estimates(fit, profile['abouts'], (profile['batch_size'],),
                                             estimate_levels)}


def pooled(builds, estimate_levels=()):
    """One fit over every batch size of one (topology, level): what separates B from H."""
    rows = [row for built in builds for row in built['ingest'].record.ingest['batches']]
    fit = ingest_growth.fit(rows)
    first = builds[0]['ingest'].record
    sizes = sorted({built['ingest'].record.ingest['batch_size'] for built in builds})
    return {'topology': first.material['source']['topology'], 'n': first.material['n'],
            'batch_sizes': sizes, 'growth': fit.as_dict() if fit else None,
            'estimate_ingest_ms': _estimates(fit, first.ingest['abouts'], sizes, estimate_levels)}


def run_build(args):
    layout = public_layout(args.out)
    writer = binaries.KmpBinary.locate(args.binary)
    reader = binaries.KmpBinary.locate(args.reader) if args.reader else writer
    cpus = None if args.cpus == 'none' else args.cpus
    results, pools = [], []
    for topology in args.topology:
        for level in args.levels:
            builds = []
            for batch_size in args.batch_size:
                request = BuildRequest(args.seed, topology, level, batch_size, cpus, args.timeout)
                started = time.perf_counter()
                built = build_stores(request, writer, reader, layout)
                builds.append(built)
                row = summary(built, args.estimate)
                row['elapsed_s'] = round(time.perf_counter() - started, 3)
                if args.check_answers:
                    row['answers_ingest_vs_import'] = _check_answers(layout, reader, built, level)
                results.append(row)
                print(json.dumps(row, sort_keys=True), file=sys.stderr, flush=True)
            if len(builds) > 1:
                pools.append(pooled(builds, args.estimate))
    print(json.dumps({'builds': results, 'pooled': pools}, indent=1, sort_keys=True))
    failed = [row for row in results if 'answers_ingest_vs_import' in row
              and not row['answers_ingest_vs_import']['stores_agree']]
    return 1 if failed else 0


def _check_answers(layout, reader, built, level):
    questions = [q for q in synth.load_questions(built['loaded']) if q.min_level <= level]
    scratch = layout.require_inside(layout.scratch())
    scratch.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix='answers-', dir=scratch))
    try:
        return compare_answers(reader, built['ingest'].template, built['import'].template, questions, root)
    finally:
        shutil.rmtree(root, ignore_errors=True)


def run_materialize(args):
    layout = public_layout(args.out)
    cache = StoreCache(layout)
    destination = Path(args.dest) if args.dest else layout.scratch() / f'materialized-{args.store_key[:16]}'
    path, copy_ms = cache.materialize(args.store_key, destination)
    print(json.dumps({'store_key': args.store_key, 'data_dir': str(path), 'copy_ms': round(copy_ms, 3)}))
    return 0


def run_cache(args):
    layout = public_layout(args.out)
    cache = StoreCache(layout)
    if args.operation == 'gc':
        print(json.dumps({'removed': [str(path) for path in cache.gc()]}, indent=1))
        return 0
    rows = []
    for directory, entry, error in cache.entries():
        if entry is None:
            rows.append({'store_key': directory.name, 'error': error})
            continue
        record = entry.record
        rows.append({'store_key': record.store_key, 'label': record.label,
                     'build': record.material['build'], 'n': record.material['n'],
                     'content_digest': record.content_digest, 'event_count': record.event_count,
                     'template_bytes': record.template['bytes'], 'created_at': record.created_at})
    print(json.dumps(rows, indent=1, sort_keys=True))
    return 0


def add_arguments(build, materialize, cache):
    from ..cli import parse_levels, parse_topologies
    build.add_argument('--generator', choices=(GENERATOR,), default=GENERATOR)
    build.add_argument('--seed', type=int, required=True)
    build.add_argument('--levels', type=parse_levels, default=(1000, 10000))
    build.add_argument('--topology', type=parse_topologies, default=('mono', 'multi'))
    build.add_argument('--batch-size', type=parse_levels, default=(DEFAULT_BATCH,),
                       help=f'comma-separated batch sizes B (default {DEFAULT_BATCH})')
    build.add_argument('--binary', type=Path, help='writer kmp-mcp (default target/release/kmp-mcp)')
    build.add_argument('--reader', type=Path, help='reader kmp-mcp for the import (default: the writer)')
    build.add_argument('--cpus', default=BUILD_CPUS, help="taskset-style list for the writer; 'none'")
    build.add_argument('--timeout', type=float, default=BUILD_TIMEOUT_S, help='seconds per call')
    build.add_argument('--estimate', type=parse_levels, default=(),
                       help='mono levels to predict from the fitted growth, e.g. 1e5')
    build.add_argument('--check-answers', action='store_true',
                       help='ask the world questions on the ingest and import templates, compare, '
                            'and repeat the ingest one (A/A control)')
    materialize.add_argument('--store-key', required=True)
    materialize.add_argument('--dest', type=Path, help='new directory inside the cache root')
    for command in (build, materialize, cache):
        command.add_argument('--out', type=Path, help='cache root (default <repo>/tmp/memory-bench)')
