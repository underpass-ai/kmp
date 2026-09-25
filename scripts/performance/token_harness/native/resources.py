"""Per-call resources of the MCP child, read from Linux procfs around one RPC.

Before the call the peak RSS is reset (`5` into `/proc/<pid>/clear_refs`), and
`/proc/<pid>/io` plus the `schedstat` of every task are read; after the call
`VmHWM` is the peak RSS of that call and the other two are deltas. Anything the
platform does not offer is null and named in `absent` with its reason, never
guessed. CPU of a thread that exits during the call is lost (a lower bound).
"""
from dataclasses import dataclass
from pathlib import Path

IO_FIELDS = ('rchar', 'wchar', 'syscr', 'syscw', 'read_bytes', 'write_bytes',
             'cancelled_write_bytes')
RESOURCE_FIELDS = ('rss_peak_kb', 'cpu_ns', 'io')


@dataclass(frozen=True)
class Snapshot:
    io: dict | None
    cpu_ns: int | None
    peak_reset: bool
    absent: dict  # field -> reason, for what could not be read before the call


def _reason(error):
    return f'{type(error).__name__}: {error.strerror or error}' if isinstance(error, OSError) else str(error)


class ProcessProbe:
    def __init__(self, pid, proc_root=Path('/proc')):
        self.base = Path(proc_root) / str(pid)

    def _io(self):
        values = {}
        for line in (self.base / 'io').read_text().splitlines():
            key, _, value = line.partition(':')
            if key in IO_FIELDS:
                values[key] = int(value)
        return {key: values[key] for key in IO_FIELDS}

    def _cpu_ns(self):
        total = 0
        for task in (self.base / 'task').iterdir():
            try:
                total += int((task / 'schedstat').read_text().split()[0])
            except FileNotFoundError:
                continue  # the thread exited between listing and reading
        return total

    def _peak_kb(self):
        for line in (self.base / 'status').read_text().splitlines():
            if line.startswith('VmHWM:'):
                return int(line.split()[1])
        raise ValueError('VmHWM missing from status')

    def _read(self, name, reader, absent):
        try:
            return reader()
        except (OSError, ValueError, IndexError, KeyError) as error:
            absent[name] = _reason(error)
            return None

    def before(self):
        absent = {}
        try:
            (self.base / 'clear_refs').write_text('5')
            reset = True
        except OSError as error:
            absent['rss_peak_kb'] = 'clear_refs not writable: ' + _reason(error)
            reset = False
        io = self._read('io', self._io, absent)
        cpu = self._read('cpu_ns', self._cpu_ns, absent)
        return Snapshot(io, cpu, reset, absent)

    def after(self, snapshot):
        absent = dict(snapshot.absent)
        peak = self._read('rss_peak_kb', self._peak_kb, absent) if snapshot.peak_reset else None
        io = self._read('io', self._io, absent) if snapshot.io is not None else None
        cpu = self._read('cpu_ns', self._cpu_ns, absent) if snapshot.cpu_ns is not None else None
        return {'rss_peak_kb': peak,
                'cpu_ns': None if cpu is None else cpu - snapshot.cpu_ns,
                'io': None if io is None else {k: io[k] - snapshot.io[k] for k in IO_FIELDS},
                'absent': {k: absent[k] for k in RESOURCE_FIELDS if k in absent}}


class NoProbe:
    """Stands in when resources are not requested; every field is absent with a reason."""

    reason = 'resource probing disabled'

    def before(self):
        return None

    def after(self, _snapshot):
        return {'rss_peak_kb': None, 'cpu_ns': None, 'io': None,
                'absent': {name: self.reason for name in RESOURCE_FIELDS}}
