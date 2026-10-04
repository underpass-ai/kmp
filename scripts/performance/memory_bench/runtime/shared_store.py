"""One disposable KMP data directory that several kmp-mcp processes open at once (BT18).

`session.BenchProcess` gives every process its own store; mixed-version and
multi-process checks need the opposite: one data directory, opened by several
binaries, one after another or at the same time. A `SharedStore` is one
token_harness isolated store (same env rule, same confirmation that the binary
resolved exactly that directory) that any number of `StdioServer`s open.

Each server keeps its own trace (`<name>.jsonl`) and its own stderr, whose JSON
log lines are returned at close: the file journal under `logs/` is shared by
every process of the store, so it cannot say which process logged a line.

This module is the adapter behind `application.mixed_versions`' Store and Server
ports; the application never touches processes or files itself.
"""
from dataclasses import dataclass
import hashlib
import itertools
import json
import mmap
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile

from ...token_harness.domain.errors import HarnessError
from ...token_harness.native.isolation import create_store
from ...token_harness.native.recorder import TraceRecorder
from ...token_harness.native.transport import TIMEOUT_SECONDS, StdioSession
from ..domain.errors import BenchError
from . import binaries, jev_fixture

CLIENT_NAME = 'memory-bench-bt18'
KERNEL_DIR = 'store'  # the engine's files inside the data directory
FORMAT_FILE = 'FORMAT_VERSION'
VERSION_TIMEOUT_SECONDS = 30


class SharedStoreError(BenchError):
    code = 'SHARED_STORE_FAILED'


class OpenRefused(SharedStoreError):
    """The binary would not serve this store (handshake failed, or import refused)."""
    code = 'STORE_OPEN_REFUSED'


@dataclass(frozen=True)
class Reply:
    """One tools/call as the application sees it: ok, the structured content or the error."""
    ok: bool
    structured: dict | None
    error: str | None
    wall_ms: float


@dataclass(frozen=True)
class ServerExit:
    exit_code: int | None
    log_events: tuple  # parsed JSON log lines from this process's own stderr


def binary_version(path):
    """`kmp-mcp --version` first line, run with an empty environment (no store is opened)."""
    try:
        done = subprocess.run([str(path), '--version'], capture_output=True, text=True,
                              env={'PATH': '/usr/bin:/bin'}, timeout=VERSION_TIMEOUT_SECONDS,
                              check=False)
    except (OSError, subprocess.TimeoutExpired) as failure:
        raise SharedStoreError(f'{path}: --version failed: {failure}') from failure
    lines = [line for line in done.stdout.splitlines() if line.strip()]
    return lines[0].strip() if lines else None


def declares(path, markers):
    """The subset of `markers` (name -> bytes) whose bytes occur in the binary file.

    A static capability probe: a binary that knows a sidecar file names it. The
    scenario still confirms the capability on disk before it trusts it.
    """
    found = set()
    with Path(path).open('rb') as handle, mmap.mmap(handle.fileno(), 0, access=mmap.ACCESS_READ) as data:
        for name, needle in markers.items():
            if data.find(needle) >= 0:
                found.add(name)
    return frozenset(found)


def log_events(text):
    events = []
    for line in text.splitlines():
        if not line.startswith('{'):
            continue
        try:
            value = json.loads(line)
        except ValueError:
            continue
        if isinstance(value, dict):
            events.append(value)
    return tuple(events)


def _reply(response, wall_ms):
    if 'error' in response:
        return Reply(False, None, f'rpc error: {json.dumps(response["error"])[:500]}', wall_ms)
    result = response.get('result') or {}
    structured = result.get('structuredContent')
    if result.get('isError'):
        detail = (structured or {}).get('error') if isinstance(structured, dict) else None
        text = json.dumps(detail) if detail else json.dumps(result.get('content'))
        return Reply(False, structured, text[:1000], wall_ms)
    return Reply(True, structured if isinstance(structured, dict) else {}, None, wall_ms)


class StdioServer:
    """One kmp-mcp process on a shared store; call() never raises for a tool error."""

    def __init__(self, store, binary, name, timeout_seconds=TIMEOUT_SECONDS):
        self.store, self.binary, self.name = store, binary, name
        self.recorder = TraceRecorder(store.out_dir / f'{name}.jsonl', store.isolated.redact)
        self.session = None
        try:
            self.session = StdioSession(Path(binary.path), store.isolated, self.recorder, name,
                                        itertools.count(1), timeout_seconds=timeout_seconds,
                                        probe_resources=False)
            self.protocol = self.session.handshake(CLIENT_NAME)
        except (HarnessError, OSError, ValueError) as failure:
            detail = self._stderr_tail()
            self._shutdown()
            raise OpenRefused(f'{binary.label} did not start on the store: {failure}; {detail}') \
                from failure

    def _stderr_tail(self):
        if self.session is None:
            return ''
        try:
            return self.session.stderr_path.read_text(errors='replace')[-600:]
        except OSError:
            return ''

    def call(self, tool, arguments):
        try:
            response = self.session.call(tool, arguments)
        except HarnessError as failure:  # timeout or closed stdout: the process is gone
            observed = self.session.last_observation
            wall = observed.wall_ns / 1e6 if observed is not None else 0.0
            return Reply(False, None, f'transport: {failure}', wall)
        return _reply(response, self.session.last_observation.wall_ns / 1e6)

    def _shutdown(self):
        code = None
        if self.session is not None:
            try:
                code = self.session.close()
            except (OSError, subprocess.SubprocessError):
                code = None
        if not self.recorder.handle.closed:
            self.recorder.close()
        return code

    def close(self):
        code = self._shutdown()
        text = self.session.stderr_path.read_text(errors='replace')
        text = self.store.isolated.redact(text)
        (self.store.out_dir / f'{self.name}.stderr').write_text(text)
        return ServerExit(code, log_events(text))


class SharedStore:
    """A disposable data directory (optionally seeded from a template) for many processes."""

    def __init__(self, work_dir, out_dir, label, template=None, env=None, store_files=None):
        self.work_dir, self.out_dir, self.label = Path(work_dir), Path(out_dir), label
        self.out_dir.mkdir(parents=True, exist_ok=True)
        self.env, self.store_files = dict(env or {}), dict(store_files or {})
        self.owned = ()  # extra scratch folders this store removes at close
        self.isolated = create_store(self.work_dir, label, env=self.env, template=template)
        for name, content in self.store_files.items():
            (self.isolated.data_dir / name).write_bytes(content)

    @property
    def data_dir(self):
        return self.isolated.data_dir

    def open(self, binary, name):
        return StdioServer(self, binary, name)

    def fingerprint(self):
        """sha256 over the engine's files and the format stamp (journals and telemetry excluded)."""
        digest = hashlib.sha256()
        stamp = self.data_dir / FORMAT_FILE
        digest.update(stamp.read_bytes() if stamp.is_file() else b'<no stamp>')
        kernel = self.data_dir / KERNEL_DIR
        for path in sorted(p for p in kernel.rglob('*') if p.is_file()) if kernel.is_dir() else ():
            if path.name.endswith('-shm'):
                continue  # rebuilt by any reader; holds no data
            digest.update(path.relative_to(kernel).as_posix().encode() + b'\0')
            digest.update(hashlib.sha256(path.read_bytes()).hexdigest().encode() + b'\n')
        return digest.hexdigest()

    def format_stamp(self):
        stamp = self.data_dir / FORMAT_FILE
        return stamp.read_text().strip() if stamp.is_file() else None

    def has_file(self, relative):
        return (self.data_dir / relative).exists()

    def remove_file(self, relative):
        for suffix in ('', '-wal', '-shm'):
            (self.data_dir / (relative + suffix)).unlink(missing_ok=True)

    def _snapshot(self, relative):
        """A private copy of a SQLite file and its WAL, so reading it never touches the store."""
        scratch = Path(tempfile.mkdtemp(prefix='bt18-sqlite-', dir=self.work_dir))
        source = self.data_dir / relative
        if not source.is_file():
            raise SharedStoreError(f'{relative} is not in the store')
        for suffix in ('', '-wal'):
            if Path(str(source) + suffix).is_file():
                shutil.copyfile(str(source) + suffix, scratch / (source.name + suffix))
        return scratch, scratch / source.name

    def query(self, relative, sql):
        scratch, copy = self._snapshot(relative)
        try:
            connection = sqlite3.connect(copy)
            try:
                return [tuple(row) for row in connection.execute(sql)]
            finally:
                connection.close()
        finally:
            shutil.rmtree(scratch, ignore_errors=True)

    def integrity(self):
        """PRAGMA integrity_check of the kernel database, on a copy."""
        kernels = sorted((self.data_dir / KERNEL_DIR).glob('*.sqlite3'))
        if not kernels:
            return 'no kernel database'
        results = [row[0] for kernel in kernels
                   for row in self.query(kernel.relative_to(self.data_dir).as_posix(),
                                         'PRAGMA integrity_check')]
        return 'ok' if results == ['ok'] * len(kernels) else '; '.join(map(str, results))[:300]

    def events(self, binary):
        """Event count and content digest as `binary` exports them (no process may be running)."""
        destination = Path(tempfile.mkdtemp(prefix='bt18-export-', dir=self.work_dir)) / 'bundle.jsonl'
        try:
            summary, _ = binaries.export_bundle(binaries.KmpBinary(binary.path, binary.sha256),
                                                self.isolated, destination)
        except binaries.BinaryFailed as failure:
            raise OpenRefused(f'{binary.label} export refused: {failure}') from failure
        finally:
            shutil.rmtree(destination.parent, ignore_errors=True)
        return summary['event_count'], summary['content_digest']

    def transfer(self, writer, reader, label):
        """A new store holding this one's log: `writer` exports, `reader` imports."""
        folder = Path(tempfile.mkdtemp(prefix='bt18-bundle-', dir=self.work_dir))
        bundle = folder / 'bundle.jsonl'
        target = SharedStore(self.work_dir, self.out_dir, label, env=self.env,
                             store_files=self.store_files)
        try:
            binaries.export_bundle(binaries.KmpBinary(writer.path, writer.sha256), self.isolated, bundle)
            binaries.import_bundle(binaries.KmpBinary(reader.path, reader.sha256), target.isolated,
                                   bundle)
        except binaries.BinaryFailed as failure:
            target.close()
            raise OpenRefused(str(failure)) from failure
        finally:
            shutil.rmtree(folder, ignore_errors=True)
        return target

    def fork(self, label, offline=False):
        """A new store seeded with a copy of this one's data directory (journals left out).

        `offline` puts an empty cassette in replay mode behind Jev, as jev_fixture's
        network block does: a verdict the store cannot answer fails instead of reaching
        the provider.
        """
        folder = Path(tempfile.mkdtemp(prefix='bt18-fork-', dir=self.work_dir))
        env, owned = dict(self.env), ()
        if offline:
            block = Path(tempfile.mkdtemp(prefix='bt18-block-', dir=self.work_dir))
            jev_fixture.empty_cassette(block / jev_fixture.BLOCK_FILE, jev_fixture.DEFAULT_MODEL)
            env.update({jev_fixture.CASSETTE_ENV: str(block / jev_fixture.BLOCK_FILE),
                        jev_fixture.CASSETTE_MODE_ENV: 'replay'})
            owned = (block,)
        try:
            shutil.copytree(self.data_dir, folder / 'store',
                            ignore=lambda where, names: ['logs'] if Path(where) == self.data_dir
                            and 'logs' in names else [])
            forked = SharedStore(self.work_dir, self.out_dir, label, template=folder / 'store',
                                 env=env, store_files={})
        except BaseException:
            for path in owned:
                shutil.rmtree(path, ignore_errors=True)
            raise
        finally:
            shutil.rmtree(folder, ignore_errors=True)
        forked.owned = owned
        return forked

    def close(self):
        shutil.rmtree(self.isolated.root, ignore_errors=True)
        for path in self.owned:
            shutil.rmtree(path, ignore_errors=True)
