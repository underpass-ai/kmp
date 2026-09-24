"""Verify a journey directory against its manifest before any content is interpreted.

Layout written by acceptance_surface_journeys.py: `manifest.json` lists every
original file with its uncompressed length and SHA-256; `.json`/`.jsonl`
originals are stored as `<name>.gz`. The manifest itself carries no root hash,
so its own SHA-256 is recorded by the caller for provenance.
"""
from dataclasses import dataclass
import gzip
import hashlib
import os
from pathlib import Path, PurePosixPath
import stat

from ..domain.errors import CaptureIntegrityError
from . import strict_json

MANIFEST = 'manifest.json'
DEFAULT_MAX_FILE_BYTES = 256 * 1024 * 1024
MAX_MANIFEST_BYTES = 16 * 1024 * 1024


@dataclass(frozen=True)
class VerifiedRun:
    root: Path
    manifest: dict
    manifest_sha256: str
    files: dict  # original name -> verified uncompressed bytes


def _open_regular(path):
    """Open without following symlinks; only regular files are accepted."""
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    except OSError as error:
        raise CaptureIntegrityError(f'cannot open {path.name}: {error.strerror}') from error
    if not stat.S_ISREG(os.fstat(descriptor).st_mode):
        os.close(descriptor)
        raise CaptureIntegrityError('not a regular file: ' + path.name)
    return os.fdopen(descriptor, 'rb')


def _safe_name(name):
    if not isinstance(name, str) or not name or PurePosixPath(name).name != name \
            or name in ('.', '..') or '\\' in name or '\0' in name:
        raise CaptureIntegrityError('manifest path outside the run root: ' + repr(name))
    return name


def _entries(root):
    with os.scandir(root) as listing:
        entries = {entry.name: entry for entry in listing}
    for name, entry in entries.items():
        if entry.is_symlink():
            raise CaptureIntegrityError('symlink in capture: ' + name)
        if not entry.is_file(follow_symlinks=False):
            raise CaptureIntegrityError('unexpected non-file entry: ' + name)
    return entries


def _read_manifest(root):
    with _open_regular(root / MANIFEST) as source:
        raw = source.read(MAX_MANIFEST_BYTES + 1)
    if len(raw) > MAX_MANIFEST_BYTES:
        raise CaptureIntegrityError('manifest larger than its limit')
    try:
        manifest = strict_json.loads(raw.decode('utf-8'))
    except ValueError as error:  # JSONDecodeError and UnicodeDecodeError
        raise CaptureIntegrityError('manifest is not valid UTF-8 JSON: ' + str(error)) from error
    listed = manifest.get('uncompressed_files') if isinstance(manifest, dict) else None
    if not isinstance(listed, dict):
        raise CaptureIntegrityError('manifest has no uncompressed_files table')
    return manifest, hashlib.sha256(raw).hexdigest()


def _read_original(root, stored, expected_bytes, max_file_bytes):
    if expected_bytes > max_file_bytes:
        raise CaptureIntegrityError(f'{stored} declares {expected_bytes} bytes, limit {max_file_bytes}')
    with _open_regular(root / stored) as source:
        if not stored.endswith('.gz'):
            return source.read(max_file_bytes + 1)
        with gzip.GzipFile(fileobj=source) as expanded:
            try:
                raw = expanded.read(expected_bytes + 1)
            except (OSError, EOFError) as error:
                raise CaptureIntegrityError(f'corrupt gzip {stored}: {error}') from error
    if len(raw) > expected_bytes:
        raise CaptureIntegrityError(f'gzip expansion of {stored} exceeds its declared length')
    return raw


def verify_run(root, max_file_bytes=DEFAULT_MAX_FILE_BYTES):
    """Return verified originals, or raise on the first integrity violation."""
    root = Path(root)
    if root.is_symlink() or not root.is_dir():
        raise CaptureIntegrityError('run root is not a plain directory: ' + str(root))
    manifest, manifest_sha256 = _read_manifest(root)
    entries = _entries(root)
    claimed, files = {MANIFEST: MANIFEST}, {}
    for name, expected in manifest['uncompressed_files'].items():
        _safe_name(name)
        if not isinstance(expected, dict) or not isinstance(expected.get('bytes'), int) \
                or not isinstance(expected.get('sha256'), str):
            raise CaptureIntegrityError('malformed manifest entry: ' + name)
        present = [stored for stored in (name, name + '.gz') if stored in entries]
        if len(present) != 1:
            reason = 'missing' if not present else 'stored twice (raw and gzip)'
            raise CaptureIntegrityError(f'{name} is {reason}')
        stored = present[0]
        if stored in claimed:
            raise CaptureIntegrityError(f'duplicate entry: {stored} claimed by {claimed[stored]} and {name}')
        claimed[stored] = name
        raw = _read_original(root, stored, expected['bytes'], max_file_bytes)
        if len(raw) != expected['bytes']:
            raise CaptureIntegrityError(f'length mismatch: {name}')
        if hashlib.sha256(raw).hexdigest() != expected['sha256']:
            raise CaptureIntegrityError(f'hash mismatch: {name}')
        files[name] = raw
    unlisted = sorted(set(entries) - set(claimed))
    if unlisted:
        raise CaptureIntegrityError('files not covered by the manifest: ' + ', '.join(unlisted))
    return VerifiedRun(root, manifest, manifest_sha256, files)
