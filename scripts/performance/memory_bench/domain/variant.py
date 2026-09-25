"""Variant TOML (kmp.bench.variant.v1, BENCH_SPEC section 8): one arm of a comparison.

A variant pre-registers what it claims (targets, guards, n) and declares how
its binary runs (store files, environment, Jev mode). The two halves digest
separately: `preregistration_digest` freezes the claim before the first run,
and `config_digest` is the part of the result cache key that changes what the
binary does. Secrets never live here: the environment is an allowlist, and a
key-like name is refused by name.
"""
from dataclasses import dataclass
import json
from pathlib import Path
import re
import tomllib

from . import cachekey
from .errors import CacheKeyInvalid, VariantInvalid
from .fields import Fields

SCHEMA = 'kmp.bench.variant.v1'
NAME = r'[a-z0-9][a-z0-9_-]{0,63}'
METRIC = r'[a-z][a-z0-9_@.]{0,63}'
GIT_REF = r'[A-Za-z0-9][A-Za-z0-9._/-]{0,127}'
STORE_FILE = r'[A-Za-z0-9][A-Za-z0-9_.-]{0,63}\.(json|kmpb)'
CLAIMS = ('parity', 'quality', 'cost')
STORES = ('shared', 'own')
JEV_MODES = ('off', 'replay', 'record')
DIRECTIONS = ('up', 'down')
ENV_ALLOWLIST = ('KMP_LEXICAL_BRIDGE', 'KMP_MCP_ENGINE', 'KMP_TYPESAFE_CASSETTE',
                 'KMP_TYPESAFE_CASSETTE_MODE', 'RUST_LOG')
PATH_ENV = ('KMP_LEXICAL_BRIDGE', 'KMP_TYPESAFE_CASSETTE')
CASSETTE_MODES = ('replay', 'record')
SECRET_HINTS = ('KEY', 'TOKEN', 'SECRET', 'PASSWORD', 'CREDENTIAL')


@dataclass(frozen=True)
class Target:
    metric: str
    direction: str | None = None
    effect: float | None = None  # smallest effect worth claiming; MDE above it is `indecidible`

    def as_dict(self):
        return {'metric': self.metric, 'direction': self.direction, 'effect': self.effect}


@dataclass(frozen=True)
class Guard:
    metric: str
    margin: float = 0.0  # tolerated worsening; 0 for every guard of section 12

    def as_dict(self):
        return {'metric': self.metric, 'margin': self.margin}


@dataclass(frozen=True)
class BinarySpec:
    path: str | None = None  # relative to the repository root, or absolute
    git_ref: str | None = None  # built in a worktree with `cargo build --release --locked`

    def as_dict(self):
        return {'path': self.path, 'git_ref': self.git_ref}


@dataclass(frozen=True)
class StoreFile:
    """A file copied into the store data directory before the binary starts."""
    name: str
    sha256: str  # of the exact bytes written
    source: Path | None = None  # file copied verbatim
    inline: str | None = None  # canonical JSON text written as UTF-8

    def content(self, read_bytes=Path.read_bytes):
        return self.inline.encode('utf-8') if self.inline is not None else read_bytes(self.source)


@dataclass(frozen=True)
class EnvValue:
    name: str
    value: str  # what the child process receives; path keys are made absolute
    content_sha256: str | None = None  # path keys: digest of the named file, None if absent

    def digest_material(self):
        if self.name not in PATH_ENV:
            return self.value
        return {'sha256': self.content_sha256} if self.content_sha256 else {'absent': True}


@dataclass(frozen=True)
class Variant:
    name: str
    claim: str
    targets: tuple
    guards: tuple
    n: int
    store: str
    jev: str
    binary: BinarySpec
    store_files: tuple = ()
    env: tuple = ()
    description: str | None = None

    def env_map(self):
        return {item.name: item.value for item in self.env}

    def preregistration(self):
        return {'schema': SCHEMA, 'name': self.name, 'claim': self.claim,
                'targets': [t.as_dict() for t in self.targets],
                'guards': [g.as_dict() for g in self.guards], 'n': self.n}

    def preregistration_digest(self):
        return cachekey.digest(self.preregistration())

    def execution_config(self):
        """Everything besides the binary and the store that changes what the binary does."""
        return {'jev': self.jev, 'store_files': {f.name: f.sha256 for f in self.store_files},
                'env': {item.name: item.digest_material() for item in self.env}}

    def config_digest(self):
        return cachekey.digest(self.execution_config())

    def as_dict(self):
        return {**self.preregistration(), 'description': self.description, 'store': self.store,
                'jev': self.jev, 'binary': self.binary.as_dict(),
                'store_files': {f.name: f.sha256 for f in self.store_files},
                'env': {item.name: item.value for item in self.env},
                'preregistration_digest': self.preregistration_digest(),
                'config_digest': self.config_digest()}


def _targets(fields, claim):
    items = fields.objects('targets')
    if not items:
        fields.fail('targets', 'pre-register at least one target metric')
    targets = []
    for index, item in enumerate(items):
        where = f'{fields.where}.targets[{index}]'
        if isinstance(item, str):
            if claim != 'parity':
                raise VariantInvalid(f'{where}: a {claim} target is a table with metric, direction, effect')
            targets.append(Target(_metric(item, where)))
            continue
        target = Fields(item, where, VariantInvalid)
        value = Target(_metric(target.text('metric'), where),
                       target.text('direction', required=claim != 'parity', choices=DIRECTIONS),
                       target.number('effect', required=claim != 'parity', minimum=0.0))
        target.done()
        if value.effect is not None and value.effect <= 0:
            raise VariantInvalid(f'{where}.effect: must be above zero')
        targets.append(value)
    return tuple(targets)


def _guards(fields):
    guards = []
    for index, item in enumerate(fields.objects('guards')):
        where = f'{fields.where}.guards[{index}]'
        if isinstance(item, str):
            guards.append(Guard(_metric(item, where)))
            continue
        guard = Fields(item, where, VariantInvalid)
        guards.append(Guard(_metric(guard.text('metric'), where), guard.number('margin', False, 0.0) or 0.0))
        guard.done()
    return tuple(guards)


def _metric(value, where):
    if not isinstance(value, str) or not re.fullmatch(METRIC, value):
        raise VariantInvalid(f'{where}: {value!r} is not a metric name')
    return value


def _binary(fields):
    table = Fields(fields.mapping('binary', required=True), fields.where + '.binary', VariantInvalid)
    binary = BinarySpec(table.text('path', required=False), table.text('git_ref', False, GIT_REF))
    table.done()
    if (binary.path is None) == (binary.git_ref is None):
        table.fail('path', 'give exactly one of path or git_ref')
    if binary.git_ref is not None and '..' in binary.git_ref:
        table.fail('git_ref', 'may not contain ..')
    return binary


def _resolve(value, base_dir):
    """Absolute, because the binary runs with its disposable store root as cwd."""
    path = Path(value)
    return (path if path.is_absolute() else Path(base_dir) / path).absolute()


def _store_files(fields, base_dir, read_bytes):
    files = []
    for name, value in sorted((fields.mapping('store_files') or {}).items()):
        where = f'{fields.where}.store_files.{name}'
        if not re.fullmatch(STORE_FILE, name):
            raise VariantInvalid(f'{where}: a store file is a bare *.json or *.kmpb name')
        if isinstance(value, dict) and set(value) == {'json'}:
            try:
                text = cachekey.canonical_json(value['json'])
            except CacheKeyInvalid as error:  # e.g. a TOML datetime, which JSON cannot hold
                raise VariantInvalid(f'{where}: inline json: {error.detail}') from error
            files.append(StoreFile(name, cachekey.sha256_hex(text.encode('utf-8')), inline=text))
            continue
        if isinstance(value, dict) and set(value) == {'path'}:
            value = value['path']
        if not isinstance(value, str) or not value:
            raise VariantInvalid(f'{where}: a path string, {{path = ...}} or {{json = ...}}')
        source = _resolve(value, base_dir)
        try:
            content = read_bytes(source)
        except OSError as error:
            raise VariantInvalid(f'{where}: cannot read {source}: {error.strerror}') from error
        if name.endswith('.json'):
            try:
                json.loads(content.decode('utf-8'))
            except ValueError as error:
                raise VariantInvalid(f'{where}: {source} is not UTF-8 JSON') from error
        files.append(StoreFile(name, cachekey.sha256_hex(content), source=source))
    return tuple(files)


def _env(fields, base_dir, read_bytes):
    items = []
    for name, value in sorted((fields.mapping('env') or {}).items()):
        where = f'{fields.where}.env.{name}'
        if any(hint in name.upper() for hint in SECRET_HINTS):
            raise VariantInvalid(f'{where}: secrets never live in a variant; pass them to the runner')
        if name not in ENV_ALLOWLIST:
            raise VariantInvalid(f'{where}: not in the allowlist ({", ".join(ENV_ALLOWLIST)})')
        if not isinstance(value, str) or not value or '\0' in value or '\n' in value:
            raise VariantInvalid(f'{where}: a non-empty single-line string')
        if name == 'KMP_TYPESAFE_CASSETTE_MODE' and value not in CASSETTE_MODES:
            raise VariantInvalid(f'{where}: replay or record')
        if name not in PATH_ENV:
            items.append(EnvValue(name, value))
            continue
        path = _resolve(value, base_dir)
        try:
            content_sha256 = cachekey.sha256_hex(read_bytes(path))
        except FileNotFoundError:
            content_sha256 = None
        except OSError as error:
            raise VariantInvalid(f'{where}: cannot read {path}: {error.strerror}') from error
        items.append(EnvValue(name, str(path), content_sha256))
    return tuple(items)


def _check_jev(variant, fields):
    env = {item.name: item for item in variant.env}
    mode = env['KMP_TYPESAFE_CASSETTE_MODE'].value if 'KMP_TYPESAFE_CASSETTE_MODE' in env else None
    cassette = env.get('KMP_TYPESAFE_CASSETTE')
    if variant.jev == 'off':
        if cassette or mode or any(f.name == 'typesafe.json' for f in variant.store_files):
            fields.fail('jev', "jev='off' sets no cassette and no typesafe.json")
    elif variant.jev == 'replay':
        if mode == 'record':
            fields.fail('jev', "jev='replay' cannot record the cassette")
        if cassette is not None and cassette.content_sha256 is None:
            fields.fail('env', f'KMP_TYPESAFE_CASSETTE {cassette.value} does not exist; replay needs it')
    elif mode == 'replay':
        fields.fail('jev', "jev='record' cannot replay the cassette")
    for item in (env.get('KMP_LEXICAL_BRIDGE'),):
        if item is not None and item.content_sha256 is None:
            fields.fail('env', f'KMP_LEXICAL_BRIDGE {item.value} does not exist')


def parse_variant(data, origin='<variant>', base_dir='.', read_bytes=Path.read_bytes):
    """Validate a parsed TOML table; relative paths resolve against `base_dir` (the repo root)."""
    fields = Fields(data, origin, VariantInvalid)
    fields.text('schema', required=False, choices=(SCHEMA,))
    claim = fields.text('claim', choices=CLAIMS)
    variant = Variant(
        name=fields.text('name', pattern=NAME), claim=claim, targets=_targets(fields, claim),
        guards=_guards(fields), n=fields.integer('n', minimum=1),
        store=fields.text('store', choices=STORES), jev=fields.text('jev', choices=JEV_MODES),
        binary=_binary(fields), store_files=_store_files(fields, base_dir, read_bytes),
        env=_env(fields, base_dir, read_bytes), description=fields.text('description', required=False))
    fields.done()
    _check_jev(variant, fields)
    return variant


def parse_variant_toml(text, origin='<variant>', base_dir='.', read_bytes=Path.read_bytes):
    try:
        data = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        raise VariantInvalid(f'{origin}: not TOML: {error}') from error
    return parse_variant(data, origin, base_dir, read_bytes)


def load_variant(path, repo_root):
    path = Path(path)
    return parse_variant_toml(path.read_text(encoding='utf-8'), str(path), repo_root)
