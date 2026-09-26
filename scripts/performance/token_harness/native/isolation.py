"""Disposable stores whose location is validated before the first write (annex A.9).

The child environment is built from an allowlist, never inherited wholesale:
HOME, XDG_* and the KMP data directory all point inside a fresh temporary
directory. The binary's own startup log must then confirm that it resolved
exactly that directory by the explicit `env` rule before any tool is called.

A caller may add a variant's configuration, but only the names in
`VARIANT_ENV_ALLOWLIST`, passed explicitly; any other name is refused. Secrets
(only `TYPESAFE_API_KEY`) reach the child but never `allowlisted_env()`, so they
never reach a manifest. A store may start as a copy of a template (an already
built store); the copy lives inside the disposable root and is still confirmed.
"""
from dataclasses import dataclass, field
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile

from ..domain.errors import HarnessError

PASSTHROUGH = ('PATH', 'LANG', 'LC_ALL')
DATA_DIR_EVENT = 'embedded backend data dir resolved'
HARNESS_ENV = ('KMP_MCP_DATA_DIR', 'KMP_MCP_BACKEND', 'KMP_VIEWER_ADDR')
VARIANT_ENV_ALLOWLIST = ('KMP_LEXICAL_BRIDGE', 'KMP_TYPESAFE_CASSETTE',
                         'KMP_TYPESAFE_CASSETTE_MODE', 'KMP_MCP_ENGINE', 'RUST_LOG',
                         'KMP_JUDGEMENT_DEADLINES')
PATH_VALUED_ENV = ('KMP_LEXICAL_BRIDGE', 'KMP_TYPESAFE_CASSETTE')
SECRET_ENV_ALLOWLIST = ('TYPESAFE_API_KEY',)
# The binary's default filter; the store confirmation and `kmp_mcp_tool` events need it.
# A variant's RUST_LOG is appended, so a later directive for the same target wins.
BASE_LOG_FILTER = 'kmp_mcp=info'


class IsolationError(HarnessError):
    code = 'STORE_ISOLATION_REFUSED'


@dataclass(frozen=True)
class IsolatedStore:
    root: Path
    data_dir: Path
    env: dict = field(repr=False)
    secret_names: tuple = ()
    template_digest: str | None = None  # sha256 of the template tree the store was seeded from

    def allowlisted_env(self):
        """The non-secret configuration recorded in the manifest (paths made relative)."""
        return {key: (value.replace(str(self.root), '<store-root>') if isinstance(value, str) else value)
                for key, value in sorted(self.env.items())
                if key != 'PATH' and key not in self.secret_names}

    def redactions(self):
        """(secret, public) pairs for anything written from this store's processes."""
        secrets = [(self.env[name], f'<{name}>') for name in self.secret_names if self.env.get(name)]
        return secrets + [(str(self.root), '<store-root>')]


def protected_locations(parent_env):
    """Places a real user store or its configuration can live on this machine."""
    home = Path(parent_env.get('HOME') or Path.home())
    data = Path(parent_env.get('XDG_DATA_HOME') or home / '.local/share')
    config = Path(parent_env.get('XDG_CONFIG_HOME') or home / '.config')
    places = [data / 'kmp', config / 'kmp', home / '.kernel', home / '.kmp']
    for key in ('KMP_MCP_DATA_DIR', 'CODEX_HOME'):
        if parent_env.get(key):
            places.append(Path(parent_env[key]))
    return [place.resolve() for place in places]


def _inside(path, place):
    return path == place or place in path.parents


def _refuse_overlap(path, parent_env, what):
    path = Path(path).resolve()
    for place in protected_locations(parent_env):
        if _inside(path, place) or _inside(place, path):
            raise IsolationError(f'{what} {path} overlaps the real KMP location {place}')
    return path


def variant_env(requested, parent_env):
    """Validate a variant's explicit environment; the result is added to the child's."""
    env = {}
    for key, value in sorted((requested or {}).items()):
        if key not in VARIANT_ENV_ALLOWLIST:
            raise IsolationError(f'{key} is not allowlisted ({", ".join(VARIANT_ENV_ALLOWLIST)})')
        if not isinstance(value, str) or not value:
            raise IsolationError(f'{key} must be a non-empty string')
        if key in PATH_VALUED_ENV:
            value = str(_refuse_overlap(value, parent_env, key))
        env[key] = f'{BASE_LOG_FILTER},{value}' if key == 'RUST_LOG' else value
    return env


def secret_env(requested):
    for key, value in (requested or {}).items():
        if key not in SECRET_ENV_ALLOWLIST:
            raise IsolationError(f'{key} is not an allowlisted secret')
        if not isinstance(value, str) or not value:
            raise IsolationError(f'secret {key} must be a non-empty string')
    return dict(requested or {})


def tree_digest(folder):
    """sha256 over (relative path, file sha256) of every regular file, sorted."""
    folder, digest = Path(folder), hashlib.sha256()
    for path in sorted(p for p in folder.rglob('*') if p.is_file() and not p.is_symlink()):
        digest.update(path.relative_to(folder).as_posix().encode() + b'\0')
        digest.update(hashlib.sha256(path.read_bytes()).hexdigest().encode() + b'\n')
    return digest.hexdigest()


def _seed(data_dir, template, parent_env):
    template = _refuse_overlap(template, parent_env, 'template')
    if not template.is_dir():
        raise IsolationError(f'template {template} is not a directory')
    if any(p.is_symlink() for p in template.rglob('*')):
        raise IsolationError(f'template {template} contains a symlink')
    shutil.copytree(template, data_dir, dirs_exist_ok=True)
    digest = tree_digest(template)
    if tree_digest(data_dir) != digest:
        raise IsolationError('template copy differs from its source')
    return digest


def create_store(work_dir, label, parent_env=None, *, env=None, secrets=None, template=None):
    """A disposable root; `env` is a variant's allowlisted configuration, `template` a store to copy."""
    parent_env = dict(os.environ if parent_env is None else parent_env)
    work_dir = Path(work_dir).resolve()
    work_dir.mkdir(parents=True, exist_ok=True)
    extra, hidden = variant_env(env, parent_env), secret_env(secrets)
    root = Path(tempfile.mkdtemp(prefix=label + '-', dir=work_dir)).resolve()
    try:
        _refuse_overlap(root, parent_env, 'store root')
        layout = {'HOME': 'home', 'XDG_DATA_HOME': 'xdg-data', 'XDG_CONFIG_HOME': 'xdg-config',
                  'XDG_CACHE_HOME': 'xdg-cache', 'CODEX_HOME': 'codex-home', 'KMP_MCP_DATA_DIR': 'store'}
        child = {key: parent_env[key] for key in PASSTHROUGH if key in parent_env}
        for key, name in layout.items():
            (root / name).mkdir()
            child[key] = str(root / name)
        child.update(KMP_MCP_BACKEND='embedded', KMP_VIEWER_ADDR='off')
        child.update(extra)
        child.update(hidden)
        digest = _seed(root / 'store', template, parent_env) if template is not None else None
        store = IsolatedStore(root, root / 'store', child, tuple(sorted(hidden)), digest)
        validate_env(store, parent_env)
    except BaseException:
        shutil.rmtree(root, ignore_errors=True)
        raise
    return store


def validate_env(store, parent_env):
    """Refuse when any isolated variable resolves to the operator's real location."""
    for key in ('HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME', 'XDG_CACHE_HOME', 'KMP_MCP_DATA_DIR'):
        value = Path(store.env[key]).resolve()
        if not _inside(value, store.root):
            raise IsolationError(f'{key} escapes the disposable root')
        real = parent_env.get(key)
        if real and Path(real).resolve() == value:
            raise IsolationError(f'{key} equals the parent environment value')
    stray = [key for key in store.env if key.startswith(('KMP_', 'KERNEL_'))
             and key not in HARNESS_ENV + VARIANT_ENV_ALLOWLIST]
    if stray:
        raise IsolationError('unexpected store configuration: ' + ', '.join(stray))
    leaked = [key for key in store.allowlisted_env() if key in SECRET_ENV_ALLOWLIST]
    if leaked:
        raise IsolationError('secret in the recorded environment: ' + ', '.join(leaked))


def confirm_effective_store(store, stderr_text):
    """The binary must log that it resolved the disposable directory via the env rule."""
    for line in stderr_text.splitlines():
        if DATA_DIR_EVENT not in line:
            continue
        try:
            fields = json.loads(line).get('fields', {})
        except ValueError:
            continue
        resolved = Path(fields.get('data_dir', '')).resolve()
        if resolved == store.data_dir.resolve() and fields.get('rule') == 'env':
            return {'data_dir': '<store-root>/store', 'rule': 'env',
                    'engine': fields.get('requested_engine')}
        raise IsolationError(f'binary resolved {resolved} by rule {fields.get("rule")!r}')
    raise IsolationError('binary did not log its resolved data directory')
