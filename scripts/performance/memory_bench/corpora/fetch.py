"""Download public benchmark files against `datasets.lock.json` (BENCH_SPEC 4.6 and 14).

The lock pins every file by URL, SHA-256 and size and records its licence as
verified at the source, whether it may be redistributed and where it may live:

- `storage: public`  -> `<repo>/tmp/memory-bench/datasets/<dataset>/` (git-ignored);
- `storage: private` -> `$MEMORY_BENCH_PRIVATE_ROOT/datasets/<dataset>/`, outside the
  repository (LoCoMo, CC BY-NC; HotpotQA, CC BY-SA). There is no default.

`fetch` streams each file into `<name>.partial`, hashes it on the way and renames it
only when size and SHA-256 match the lock; a file already present with the right
digest is kept (`present`). Nothing is ever unpacked: adapters read archives in
place. `check_lock` enforces the licence rules on the lock itself: a non-commercial
or share-alike licence must be private and not redistributable.
"""
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import time
import urllib.request

from ..domain import cachekey
from ..domain.jsonl import REPO_ROOT, guard_private_path
from ..runtime.layout import PRIVATE_ROOT_ENV
from .public_errors import FetchFailed, LicenceRefused

LOCK_PATH = Path(__file__).with_name('datasets.lock.json')
LOCK_SCHEMA = 'kmp.bench.datasets_lock.v1'
STORAGES = ('public', 'private')
RESTRICTED = ('-NC', '-SA')  # SPDX markers of non-commercial and share-alike licences
CHUNK = 1 << 20
USER_AGENT = 'kmp-memory-bench-fetch/1'
TIMEOUT_S = 120.0


@dataclass(frozen=True)
class LockedFile:
    name: str
    url: str
    sha256: str
    bytes: int


@dataclass(frozen=True)
class LockedDataset:
    name: str
    spdx: str
    redistributable: bool
    storage: str
    files: tuple
    corpora: tuple

    def digest(self):
        """Identity of the locked bytes: what a store built from this dataset is keyed by."""
        return cachekey.digest({'dataset': self.name,
                                'files': sorted([f.name, f.sha256, f.bytes] for f in self.files)})

    def file(self, name):
        for locked in self.files:
            if locked.name == name:
                return locked
        raise FetchFailed(f'{self.name}: no locked file {name!r}')


def _dataset(name, raw):
    try:
        licence = raw['licence']
        files = tuple(LockedFile(item['name'], item['url'], cachekey.require_hex64(item['sha256'], 'sha256'),
                                 int(item['bytes'])) for item in raw['files'])
        entry = LockedDataset(name, licence['spdx'], bool(raw['redistributable']), raw['storage'],
                              files, tuple(raw.get('corpora', ())))
        for key in ('verified_at', 'verified_from'):
            if not licence.get(key):
                raise FetchFailed(f'{name}: licence.{key} is required (verify the licence at the source)')
    except (KeyError, TypeError, ValueError) as failure:
        raise FetchFailed(f'{name}: malformed lock entry: {failure}') from failure
    if entry.storage not in STORAGES:
        raise FetchFailed(f'{name}: storage must be one of {", ".join(STORAGES)}')
    if len({f.name for f in files}) != len(files) or any('/' in f.name or f.name.startswith('.') for f in files):
        raise FetchFailed(f'{name}: file names must be unique plain names')
    return entry


def check_lock(entries):
    """Licence rules (BENCH_SPEC 14): NC/SA material is private and never redistributed."""
    for entry in entries.values():
        if any(marker in entry.spdx.upper() for marker in RESTRICTED):
            if entry.storage != 'private' or entry.redistributable:
                raise LicenceRefused(f'{entry.name} is {entry.spdx}: it must be private and not redistributable')
    return entries


def load_lock(path=LOCK_PATH):
    raw = json.loads(Path(path).read_text(encoding='utf-8'))
    if raw.get('schema') != LOCK_SCHEMA:
        raise FetchFailed(f'{path}: not a {LOCK_SCHEMA} file')
    return check_lock({name: _dataset(name, item) for name, item in raw['datasets'].items()})


def dataset_dir(entry, public_root=None, private_root=None, env=None, repo_root=REPO_ROOT):
    """Where `entry` lives; a private dataset needs a private root outside the repository."""
    if entry.storage == 'public':
        root = Path(public_root) if public_root else Path(repo_root) / 'tmp' / 'memory-bench'
        return root / 'datasets' / entry.name
    env = os.environ if env is None else env
    chosen = private_root or env.get(PRIVATE_ROOT_ENV)
    if not chosen:
        raise LicenceRefused(f'{entry.name} is private ({entry.spdx}): set {PRIVATE_ROOT_ENV} '
                             'or pass --private-root (outside the repository)')
    return guard_private_path(Path(chosen).expanduser(), repo_root) / 'datasets' / entry.name


def sha256_of(path):
    hasher = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(CHUNK), b''):
            hasher.update(block)
    return hasher.hexdigest()


def _download(locked, target, opener, timeout):
    partial = target.with_name(target.name + '.partial')
    hasher, size = hashlib.sha256(), 0
    request = urllib.request.Request(locked.url, headers={'User-Agent': USER_AGENT})
    try:
        with opener(request, timeout=timeout) as response, open(partial, 'wb') as out:
            for block in iter(lambda: response.read(CHUNK), b''):
                hasher.update(block)
                size += len(block)
                out.write(block)
    except OSError as failure:
        partial.unlink(missing_ok=True)
        raise FetchFailed(f'{locked.name}: download failed: {failure}') from failure
    if size != locked.bytes or hasher.hexdigest() != locked.sha256:
        partial.unlink(missing_ok=True)
        raise FetchFailed(f'{locked.name}: got {size} bytes sha256 {hasher.hexdigest()}, '
                          f'the lock says {locked.bytes} bytes sha256 {locked.sha256}')
    os.replace(partial, target)
    return size


def verify_file(locked, directory):
    path = Path(directory) / locked.name
    if not path.is_file():
        return 'missing'
    if path.stat().st_size != locked.bytes or sha256_of(path) != locked.sha256:
        return 'mismatch'
    return 'present'


def fetch(entry, directory, opener=urllib.request.urlopen, timeout=TIMEOUT_S, download=True):
    """Every locked file of `entry` in `directory`, verified; returns one row per file."""
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    rows = []
    for locked in entry.files:
        started = time.perf_counter()
        status = verify_file(locked, directory)
        if status != 'present':
            if not download:
                raise FetchFailed(f'{entry.name}/{locked.name}: {status} (run fetch)')
            if status == 'mismatch':
                (directory / locked.name).unlink()
            _download(locked, directory / locked.name, opener, timeout)
            status = 'downloaded'
        rows.append({'dataset': entry.name, 'file': locked.name, 'status': status, 'bytes': locked.bytes,
                     'sha256': locked.sha256, 'licence': entry.spdx, 'storage': entry.storage,
                     'seconds': round(time.perf_counter() - started, 3)})
    return rows


def locate(name, public_root=None, private_root=None, lock=None, download=False):
    """(LockedDataset, directory) with every file verified; downloads only when asked."""
    lock = lock or load_lock()
    if name not in lock:
        raise FetchFailed(f'unknown dataset {name!r}; locked: {", ".join(sorted(lock))}')
    entry = lock[name]
    directory = dataset_dir(entry, public_root, private_root)
    fetch(entry, directory, download=download)
    return entry, directory
