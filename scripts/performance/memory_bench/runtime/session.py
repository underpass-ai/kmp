"""One kmp-mcp process of a run: disposable store, pinned cores, per-journey traces.

A `BenchProcess` is token_harness' native session plus what the bench adds:

- the store is a disposable token_harness store, seeded from the run's store
  template and given the variant's store files before the binary starts;
- the child inherits the pinned cores (probes.Pinning) and the bench's log
  filter, with the variant's RUST_LOG after it;
- its handshake frames go to `process-<n>.jsonl`, and each journey's frames to
  that journey's own trace (`record_into`), so one process can serve many
  questions and still keep warm latency;
- every tool call is tapped (`CallTap`), so a record can name the exact
  arguments sent and the observation token_harness took around them;
- at close, the server journal is read and parsed (server_log) and the store
  is deleted.
"""
from contextlib import contextmanager
from dataclasses import dataclass, field
from pathlib import Path
import shutil

from ...token_harness.domain.errors import HarnessError
from ...token_harness.native.isolation import create_store
from ...token_harness.native.recorder import TraceRecorder
from ...token_harness.native.transport import TIMEOUT_SECONDS, StdioSession, TransportError
from . import probes, server_log

CLIENT_NAME = 'memory-bench'


def child_env(variant_env):
    """The variant's allowlisted environment with the bench's log filter in front of its RUST_LOG."""
    env = dict(variant_env or {})
    extra = env.get('RUST_LOG')
    env['RUST_LOG'] = server_log.BENCH_LOG_FILTER + (',' + extra if extra else '')
    return env


@dataclass(frozen=True)
class ProcessConfig:
    binary: Path
    work_dir: Path
    label: str
    env: dict = field(default_factory=dict)  # the variant's allowlisted environment
    store_files: tuple = ()  # domain.variant.StoreFile, written beside the store
    template: Path | None = None  # a built store (data directory) copied into every process
    secrets: dict = field(default_factory=dict, repr=False)  # never recorded
    timeout_seconds: float = TIMEOUT_SECONDS
    probe_resources: bool = True
    pinning: probes.Pinning = probes.Pinning(None, frozenset(), 'pinning not requested')
    # objects with prepare_store(data_dir) and harvest_store(data_dir, journal_lines), such as
    # jev_fixture.JevSample: state that must not live in the store template (a Jev book)
    store_hooks: tuple = ()

    def store_file_digests(self):
        return tuple((item.name, item.sha256) for item in self.store_files)


@dataclass(frozen=True)
class CallAttempt:
    """One tools/call a journey made, with what token_harness observed around it."""
    call_index: int
    process_call_index: int  # position among the process's tools/call, observed or not
    tool: str
    arguments: dict
    observation: object | None  # token_harness CallObservation; None if it never got one


class CallTap:
    """Stands in for the session inside token_harness' driver and keeps every attempt."""

    def __init__(self, session):
        self.session, self.attempts = session, []

    def call(self, tool, arguments):
        before, position = len(self.session.observations), self.session.tool_calls
        try:
            return self.session.call(tool, arguments)
        except OSError as error:  # the child died under us: a transport failure, not a crash
            self.session.broken = True
            raise TransportError(f'{self.session.name}: {type(error).__name__}: {error}') from error
        finally:
            observed = self.session.observations[before:]
            self.attempts.append(CallAttempt(len(self.attempts), position, tool, arguments,
                                              observed[-1] if observed else None))


@dataclass(frozen=True)
class ProcessReport:
    index: int
    exit_code: int | None
    startup_ns: int
    cpus_allowed: str | None
    telemetry: server_log.ProcessTelemetry
    store_files: tuple  # server_log.StoreFileAck
    tool_calls: int  # tools/call the harness sent (answered or timed out)
    effective_store: dict

    def as_dict(self):
        return {'process': self.index, 'exit_code': self.exit_code, 'startup_ns': self.startup_ns,
                'cpus_allowed': self.cpus_allowed, 'effective_store': self.effective_store,
                'tool_lines': len(self.telemetry.calls),
                'unattributed_judgements': self.telemetry.unattributed_judgements,
                'malformed_log_lines': self.telemetry.malformed,
                'store_files': [ack.as_dict() for ack in self.store_files]}


class BenchProcess:
    def __init__(self, config, out_dir, index, ids):
        self.config, self.out_dir, self.index = config, Path(out_dir), index
        self.name = f'process-{index}'
        self.store = create_store(config.work_dir, f'{config.label}-{index}', env=child_env(config.env),
                                  secrets=config.secrets, template=config.template)
        self.session = None
        try:
            for item in config.store_files:
                (self.store.data_dir / item.name).write_bytes(item.content())
            for hook in config.store_hooks:
                hook.prepare_store(self.store.data_dir)
            self.cursor = server_log.LogCursor(self.store.data_dir)
            self.recorder = TraceRecorder(self.out_dir / f'{self.name}.jsonl', self.store.redact)
            self.recorder.marker({'session_start': self.name})
            with config.pinning.for_children():
                self.session = StdioSession(Path(config.binary).resolve(), self.store, self.recorder,
                                            self.name, ids, timeout_seconds=config.timeout_seconds,
                                            probe_resources=config.probe_resources)
            self.cpus_allowed = probes.allowed_cpus(self.session.process.pid)
            if config.pinning.applied and self.cpus_allowed != probes.format_cpu_list(config.pinning.cpus):
                raise probes.PinningError(f'child runs on cpus {self.cpus_allowed}, '
                                          f'pinned {probes.format_cpu_list(config.pinning.cpus)}')
            self.protocol = self.session.handshake(CLIENT_NAME)
        except BaseException:
            self._abandon()
            raise
        self.startup_ns = self.protocol['startup_ns']
        self.journeys_served, self.attempts = 0, []

    @property
    def usable(self):
        return not self.session.broken and self.session.process.poll() is None

    @contextmanager
    def journey(self, name=None):
        """Frames of the block go to `<name>.jsonl`; name None keeps them in the process trace."""
        tap = CallTap(self.session)
        if name is None:
            try:
                yield tap
            finally:
                self.attempts.extend(tap.attempts)
            return
        recorder = TraceRecorder(self.out_dir / f'{name}.jsonl', self.store.redact)
        recorder.marker({'session_start': name, 'process': self.name})
        previous = self.session.record_into(recorder)
        try:
            yield tap
        finally:
            self.session.record_into(previous)
            recorder.close()
            self.attempts.extend(tap.attempts)
            self.journeys_served += 1

    def close(self):
        """Stop the child, keep its stderr (redacted), read its journal, delete the store."""
        try:
            code = self.session.close()
            self.recorder.marker({'session_end': self.name, 'exit_code': code})
            self.recorder.close()
            text = self.session.stderr_path.read_text(errors='replace')
            text = self.store.redact(text)
            (self.out_dir / f'{self.name}.stderr').write_text(text)
            lines = self.cursor.read_new()
            telemetry = server_log.parse(lines)
            for hook in self.config.store_hooks:
                hook.harvest_store(self.store.data_dir, lines)
            acks = server_log.acknowledge(self.config.store_file_digests(), telemetry)
            return ProcessReport(self.index, code, self.startup_ns, self.cpus_allowed, telemetry, acks,
                                 self.session.tool_calls, self.protocol['effective_store'])
        finally:
            shutil.rmtree(self.store.root, ignore_errors=True)

    def _abandon(self):
        if self.session is not None:
            try:
                self.session.broken = True
                self.session.close()
                text = self.session.stderr_path.read_text(errors='replace')
                text = self.store.redact(text)
                (self.out_dir / f'{self.name}.stderr').write_text(text)
            except (OSError, HarnessError, ValueError):
                pass
        recorder = getattr(self, 'recorder', None)
        if recorder is not None and not recorder.handle.closed:
            recorder.close()
        shutil.rmtree(self.store.root, ignore_errors=True)
