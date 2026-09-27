"""Canonical digests and cache keys (BENCH_SPEC section 8, SCHEMAS.md section 6).

Every key is the SHA-256 of canonical JSON — sorted keys, no whitespace,
UTF-8, no NaN — over a `kind`, a version and the key material, so two kinds
of key can never collide. Results, reports and worlds carry the bench version,
so a bench version bump invalidates them at once. A store does not: what it
holds depends on its source, N, format, reader, writer and batch, never on how
answers are scored, so its key carries `STORE_KEY_VERSION` instead. Digests are lowercase hex without a prefix; KMP's own
`content_digest` keeps its `sha256:` prefix verbatim.
"""
import hashlib
import json
import math
import re

from .. import BENCH_VERSION
from .errors import CacheKeyInvalid

HEX64 = re.compile(r'[0-9a-f]{64}')
CONTENT_DIGEST = re.compile(r'sha256:[0-9a-f]{64}')
MODE = re.compile(r'[a-z0-9][a-z0-9-]{0,31}')
BUILDS = ('ingest', 'import', 'copy')
# The version inside every store key. Frozen at v1, the bench version the cached stores of
# 25-26 Sept were built under: the v2 (scoring rules) and v3 (ref reading) bumps did not
# change what a store holds, so those stores keep their keys. Bump it only when what a
# store holds or how it is built changes meaning.
STORE_KEY_VERSION = 'kmp.memory_bench.v1'
# Run manifests must agree on these for two runs to be compared at all (section 8, anti-drift).
COMPARABILITY_FIELDS = ('questions_digest', 'driver_version', 'encoders', 'bench_version')


def _check(value, where):
    if value is None or isinstance(value, (bool, str, int)):
        return
    if isinstance(value, float):
        if not math.isfinite(value):
            raise CacheKeyInvalid(f'{where}: non-finite number')
        return
    if isinstance(value, (list, tuple)):
        for index, item in enumerate(value):
            _check(item, f'{where}[{index}]')
        return
    if isinstance(value, dict):
        for key, item in value.items():
            if not isinstance(key, str):
                raise CacheKeyInvalid(f'{where}: non-string key {key!r}')
            _check(item, f'{where}.{key}')
        return
    raise CacheKeyInvalid(f'{where}: {type(value).__name__} is not JSON')


def canonical_json(value):
    _check(value, '$')
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':'), allow_nan=False)


def sha256_hex(data):
    return hashlib.sha256(data).hexdigest()


def digest(value):
    return sha256_hex(canonical_json(value).encode('utf-8'))


def require_hex64(value, name):
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        raise CacheKeyInvalid(f'{name} must be 64 lowercase hex characters, got {value!r}')
    return value


def _key(kind, material, bench_version):
    return digest({'kind': kind, 'bench_version': bench_version, 'material': material})


def ordered_digest(pairs):
    """One digest over (id, digest) pairs sorted by id; ids must be unique."""
    pairs = sorted(pairs)
    ids = [identifier for identifier, _ in pairs]
    if len(set(ids)) != len(ids):
        raise CacheKeyInvalid('duplicate id in a digested collection')
    hasher = hashlib.sha256()
    for identifier, item_digest in pairs:
        hasher.update(identifier.encode('utf-8') + b'\0' + require_hex64(item_digest, identifier).encode() + b'\0')
    return hasher.hexdigest()


def result_key(*, binary_sha256, config_digest, store_key, questions_digest, mode,
               bench_version=BENCH_VERSION, nonce=None):
    """Section 8: (binary sha, config digest, store key, questions digest, mode, bench version).

    `nonce` is only for runs that must never be reused (jev='record').
    """
    if not isinstance(mode, str) or not MODE.fullmatch(mode):
        raise CacheKeyInvalid(f'mode {mode!r} is not a mode name')
    material = {'binary_sha256': require_hex64(binary_sha256, 'binary_sha256'),
                'config_digest': require_hex64(config_digest, 'config_digest'),
                'store_key': require_hex64(store_key, 'store_key'),
                'questions_digest': require_hex64(questions_digest, 'questions_digest'),
                'mode': mode}
    if nonce is not None:
        material['nonce'] = str(nonce)
    return _key('result', material, bench_version)


def world_key(*, generator, generator_version, seed, topology, max_level, block_size,
              bench_version=BENCH_VERSION):
    for name, value in (('seed', seed), ('max_level', max_level), ('block_size', block_size)):
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            raise CacheKeyInvalid(f'{name} must be a non-negative integer')
    return _key('world', {'generator': generator, 'generator_version': generator_version,
                          'seed': seed, 'topology': topology, 'max_level': max_level,
                          'block_size': block_size}, bench_version)


def synth_source(*, generator, generator_version, seed, topology, world_digest):
    return {'kind': 'synth', 'generator': generator, 'generator_version': generator_version,
            'seed': seed, 'topology': topology,
            'world_digest': require_hex64(world_digest, 'world_digest')}


def bundle_source(*, content_digest, label):
    """A store rebuilt from a KMP bundle (real-store copy, judged corpus, public dataset)."""
    if not isinstance(content_digest, str) or not CONTENT_DIGEST.fullmatch(content_digest):
        raise CacheKeyInvalid(f'content_digest must be sha256:<hex>, got {content_digest!r}')
    return {'kind': 'bundle', 'content_digest': content_digest, 'label': label}


def dataset_source(*, corpus, lock_sha256, selection_digest, adapter_version):
    """A store loaded from a public dataset (BT17): the locked files, the subset and the adapter."""
    return {'kind': 'dataset', 'corpus': corpus,
            'lock_sha256': require_hex64(lock_sha256, 'lock_sha256'),
            'selection_digest': require_hex64(selection_digest, 'selection_digest'),
            'adapter_version': adapter_version}


def store_key(*, source, n, bundle_format, reader_sha256, build, writer_sha256=None,
              batch_size=None, store_mode='shared'):
    """BT14: generator, version, seed, topology, N, format and the reader's sha are all in;
    the bench version is not (`STORE_KEY_VERSION`)."""
    if not isinstance(source, dict) or source.get('kind') not in ('synth', 'bundle', 'dataset'):
        raise CacheKeyInvalid('store source must come from synth_source(), bundle_source() '
                              'or dataset_source()')
    if build not in BUILDS:
        raise CacheKeyInvalid(f'build {build!r} is not one of {", ".join(BUILDS)}')
    if store_mode not in ('shared', 'own'):
        raise CacheKeyInvalid(f'store_mode {store_mode!r} is not shared or own')
    material = {'source': source, 'n': n, 'bundle_format': bundle_format,
                'reader_sha256': require_hex64(reader_sha256, 'reader_sha256'), 'build': build,
                'writer_sha256': None if writer_sha256 is None else require_hex64(writer_sha256, 'writer_sha256'),
                'batch_size': batch_size, 'store_mode': store_mode}
    return _key('store', material, STORE_KEY_VERSION)


def report_key(*, result_keys, options, bench_version=BENCH_VERSION):
    keys = sorted(require_hex64(key, 'result_key') for key in result_keys)
    return _key('report', {'result_keys': keys, 'options': options}, bench_version)


def drift(first, second):
    """Comparability fields on which two run manifests disagree; () means comparable."""
    return tuple(name for name in COMPARABILITY_FIELDS if first.get(name) != second.get(name))
