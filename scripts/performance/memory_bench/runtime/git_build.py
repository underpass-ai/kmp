"""The binary a variant names: a path as it is, or a `git_ref` built with provenance.

A `git_ref` (SCHEMAS.md section 4) is resolved to a commit and built in a
throwaway git worktree under the scratch area with
`cargo build --release --locked -p kmp-mcp`; the executable and its
`provenance.json` land in `bin/<binary_sha256>/` (section 7) and the worktree is
removed. A second request for the same commit is a cache hit. Every build shares
one cargo target directory in the scratch area, so a later ref recompiles only
what changed; deleting `work/` costs a full build, nothing else.

A `path` binary gets the provenance that can be read without building: its
SHA-256, its `--version` and, when it lives inside this checkout, the checkout's
commit and whether the tree had uncommitted changes (a dirty build is not the
commit it names).
"""
from dataclasses import dataclass
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from ...token_harness.native.capture import sha256_file
from ..domain import cachekey
from ..domain.errors import BenchError
from ..domain.jsonl import REPO_ROOT

SCHEMA = 'kmp.bench.binary.v1'
PROVENANCE = 'provenance.json'
EXECUTABLE = 'kmp-mcp'
BUILD_COMMAND = ('cargo', 'build', '--release', '--locked', '-p', 'kmp-mcp')
TARGET_DIR = 'cargo-target'
TOOL_TIMEOUT_S = 60.0
BUILD_TIMEOUT_S = 3600.0


class GitBuildFailed(BenchError):
    code = 'GIT_BUILD_FAILED'


@dataclass(frozen=True)
class ResolvedBinary:
    path: Path
    sha256: str
    version: str | None
    provenance: dict


def _run(argv, cwd, runner, timeout=TOOL_TIMEOUT_S, env=None):
    try:
        done = runner(list(argv), cwd=str(cwd), capture_output=True, text=True, timeout=timeout,
                      check=False, env=env)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise GitBuildFailed(f'{" ".join(argv)}: {type(error).__name__}: {error}') from error
    if done.returncode != 0:
        raise GitBuildFailed(f'{" ".join(argv)} exited {done.returncode}: {(done.stderr or "")[-800:]}')
    return done.stdout.strip()


def binary_version(path, runner=subprocess.run):
    """`kmp-mcp --version`, or None when the binary does not answer it."""
    try:
        return _run([str(path), '--version'], Path(path).parent, runner) or None
    except GitBuildFailed:
        return None


def resolve_commit(ref, repo_root=REPO_ROOT, runner=subprocess.run):
    return _run(['git', 'rev-parse', '--verify', '--end-of-options', f'{ref}^{{commit}}'], repo_root, runner)


def path_provenance(path, repo_root=REPO_ROOT, runner=subprocess.run):
    """What a path binary can say about itself; the checkout's commit when it was built here."""
    path = Path(path).resolve()
    provenance = {'source': 'path'}
    if Path(repo_root).resolve() in path.parents:
        provenance['path'] = path.relative_to(Path(repo_root).resolve()).as_posix()
        try:
            provenance['git_commit'] = _run(['git', 'rev-parse', 'HEAD'], repo_root, runner)
            status = _run(['git', 'status', '--porcelain', '--untracked-files=no'], repo_root, runner)
            provenance['dirty'] = bool(status)
        except GitBuildFailed as error:
            provenance['git_error'] = error.detail
    return provenance


def resolve_path(value, repo_root=REPO_ROOT, runner=subprocess.run):
    path = Path(value)
    path = (path if path.is_absolute() else Path(repo_root) / path).resolve()
    if not path.is_file():
        raise GitBuildFailed(f'{path} not found: cargo build --release -p kmp-mcp')
    return ResolvedBinary(path, sha256_file(path), binary_version(path, runner),
                          path_provenance(path, repo_root, runner))


def cached_build(layout, commit):
    """The binary built earlier from `commit`, or None."""
    folder = layout.root / 'bin'
    for record in sorted(folder.glob(f'*/{PROVENANCE}')) if folder.is_dir() else ():
        try:
            provenance = json.loads(record.read_text(encoding='utf-8'))
        except ValueError:
            continue
        executable = record.parent / EXECUTABLE
        if provenance.get('git_commit') == commit and executable.is_file() \
                and sha256_file(executable) == record.parent.name:
            return ResolvedBinary(executable, record.parent.name, provenance.get('version'), provenance)
    return None


def _remove_worktree(worktree, repo_root, runner):
    if worktree.exists():
        try:
            _run(['git', 'worktree', 'remove', '--force', str(worktree)], repo_root, runner)
        except GitBuildFailed:
            shutil.rmtree(worktree, ignore_errors=True)
    try:
        _run(['git', 'worktree', 'prune'], repo_root, runner)
    except GitBuildFailed:
        pass


def _now():
    return datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')


def build_ref(ref, layout, repo_root=REPO_ROOT, runner=subprocess.run, log=sys.stderr):
    """`bin/<sha>/kmp-mcp` built from `ref` in a disposable worktree (a cache hit if built before)."""
    commit = resolve_commit(ref, repo_root, runner)
    hit = cached_build(layout, commit)
    if hit is not None:
        return hit
    scratch = layout.require_inside(layout.scratch())
    scratch.mkdir(parents=True, exist_ok=True)
    worktree = layout.require_inside(scratch / f'build-{commit[:16]}')
    target = layout.require_inside(scratch / TARGET_DIR)
    _remove_worktree(worktree, repo_root, runner)
    _run(['git', 'worktree', 'add', '--detach', str(worktree), commit], repo_root, runner)
    try:
        print(f'memory_bench: building {ref} ({commit[:12]}) in {worktree}', file=log, flush=True)
        env = {**os.environ, 'CARGO_TARGET_DIR': str(target)}
        started = time.perf_counter()
        _run(BUILD_COMMAND, worktree, runner, BUILD_TIMEOUT_S, env)
        seconds = round(time.perf_counter() - started, 3)
        built = target / 'release' / EXECUTABLE
        sha = sha256_file(built)
        provenance = {'schema': SCHEMA, 'source': 'git_ref', 'git_ref': ref, 'git_commit': commit,
                      'build': ' '.join(BUILD_COMMAND), 'cargo': _run(['cargo', '--version'], worktree, runner),
                      'rustc': _run(['rustc', '--version'], worktree, runner), 'built_at': _now(),
                      'build_s': seconds, 'binary_sha256': sha, 'version': binary_version(built, runner)}
    finally:
        _remove_worktree(worktree, repo_root, runner)
    entry = layout.entry('bin', sha)
    if not entry.exists():
        staging = layout.require_inside(scratch / f'bin-{sha}.partial')
        shutil.rmtree(staging, ignore_errors=True)
        staging.mkdir(parents=True)
        shutil.copy2(built, staging / EXECUTABLE)
        (staging / PROVENANCE).write_text(cachekey.canonical_json(provenance) + '\n', encoding='utf-8')
        entry.parent.mkdir(parents=True, exist_ok=True)
        staging.rename(entry)
    return ResolvedBinary(entry / EXECUTABLE, sha, provenance['version'], provenance)


def resolve_variant_binary(variant, layout, repo_root=REPO_ROOT, runner=subprocess.run, log=sys.stderr):
    """The executable, SHA-256, version and provenance of a variant's `[binary]`."""
    if variant.binary.path is not None:
        return resolve_path(variant.binary.path, repo_root, runner)
    return build_ref(variant.binary.git_ref, layout, repo_root, runner, log)
