"""The B-real freeze directory: one dated, private copy of the real store and its labeling.

Layout (`<private root>/<YYYY-MM-DD>/`, never inside the repository):

    store/                 a complete KMP data directory: FORMAT_VERSION and store/kernel.sqlite3
    bundle.jsonl           `kmp-mcp export` of that store (content_digest, rebuilds for other binaries)
    reference/kmp-mcp      the pinned reference binary that builds the kernel part of every pool
    freeze.json            kmp.bench.breal.freeze.v1: hashes, integrity, content digest, reference
    questions.jsonl        kmp.bench.breal.intake.v1: the real asks with facet templates
    labeling-rules.json    kmp.bench.breal.rules.v1, hashed and registered before the first pool
    pools/<id>.json        kmp.bench.breal.pool.v1: shuffled, source-free pool of each question
    pools/_provenance/     which source put each ref in a pool; never shown while labeling
    labels/<labeler>/      kmp.bench.breal.label.v1, one file per question
    labels/adjudicated/    Tirso's resolution of disagreements
    agreement.json         kmp.bench.breal.agreement.v1: aggregates only
    gold.jsonl             kmp.bench.question.v1, corpus b-real: what a runner reads

The store copy is made by the operator (`cp -a` of the data directory while no
writer runs); `freeze` adopts it: it checks the layout and `PRAGMA
integrity_check`, exports the bundle through the reference binary on a
disposable copy (the frozen files are never opened for writing), copies the
reference binary in and records every hash. Every write goes through
`FreezeDir.path`, which refuses anything inside the repository.
"""
from dataclasses import dataclass
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile

from ..domain import jsonl
from ..domain.cachekey import canonical_json
from ..domain.errors import BenchError
from .store_snapshot import LIVE_STORE_PARTS, read_store

FREEZE_SCHEMA = 'kmp.bench.breal.freeze.v1'
REGISTRY_NAME = 'rules-registry.jsonl'  # shared with the negative rules (negative_rules.py)
DATABASE = Path('store') / 'kernel.sqlite3'


class FreezeInvalid(BenchError):
    """The freeze directory is missing a part, fails a hash, or would land in the repository."""
    code = 'BREAL_FREEZE_INVALID'


def sha256_file(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def now_utc(now=None):
    return (now or datetime.datetime.now(datetime.timezone.utc)).strftime('%Y-%m-%dT%H:%M:%SZ')


@dataclass(frozen=True)
class FreezeDir:
    root: Path

    @classmethod
    def at(cls, root, repo_root=jsonl.REPO_ROOT):
        root = jsonl.guard_private_path(Path(root).expanduser(), repo_root).resolve()
        if any(part in str(root) for part in LIVE_STORE_PARTS):
            raise FreezeInvalid(f'{root} is inside a live KMP location')
        return cls(root)

    def path(self, *parts):
        """A path inside the freeze; the one door every writer goes through."""
        return self.inside(self.root / Path(*parts))

    def inside(self, target):
        target = Path(target).resolve()
        if target != self.root and self.root not in target.parents:
            raise FreezeInvalid(f'{target} escapes the freeze directory {self.root}')
        return jsonl.guard_private_path(target)

    @property
    def private_root(self):
        return self.root.parent

    data_dir = property(lambda self: self.path('store'))
    database = property(lambda self: self.path('store', DATABASE))
    bundle = property(lambda self: self.path('bundle.jsonl'))
    reference = property(lambda self: self.path('reference', 'kmp-mcp'))
    manifest = property(lambda self: self.path('freeze.json'))
    questions = property(lambda self: self.path('questions.jsonl'))
    rules = property(lambda self: self.path('labeling-rules.json'))
    agreement = property(lambda self: self.path('agreement.json'))
    gold = property(lambda self: self.path('gold.jsonl'))

    def pool(self, question_id):
        return self.path('pools', f'{question_id}.json')

    def provenance(self, question_id):
        return self.path('pools', '_provenance', f'{question_id}.json')

    def label(self, labeler, question_id):
        return self.path('labels', labeler, f'{question_id}.json')

    def labelers(self):
        base = self.root / 'labels'
        return sorted(p.name for p in base.iterdir() if p.is_dir()) if base.is_dir() else []

    def write_json(self, target, value, overwrite=True):
        target = self.inside(target)
        target.parent.mkdir(parents=True, exist_ok=True)
        text = json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + '\n'
        with target.open('w' if overwrite else 'x', encoding='utf-8') as handle:
            handle.write(text)
        return target

    def write_text(self, target, text):
        target = self.inside(target)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding='utf-8')
        return target

    def read_manifest(self):
        try:
            return json.loads(self.manifest.read_text(encoding='utf-8'))
        except (OSError, ValueError) as failure:
            raise FreezeInvalid(f'no readable freeze.json in {self.root}: run `real freeze` first') from failure


def check_data_dir(data_dir):
    data_dir = Path(data_dir)
    version = data_dir / 'FORMAT_VERSION'
    if not version.is_file() or not (data_dir / DATABASE).is_file():
        raise FreezeInvalid(f'{data_dir} is not a KMP data directory (FORMAT_VERSION and store/kernel.sqlite3)')
    wal = data_dir / 'store' / 'kernel.sqlite3-wal'
    if wal.exists() and wal.stat().st_size:
        raise FreezeInvalid(f'{wal} is not empty: copy the store again with no writer running')
    return version.read_text(encoding='utf-8').strip()


def integrity(database):
    """`PRAGMA integrity_check` on an immutable read-only open (writes nothing, not even -shm)."""
    try:
        connection = sqlite3.connect(f'file:{database}?mode=ro&immutable=1', uri=True)
        try:
            rows = connection.execute('PRAGMA integrity_check').fetchall()
        finally:
            connection.close()
    except sqlite3.Error as failure:
        raise FreezeInvalid(f'{database}: {failure}') from failure
    return 'ok' if rows == [('ok',)] else '; '.join(str(row[0]) for row in rows[:5])


def binary_version(binary):
    try:
        done = subprocess.run([str(binary), '--version'], capture_output=True, text=True, timeout=30,
                              env={'PATH': '/usr/bin:/bin'})
    except (OSError, subprocess.TimeoutExpired) as failure:
        raise FreezeInvalid(f'{binary} --version failed: {failure}') from failure
    return done.stdout.strip() or None


def export_bundle(binary, data_dir, bundle, work_dir):
    """`kmp-mcp export` on a disposable copy; returns the export's JSON summary."""
    Path(work_dir).mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix='breal-export-', dir=work_dir))
    try:
        copy = root / 'data'
        shutil.copytree(data_dir, copy)
        env = {'PATH': '/usr/bin:/bin', 'KMP_MCP_DATA_DIR': str(copy), 'KMP_MCP_BACKEND': 'embedded',
               'KMP_VIEWER_ADDR': 'off'}
        for key, name in (('HOME', 'home'), ('XDG_DATA_HOME', 'xdg-data'), ('XDG_CONFIG_HOME', 'xdg-config'),
                          ('XDG_CACHE_HOME', 'xdg-cache'), ('CODEX_HOME', 'codex-home')):
            (root / name).mkdir()
            env[key] = str(root / name)
        partial = root / 'bundle.jsonl'
        done = subprocess.run([str(binary), 'export', str(partial)], capture_output=True, text=True,
                              timeout=600, env=env, cwd=root)
        if done.returncode != 0:
            raise FreezeInvalid(f'export failed ({done.returncode}): {done.stderr.strip()[:300]}')
        summary = json.loads(done.stdout.strip().splitlines()[-1])
        Path(bundle).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(partial, bundle)
        return summary
    finally:
        shutil.rmtree(root, ignore_errors=True)


def freeze(freeze_dir, binary, work_dir, source_note=None, now=None):
    """Adopt `freeze_dir/store` (already copied) and write freeze.json; returns the manifest."""
    fd = freeze_dir
    fmt = check_data_dir(fd.data_dir)
    status = integrity(fd.database)
    if status != 'ok':
        raise FreezeInvalid(f'integrity_check: {status}')
    binary = Path(binary).resolve()
    reference = fd.reference
    if reference.exists() and sha256_file(reference) != sha256_file(binary):
        raise FreezeInvalid(f'{reference} already pins another binary')
    reference.parent.mkdir(parents=True, exist_ok=True)
    if not reference.exists():
        shutil.copy2(binary, reference)
    summary = export_bundle(reference, fd.data_dir, fd.bundle, work_dir)
    snapshot = read_store(fd.data_dir)
    manifest = {'schema': FREEZE_SCHEMA, 'date': fd.root.name, 'frozen_at': now_utc(now),
                'source': source_note,
                'store': {'format_version': fmt, 'kernel_sha256': sha256_file(fd.database),
                          'integrity_check': status, 'snapshot_digest': snapshot.digest(),
                          'abouts': [about.about for about in snapshot.abouts],
                          'entries': sum(len(about.entries) for about in snapshot.abouts)},
                'bundle': {'sha256': sha256_file(fd.bundle), 'content_digest': summary.get('content_digest'),
                           'event_count': summary.get('event_count'), 'bundle_format': 3},
                'reference': {'sha256': sha256_file(reference), 'version': binary_version(reference),
                              'path': 'reference/kmp-mcp'}}
    fd.write_json(fd.manifest, manifest)
    return manifest


def verify(freeze_dir):
    """The manifest, after checking the store and the reference binary still hash as recorded."""
    manifest = freeze_dir.read_manifest()
    if sha256_file(freeze_dir.database) != manifest['store']['kernel_sha256']:
        raise FreezeInvalid('store/store/kernel.sqlite3 changed since the freeze')
    if sha256_file(freeze_dir.reference) != manifest['reference']['sha256']:
        raise FreezeInvalid('reference/kmp-mcp changed since the freeze')
    return manifest


# --- rules registry (BENCH_SPEC section 2, principle 2) -----------------------------------------

def register(private_root, rules_sha256, rules_version, now=None):
    """Append a hash to `<private root>/rules-registry.jsonl` (the registry the negatives use)."""
    registry = jsonl.guard_private_path(Path(private_root) / REGISTRY_NAME)
    registry.parent.mkdir(parents=True, exist_ok=True)
    entry = {'registered_at': now_utc(now), 'rules_sha256': rules_sha256, 'rules_version': rules_version}
    with registry.open('a', encoding='utf-8') as handle:
        handle.write(canonical_json(entry) + '\n')
    return entry


def registrations(private_root):
    registry = Path(private_root) / REGISTRY_NAME
    if not registry.is_file():
        return []
    return [json.loads(line) for line in registry.read_text(encoding='utf-8').splitlines() if line.strip()]


def first_registration(private_root, rules_version=None, rules_sha256=None):
    """The earliest registration matching the version and/or the hash, or None."""
    rows = [row for row in registrations(private_root)
            if (rules_version is None or row.get('rules_version') == rules_version)
            and (rules_sha256 is None or row.get('rules_sha256') == rules_sha256)]
    return min(rows, key=lambda row: row['registered_at']) if rows else None


def file_sha256(path):
    return sha256_file(path) if os.path.isfile(path) else None
