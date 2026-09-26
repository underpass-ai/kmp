"""Public-dataset stores, loaded by the operator's path and cached (BENCH_SPEC 4.6, BT17).

The same two cache entries as `application/build.py` builds for synth-v1 worlds
(SCHEMAS.md 7.1), keyed with `cachekey.dataset_source` (locked files, subset,
adapter version) instead of a world:

1. `build = ingest`: the writer loads the corpus over MCP stdio, one `kmp_ingest`
   per batch of entries (`public_corpus.ingest_calls`: entries only, no relation),
   every receipt checked; `kmp-mcp export` writes the bundle.
2. `build = import`: the reader replays that bundle into an empty store with
   `kmp-mcp import` and must re-export the same content digest. Runs read this one.

Timings (ingest, export, import, copy) and the per-batch profile land in each
`store.json`, as for synth stores. A private corpus (LoCoMo) is built in the
private layout outside the repository.
"""
import itertools
import shutil
import sys
import time

from ...token_harness.native.isolation import create_store
from ...token_harness.native.transport import StdioSession
from ..corpora.public_corpus import DEFAULT_BATCH, ingest_calls
from ..domain import cachekey, ingest_growth
from ..domain.receipt import is_accepted
from ..runtime import binaries, probes, server_log
from ..runtime.store_cache import BUNDLE, TEMPLATE, StoreCache, copy_template
from . import build as synth_build

CLIENT_NAME = 'memory-bench-public-build'


def source_of(corpus, lock_entry):
    return cachekey.dataset_source(corpus=corpus.name, lock_sha256=lock_entry.digest(),
                                   selection_digest=corpus.selection_digest(),
                                   adapter_version=corpus.adapter_version)


def ingest_material(corpus, lock_entry, writer, batch_size):
    return {'source': source_of(corpus, lock_entry), 'n': corpus.entry_count(),
            'bundle_format': synth_build.INGEST_FORMAT, 'reader_sha256': writer.sha256,
            'build': 'ingest', 'writer_sha256': writer.sha256, 'batch_size': batch_size,
            'store_mode': 'shared'}


def import_material(corpus, lock_entry, writer, reader, header, batch_size):
    return {'source': source_of(corpus, lock_entry), 'n': corpus.entry_count(),
            'bundle_format': binaries.bundle_format(header), 'reader_sha256': reader.sha256,
            'build': 'import', 'writer_sha256': writer.sha256, 'batch_size': batch_size,
            'store_mode': 'shared'}


def label(corpus, batch_size):
    return f'{corpus.name} {corpus.entry_count()} entries B{batch_size}'


def ingest_corpus(binary, corpus, batch_size, work_dir, cpus, timeout_s):
    """Load `corpus` into a new disposable store over stdio; (IsolatedStore, ingest profile)."""
    store = create_store(work_dir, f'public-{corpus.name}')
    try:
        cursor = server_log.LogCursor(store.data_dir)
        pinning = probes.resolve_pinning(cpus)
        with pinning.for_children():
            session = StdioSession(binary.path, store, synth_build._Discard(), 'build', itertools.count(1),
                                   timeout_seconds=timeout_s, probe_resources=False)
        try:
            protocol = session.handshake(CLIENT_NAME)
            cpus_allowed = probes.allowed_cpus(session.process.pid)
            rows, held, abouts = [], {}, {}
            started = time.perf_counter_ns()
            for about, arguments in ingest_calls(corpus, batch_size):
                entries = arguments['memory']['entries']
                call_started = time.perf_counter_ns()
                structured = synth_build._structured(session.call('kmp_ingest', arguments), f'ingest {about}')
                wall_ms = (time.perf_counter_ns() - call_started) / 1e6
                if not is_accepted(structured):
                    raise synth_build.BuildFailed(f'ingest {about}: receipt not accepted')
                index = abouts.setdefault(about, len(abouts))
                rows.append([index, held.get(about, 0), len(entries), 0, round(wall_ms, 3), None])
                held[about] = held.get(about, 0) + len(entries)
            ingest_ms = (time.perf_counter_ns() - started) / 1e6
        finally:
            session.close()
        durations = synth_build._server_ms(cursor)
        if len(durations) >= len(rows):
            for row, duration in zip(rows, durations):
                row[-1] = duration
        return store, {
            'batch_size': batch_size, 'columns': list(ingest_growth.COLUMNS), 'batches': rows,
            'abouts': sorted(abouts, key=abouts.get), 'entries': sum(row[2] for row in rows),
            'relations': 0, 'writes': 0, 'reviews': 0, 'write_ms': [], 'ingest_ms': round(ingest_ms, 3),
            'writes_ms': 0.0, 'server': protocol.get('server'), 'cpus_allowed': cpus_allowed,
            'pinning': pinning.as_dict(), 'loadavg': probes.loadavg()}
    except BaseException:
        shutil.rmtree(store.root, ignore_errors=True)
        raise


def build_ingest(cache, corpus, lock_entry, writer, batch_size, cpus, timeout_s):
    material = ingest_material(corpus, lock_entry, writer, batch_size)
    staging = cache.stage(cachekey.store_key(**material))
    store = None
    try:
        store, profile = ingest_corpus(writer, corpus, batch_size, cache.layout.scratch(), cpus, timeout_s)
        summary, exported = binaries.export_bundle(writer, store, staging / BUNDLE, timeout_s)
        header = binaries.bundle_header(staging / BUNDLE)
        if header['content_digest'] != summary['content_digest']:
            raise synth_build.BuildFailed('export summary and bundle header disagree')
        copy_template(store.data_dir, staging / TEMPLATE)
        timings = {'ingest': profile['ingest_ms'], 'export': exported.wall_ms, 'import': None,
                   'copy': synth_build._copy_ms(cache, staging / TEMPLATE)}
        checks = {'receipts_accepted': len(profile['batches']), 'export_data_dir': 'confirmed',
                  'template_excludes': ['logs']}
        record = synth_build._record(material, label(corpus, batch_size), header, staging, timings,
                                     {'import': 'built by ingest over MCP, not imported'}, profile, checks)
        return cache.commit(staging, record)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    finally:
        if store is not None:
            shutil.rmtree(store.root, ignore_errors=True)


def build_import(cache, corpus, lock_entry, writer, reader, source, batch_size, timeout_s):
    header = binaries.bundle_header(source.bundle)
    material = import_material(corpus, lock_entry, writer, reader, header, batch_size)
    staging = cache.stage(cachekey.store_key(**material))
    store = create_store(cache.layout.scratch(), f'public-import-{corpus.name}')
    try:
        report, imported = binaries.import_bundle(reader, store, source.bundle, timeout_s)
        again, exported = binaries.export_bundle(reader, store, store.root / 'reexport.jsonl', timeout_s)
        if again['content_digest'] != header['content_digest'] or again['event_count'] != header['event_count']:
            raise synth_build.BuildFailed('the imported store does not re-export the bundle it was built from')
        shutil.copyfile(source.bundle, staging / BUNDLE)
        copy_template(store.data_dir, staging / TEMPLATE)
        timings = {'ingest': None, 'export': exported.wall_ms, 'import': imported.wall_ms,
                   'copy': synth_build._copy_ms(cache, staging / TEMPLATE)}
        checks = {'source_store_key': source.record.store_key, 'reexport_content_digest': 'equal',
                  'events_imported': report['events_imported'],
                  'mutations_applied': report.get('mutations_applied'), 'template_excludes': ['logs']}
        record = synth_build._record(material, label(corpus, batch_size) + ' imported', header, staging,
                                     timings, {'ingest': f'imported from the bundle of store '
                                                         f'{source.record.store_key}'}, None, checks)
        return cache.commit(staging, record)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    finally:
        shutil.rmtree(store.root, ignore_errors=True)


def build_public(corpus, lock_entry, writer, layout, reader=None, batch_size=DEFAULT_BATCH,
                 cpus=synth_build.BUILD_CPUS, timeout_s=synth_build.BUILD_TIMEOUT_S, log=sys.stderr):
    """{'ingest': CachedStore, 'import': CachedStore, 'built': [...]}: found in the cache or built."""
    if corpus.private and not layout.private:
        raise synth_build.BuildFailed(f'{corpus.name} is private: build it in the private layout')
    cache = StoreCache(layout)
    reader = reader or writer
    built = []
    ingested = cache.get(cachekey.store_key(**ingest_material(corpus, lock_entry, writer, batch_size)))
    if ingested is None:
        print(f'public build: ingesting {label(corpus, batch_size)}', file=log, flush=True)
        ingested = build_ingest(cache, corpus, lock_entry, writer, batch_size, cpus, timeout_s)
        built.append('ingest')
    header = binaries.bundle_header(ingested.bundle)
    imported = cache.get(cachekey.store_key(**import_material(corpus, lock_entry, writer, reader,
                                                              header, batch_size)))
    if imported is None:
        imported = build_import(cache, corpus, lock_entry, writer, reader, ingested, batch_size, timeout_s)
        built.append('import')
    return {'ingest': ingested, 'import': imported, 'built': built}


def timings(built):
    """The load timings a report quotes, in ms, from the two store.json records."""
    ingest, imported = built['ingest'].record, built['import'].record
    profile = ingest.ingest or {}
    return {'entries': profile.get('entries'), 'batches': len(profile.get('batches') or []),
            'ingest_ms': ingest.timings_ms['ingest'], 'export_ms': ingest.timings_ms['export'],
            'import_ms': imported.timings_ms['import'], 'reexport_ms': imported.timings_ms['export'],
            'copy_ms': imported.timings_ms['copy'], 'template_bytes': imported.template['bytes'],
            'bundle_bytes': imported.bundle['bytes'], 'event_count': imported.event_count,
            'content_digest': imported.content_digest, 'ingest_key': ingest.store_key,
            'import_key': imported.store_key}
