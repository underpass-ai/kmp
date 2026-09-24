"""Disposable stores whose location is validated before the first write (annex A.9).

The child environment is built from an allowlist, never inherited wholesale:
HOME, XDG_* and the KMP data directory all point inside a fresh temporary
directory. The binary's own startup log must then confirm that it resolved
exactly that directory by the explicit `env` rule before any tool is called.
"""
from dataclasses import dataclass
import json
import os
from pathlib import Path
import tempfile

from ..domain.errors import HarnessError

PASSTHROUGH = ('PATH', 'LANG', 'LC_ALL')
DATA_DIR_EVENT = 'embedded backend data dir resolved'


class IsolationError(HarnessError):
    code = 'STORE_ISOLATION_REFUSED'


@dataclass(frozen=True)
class IsolatedStore:
    root: Path
    data_dir: Path
    env: dict

    def allowlisted_env(self):
        """The non-secret configuration recorded in the manifest (paths made relative)."""
        return {key: (value.replace(str(self.root), '<store-root>') if isinstance(value, str) else value)
                for key, value in sorted(self.env.items()) if key != 'PATH'}


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


def create_store(work_dir, label, parent_env=None):
    parent_env = dict(os.environ if parent_env is None else parent_env)
    work_dir = Path(work_dir).resolve()
    work_dir.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix=label + '-', dir=work_dir)).resolve()
    for place in protected_locations(parent_env):
        if _inside(root, place) or _inside(place, root):
            raise IsolationError(f'{root} overlaps the real KMP location {place}')
    layout = {'HOME': 'home', 'XDG_DATA_HOME': 'xdg-data', 'XDG_CONFIG_HOME': 'xdg-config',
              'XDG_CACHE_HOME': 'xdg-cache', 'CODEX_HOME': 'codex-home', 'KMP_MCP_DATA_DIR': 'store'}
    env = {key: parent_env[key] for key in PASSTHROUGH if key in parent_env}
    for key, name in layout.items():
        (root / name).mkdir()
        env[key] = str(root / name)
    env.update(KMP_MCP_BACKEND='embedded', KMP_VIEWER_ADDR='off')
    store = IsolatedStore(root, root / 'store', env)
    validate_env(store, parent_env)
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
             and key not in ('KMP_MCP_DATA_DIR', 'KMP_MCP_BACKEND', 'KMP_VIEWER_ADDR')]
    if stray:
        raise IsolationError('unexpected store configuration: ' + ', '.join(stray))


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
