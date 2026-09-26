"""Built stores, cached by key (SCHEMAS.md section 7, `stores/<store_key>/`).

An entry is a directory named by its full store key:

  store.json     kmp.bench.store.v1: key material, KMP content digest, timings
  bundle.jsonl   the whole-store `kmp-mcp export` the entry was built from or exported to
  store/         the template: a KMP data directory every run copies, never opens

An entry is staged under `work/` and renamed into place in one step, so a reader
never sees half an entry; an existing entry is never overwritten. A template
holds the data directory without `logs/` (the server journals of the build, which
would otherwise sit in front of every run's own journal).
"""
from dataclasses import dataclass
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import tempfile
import time

from ...token_harness.native.capture import sha256_file
from ...token_harness.native.isolation import tree_digest
from ..domain import cachekey
from ..domain.errors import BenchError

SCHEMA = 'kmp.bench.store.v1'
RECORD = 'store.json'
BUNDLE = 'bundle.jsonl'
TEMPLATE = 'store'
STAGING_PREFIX = 'staging-'
EXCLUDED = ('logs',)  # server journals: per process, never part of a template
TIMINGS = ('ingest', 'export', 'import', 'copy')
FIELDS = ('schema', 'store_key', 'material', 'label', 'content_digest', 'event_count', 'bundle',
          'template', 'timings_ms', 'absent', 'ingest', 'checks', 'created_at')


class StoreCacheError(BenchError):
    code = 'STORE_CACHE_INVALID'


def now():
    return datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')


@dataclass(frozen=True)
class StoreRecord:
    """kmp.bench.store.v1; `material` recomputes `store_key` through cachekey.store_key."""
    store_key: str
    material: dict
    label: str
    content_digest: str
    event_count: int
    bundle: dict  # {sha256, bytes, header}
    template: dict  # {tree_sha256, bytes}
    timings_ms: dict  # ingest, export, import, copy: ms or None
    absent: dict  # timing -> why it is None
    ingest: dict | None  # the ingest profile of a build == 'ingest' entry
    checks: dict  # what the builder verified before committing
    created_at: str

    def __post_init__(self):
        cachekey.require_hex64(self.store_key, 'store_key')
        if cachekey.store_key(**self.material) != self.store_key:
            raise StoreCacheError('store.json material does not reproduce its store_key')
        if not cachekey.CONTENT_DIGEST.fullmatch(self.content_digest or ''):
            raise StoreCacheError(f'content_digest {self.content_digest!r} is not sha256:<hex>')
        if set(self.timings_ms) != set(TIMINGS):
            raise StoreCacheError(f'timings_ms must name exactly {", ".join(TIMINGS)}')
        missing = sorted(name for name, value in self.timings_ms.items()
                         if value is None and name not in self.absent)
        if missing or set(self.absent) - {n for n, v in self.timings_ms.items() if v is None}:
            raise StoreCacheError('absent must explain exactly the null timings')

    def as_dict(self):
        return {'schema': SCHEMA, **{name: getattr(self, name) for name in FIELDS[1:]}}

    @classmethod
    def from_dict(cls, data):
        if not isinstance(data, dict) or data.get('schema') != SCHEMA:
            raise StoreCacheError(f'not a {SCHEMA} record')
        if set(data) != set(FIELDS):
            raise StoreCacheError(f'store.json fields differ: {sorted(set(data) ^ set(FIELDS))}')
        return cls(**{name: data[name] for name in FIELDS[1:]})


@dataclass(frozen=True)
class CachedStore:
    directory: Path
    record: StoreRecord

    @property
    def template(self):
        return self.directory / TEMPLATE

    @property
    def bundle(self):
        return self.directory / BUNDLE


def copy_template(source, destination):
    """Copy a data directory without its journals; returns the wall ms of the copy."""
    started = time.perf_counter_ns()
    shutil.copytree(source, destination, symlinks=False,
                    ignore=lambda folder, names: [n for n in names if Path(folder) == Path(source)
                                                  and n in EXCLUDED])
    return (time.perf_counter_ns() - started) / 1e6


def tree_bytes(folder):
    return sum(path.stat().st_size for path in Path(folder).rglob('*') if path.is_file())


class StoreCache:
    def __init__(self, layout):
        self.layout = layout

    def get(self, key):
        """The committed entry for `key`, or None; a malformed entry is an error, not a miss."""
        directory = self.layout.store(key)
        if not directory.is_dir():
            return None
        try:
            record = StoreRecord.from_dict(json.loads((directory / RECORD).read_text(encoding='utf-8')))
        except (OSError, ValueError) as failure:
            raise StoreCacheError(f'{directory}: unreadable {RECORD}: {failure}') from failure
        if record.store_key != key:
            raise StoreCacheError(f'{directory}: store.json names another key')
        if not (directory / TEMPLATE).is_dir() or not (directory / BUNDLE).is_file():
            raise StoreCacheError(f'{directory}: entry without its template or bundle')
        return CachedStore(directory, record)

    def stage(self, name):
        """A fresh staging directory under work/ (named after a key's first 16 characters);
        commit() moves it into place, gc() removes an abandoned one."""
        scratch = self.layout.require_inside(self.layout.scratch())
        scratch.mkdir(parents=True, exist_ok=True)
        return Path(tempfile.mkdtemp(prefix=f'{STAGING_PREFIX}{name[:16]}-', dir=scratch))

    def commit(self, staging, record):
        """Write store.json into `staging` and rename it to stores/<key>/ (never overwrites)."""
        staging = Path(staging)
        if not (staging / TEMPLATE).is_dir() or not (staging / BUNDLE).is_file():
            raise StoreCacheError('a staged entry needs store/ and bundle.jsonl')
        if sha256_file(staging / BUNDLE) != record.bundle['sha256']:
            raise StoreCacheError('staged bundle does not match its record')
        (staging / RECORD).write_text(json.dumps(record.as_dict(), indent=1, sort_keys=True,
                                                 ensure_ascii=False) + '\n', encoding='utf-8')
        target = self.layout.require_inside(self.layout.store(record.store_key))
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists():
            raise StoreCacheError(f'{target} already exists; a cache entry is never overwritten')
        os.rename(staging, target)
        return self.get(record.store_key)

    def materialize(self, key, destination):
        """Copy the template of `key` to `destination` (new); returns (path, copy ms)."""
        entry = self.get(key)
        if entry is None:
            raise StoreCacheError(f'no cached store {key}')
        destination = self.layout.require_inside(Path(destination))
        if destination.exists():
            raise StoreCacheError(f'{destination} already exists')
        return destination, copy_template(entry.template, destination)

    def entries(self):
        folder = self.layout.root / 'stores'
        if not folder.is_dir():
            return []
        found = []
        for directory in sorted(p for p in folder.iterdir() if p.is_dir()):
            try:
                entry = self.get(directory.name)
            except (StoreCacheError, BenchError) as failure:
                found.append((directory, None, str(failure)))
                continue
            found.append((directory, entry, None))
        return found

    def gc(self):
        """Remove abandoned staging directories under work/; returns what was removed."""
        scratch = self.layout.scratch()
        if not scratch.is_dir():
            return []
        removed = sorted(p for p in scratch.iterdir() if p.is_dir() and p.name.startswith(STAGING_PREFIX))
        for path in removed:
            shutil.rmtree(path, ignore_errors=True)
        return removed


def template_summary(folder):
    return {'tree_sha256': tree_digest(folder), 'bytes': tree_bytes(folder)}


def bundle_summary(path, header):
    keep = ('bundle_format', 'event_format', 'kernel_version', 'snapshot_id', 'event_count',
            'content_digest')
    return {'sha256': sha256_file(path), 'bytes': Path(path).stat().st_size,
            'header': {name: header.get(name) for name in keep}}
