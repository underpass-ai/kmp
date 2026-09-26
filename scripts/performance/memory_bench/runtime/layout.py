"""Where the bench keeps what it generates (SCHEMAS.md section 7).

Public material lives under `<repo>/tmp/memory-bench` (git-ignored). Private
material — the real-store copy, B-real questions, their runs — lives under
$MEMORY_BENCH_PRIVATE_ROOT, which must be outside the repository; there is no
default, so private work cannot start by accident in the wrong place. Every
cache entry is a directory named by its full 64-hex key.
"""
from dataclasses import dataclass
import os
from pathlib import Path

from ..domain.cachekey import require_hex64
from ..domain.errors import BenchError
from ..domain.jsonl import REPO_ROOT, guard_private_path

PRIVATE_ROOT_ENV = 'MEMORY_BENCH_PRIVATE_ROOT'
KEYED = ('worlds', 'stores', 'runs', 'reports', 'bin', 'jev')
SCRATCH = 'work'  # disposable stores (token_harness work_dir); emptied at will


class LayoutError(BenchError):
    code = 'CACHE_LAYOUT_REFUSED'


@dataclass(frozen=True)
class CacheLayout:
    root: Path
    private: bool

    def entry(self, kind, key):
        if kind not in KEYED:
            raise LayoutError(f'{kind!r} is not a cache kind ({", ".join(KEYED)})')
        return self.root / kind / require_hex64(key, kind + ' key')

    def world(self, key):
        return self.entry('worlds', key)

    def store(self, key):
        return self.entry('stores', key)

    def run(self, key):
        return self.entry('runs', key)

    def report(self, key):
        return self.entry('reports', key)

    def scratch(self):
        return self.root / SCRATCH

    def contains(self, path):
        path, root = Path(path).resolve(), self.root.resolve()
        return path == root or root in path.parents

    def require_inside(self, path):
        """Writers call this before writing: nothing escapes the cache root."""
        if not self.contains(path):
            raise LayoutError(f'{path} is outside the cache root {self.root}')
        return Path(path)


def public_layout(root=None, repo_root=REPO_ROOT):
    return CacheLayout(Path(root) if root else Path(repo_root) / 'tmp' / 'memory-bench', False)


def private_layout(root=None, env=None, repo_root=REPO_ROOT):
    """`<private root>/cache`; the private root comes from --private-root or the environment."""
    env = os.environ if env is None else env
    chosen = root or env.get(PRIVATE_ROOT_ENV)
    if not chosen:
        raise LayoutError(f'private corpora need {PRIVATE_ROOT_ENV} (outside the repository)')
    base = guard_private_path(Path(chosen).expanduser(), repo_root)
    return CacheLayout(base / 'cache', True)
