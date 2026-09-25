"""Machine probes: core pinning of the MCP child and the state a latency run records.

Latency is measured only on the GX10's Cortex-X925 cores (BENCH_SPEC section 10,
`taskset -c 5-9`). token_harness spawns the child itself, so the pin is the
affinity the child inherits: the harness sets its own affinity to the chosen
cores around the spawn and restores it right after (the same
`sched_setaffinity` taskset performs, with no window where the child runs
elsewhere). The child's `Cpus_allowed_list` is then read back from procfs and
recorded; a run never claims a pin it did not observe. When the cores are not
available, nothing is pinned and the reason is recorded.
"""
from contextlib import contextmanager
from dataclasses import dataclass
import os
from pathlib import Path
import platform

from ..domain.errors import BenchError

LATENCY_CPUS = '5-9'  # Cortex-X925 cores of the GX10 (cpuinfo_max_freq 3.9 GHz)


class PinningError(BenchError):
    code = 'CORE_PINNING_REFUSED'


def parse_cpu_list(text):
    """'5-9' or '5,7-8' -> frozenset of cpu numbers (the taskset -c syntax)."""
    cpus = set()
    for part in str(text).split(','):
        low, dash, high = part.strip().partition('-')
        if not low.isdigit() or (dash and not high.isdigit()):
            raise PinningError(f'{text!r} is not a cpu list')
        first, last = int(low), int(high) if dash else int(low)
        if last < first:
            raise PinningError(f'{text!r}: range {part} runs backwards')
        cpus.update(range(first, last + 1))
    return frozenset(cpus)


def format_cpu_list(cpus):
    ordered, ranges = sorted(cpus), []
    for cpu in ordered:
        if ranges and ranges[-1][1] == cpu - 1:
            ranges[-1][1] = cpu
        else:
            ranges.append([cpu, cpu])
    return ','.join(f'{a}-{b}' if a != b else str(a) for a, b in ranges)


@dataclass(frozen=True)
class Pinning:
    requested: str | None
    cpus: frozenset  # empty: not pinned
    reason: str | None  # why not pinned

    @property
    def applied(self):
        return bool(self.cpus)

    @contextmanager
    def for_children(self, getaffinity=os.sched_getaffinity, setaffinity=os.sched_setaffinity):
        """Processes spawned inside the block inherit the pinned affinity."""
        if not self.applied:
            yield
            return
        previous = getaffinity(0)
        setaffinity(0, self.cpus)
        try:
            yield
        finally:
            setaffinity(0, previous)

    def as_dict(self):
        return {'requested': self.requested, 'cpus': format_cpu_list(self.cpus) if self.cpus else None,
                'method': 'sched_setaffinity inherited at spawn (taskset -c equivalent)'
                if self.applied else None, 'reason': self.reason}


def resolve_pinning(requested=LATENCY_CPUS, available=None):
    """Pin to `requested` only when every one of those cpus is available to this process."""
    if requested is None:
        return Pinning(None, frozenset(), 'pinning not requested')
    wanted = parse_cpu_list(requested)
    if available is None:
        try:
            available = os.sched_getaffinity(0)
        except (AttributeError, OSError) as error:
            return Pinning(requested, frozenset(), f'affinity unavailable: {error}')
    missing = sorted(wanted - set(available))
    if missing:
        return Pinning(requested, frozenset(), f'cpus {format_cpu_list(missing)} not available here')
    return Pinning(requested, wanted, None)


def allowed_cpus(pid, proc_root=Path('/proc')):
    """The child's Cpus_allowed_list as the kernel reports it, or None."""
    try:
        for line in (Path(proc_root) / str(pid) / 'status').read_text().splitlines():
            if line.startswith('Cpus_allowed_list:'):
                return line.split(':', 1)[1].strip()
    except OSError:
        return None
    return None


def _read(path):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return None


def governors(cpus, sys_root=Path('/sys/devices/system/cpu')):
    """The scaling governor of the given cpus: one name when they agree."""
    names = {_read(Path(sys_root) / f'cpu{cpu}/cpufreq/scaling_governor') for cpu in sorted(cpus)}
    names.discard(None)
    if not names:
        return None
    return names.pop() if len(names) == 1 else sorted(names)


def loadavg(proc_root=Path('/proc')):
    text = _read(Path(proc_root) / 'loadavg')
    try:
        return [float(value) for value in text.split()[:3]] if text else None
    except ValueError:
        return None


def machine(pinning, proc_root=Path('/proc'), sys_root=Path('/sys/devices/system/cpu')):
    """run.json `machine`: where the latencies were taken (SCHEMAS.md 3.2)."""
    cpus = pinning.cpus or frozenset(os.sched_getaffinity(0))
    return {'arch': platform.machine(), 'cpus': format_cpu_list(pinning.cpus) if pinning.applied else None,
            'pinning': pinning.as_dict(), 'governor': governors(cpus, sys_root),
            'loadavg': loadavg(proc_root), 'kernel': platform.release(),
            'python': platform.python_version()}
