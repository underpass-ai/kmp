"""The kmp-mcp a build runs, and the CLI verbs it runs beside MCP (`export`, `import`).

A binary is identified by the SHA-256 of its file (SCHEMAS.md section 6). The CLI
verbs run in the same disposable environment token_harness builds for a stdio
session (`isolation.create_store`), so `KMP_MCP_DATA_DIR` names the disposable
store by the `env` rule and nothing reaches the operator's store. `export` reports
the data directory it read; `confirm_data_dir` refuses any other, as
`confirm_effective_store` does for a session. `import` reports none, so a build
re-exports the imported store and checks both the directory and the digest.
"""
from dataclasses import dataclass
import json
from pathlib import Path
import subprocess
import time

from ...token_harness.native.capture import sha256_file
from ..domain.errors import BenchError
from ..domain.jsonl import REPO_ROOT

DEFAULT_BINARY = REPO_ROOT / 'target' / 'release' / 'kmp-mcp'
CLI_TIMEOUT_SECONDS = 3600.0
STDERR_TAIL = 800


class BinaryFailed(BenchError):
    code = 'KMP_BINARY_FAILED'


@dataclass(frozen=True)
class KmpBinary:
    path: Path
    sha256: str

    @classmethod
    def locate(cls, path=None):
        path = Path(path) if path else DEFAULT_BINARY
        if not path.is_file():
            raise BinaryFailed(f'{path} not found: cargo build --release -p kmp-mcp')
        path = path.resolve()
        return cls(path, sha256_file(path))


@dataclass(frozen=True)
class CliRun:
    verb: str
    returncode: int
    stdout: str
    stderr: str
    wall_ms: float

    def json_line(self):
        """The last non-blank stdout line, which `export` and `import` print as JSON."""
        lines = [line for line in self.stdout.splitlines() if line.strip()]
        if not lines:
            raise BinaryFailed(f'kmp-mcp {self.verb} printed nothing on stdout')
        try:
            value = json.loads(lines[-1])
        except ValueError as failure:
            raise BinaryFailed(f'kmp-mcp {self.verb} printed a non-JSON result: {failure}') from failure
        if not isinstance(value, dict):
            raise BinaryFailed(f'kmp-mcp {self.verb} printed a non-object result')
        return value


def run_cli(binary, store, verb, arguments=(), timeout_s=CLI_TIMEOUT_SECONDS):
    """`kmp-mcp <verb> <arguments>` in the store's isolated environment; refuses a failure."""
    argv = [str(binary.path), verb, *map(str, arguments)]
    started = time.perf_counter_ns()
    try:
        done = subprocess.run(argv, cwd=store.root, env=store.env, capture_output=True, text=True,
                              encoding='utf-8', timeout=timeout_s, check=False)
    except subprocess.TimeoutExpired as failure:
        raise BinaryFailed(f'kmp-mcp {verb} did not finish within {timeout_s:g}s') from failure
    wall_ms = (time.perf_counter_ns() - started) / 1e6
    stderr = done.stderr
    for secret, public in store.redactions():
        stderr = stderr.replace(secret, public)
    run = CliRun(verb, done.returncode, done.stdout, stderr, wall_ms)
    if done.returncode != 0:
        raise BinaryFailed(f'kmp-mcp {verb} exited {done.returncode}: {stderr[-STDERR_TAIL:]}')
    return run


def confirm_data_dir(store, reported):
    if Path(reported or '').resolve() != store.data_dir.resolve():
        raise BinaryFailed('kmp-mcp read a data directory other than the disposable store')


def export_bundle(binary, store, destination, timeout_s=CLI_TIMEOUT_SECONDS):
    """Whole-store export to `destination`; (export summary, CliRun). Refuses another store."""
    run = run_cli(binary, store, 'export', [Path(destination).resolve()], timeout_s)
    summary = run.json_line()
    confirm_data_dir(store, summary.get('data_dir'))
    for name in ('content_digest', 'event_count'):
        if name not in summary:
            raise BinaryFailed(f'kmp-mcp export result lacks {name}')
    return summary, run


def import_bundle(binary, store, bundle, timeout_s=CLI_TIMEOUT_SECONDS):
    """Replay `bundle` into the (empty) store; (import report, CliRun)."""
    run = run_cli(binary, store, 'import', [Path(bundle).resolve()], timeout_s)
    report = run.json_line()
    if 'events_imported' not in report:
        raise BinaryFailed('kmp-mcp import result lacks events_imported')
    return report, run


def bundle_header(path):
    """The first line of a KMP bundle: format versions, event count, content digest."""
    with Path(path).open('r', encoding='utf-8') as handle:
        first = handle.readline()
    try:
        header = json.loads(first)
    except ValueError as failure:
        raise BinaryFailed(f'{path}: bundle header is not JSON') from failure
    for name in ('bundle_format', 'event_format', 'content_digest', 'event_count'):
        if name not in header:
            raise BinaryFailed(f'{path}: bundle header lacks {name}')
    return header


def bundle_format(header):
    """The format part of a store key: what a reader must understand to import the bundle."""
    return {'bundle_format': header['bundle_format'], 'event_format': header['event_format']}
