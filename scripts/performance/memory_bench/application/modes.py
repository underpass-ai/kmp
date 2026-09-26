"""config/modes.toml (`kmp.bench.modes.v1`): what each mode runs, fixed before it runs.

A mode pins the measurement parameters the result key does not hold (max_calls,
samples, repeats, max_bytes, timeout_s), the sections it runs, the question
strata of the private corpora, the synth ladder and the judged arms. `check_run`
refuses a cached run whose manifest disagrees with its mode, so an edited mode
never silently reuses runs measured under the old parameters.
"""
from dataclasses import dataclass
from pathlib import Path
import tomllib

from ..domain import cachekey
from ..domain.errors import BenchError
from ..domain.run_record import MAX_CALLS_LIMIT
from .verdict import VERDICTS

SCHEMA = 'kmp.bench.modes.v1'
MODES_PATH = Path(__file__).resolve().parents[1] / 'config' / 'modes.toml'
SECTIONS = ('real-store', 'synth', 'retrieval-judged', 'jev-judged', 'public')
RETRIEVAL_ARMS = ('plain', 'narrow', 'wide')
TOPOLOGIES = ('mono', 'multi')


class ModeInvalid(BenchError):
    code = 'MODE_INVALID'


@dataclass(frozen=True)
class RealStrata:
    b_real: bool = True
    hard_negatives_per_stratum: int = 0  # 0 = every question
    guards_per_stratum: int = 0


@dataclass(frozen=True)
class SynthLadder:
    seed: int
    topologies: tuple
    levels: tuple
    batch_size: int
    max_bytes_sweep: tuple = ()
    per_type: int = 0  # the first per_type questions (by id) of each type; 0 = every question


@dataclass(frozen=True)
class JudgedArms:
    retrieval_arms: tuple = ()
    jev: bool = False


@dataclass(frozen=True)
class Mode:
    name: str
    description: str
    budget_s: float  # 0: no budget
    max_calls: int
    samples: int
    repeats: int
    max_bytes: int | None
    timeout_s: float
    warmup: int
    block: int
    cpus: str | None
    bootstrap_b: int
    sections: tuple
    real: RealStrata
    synth: SynthLadder | None
    judged: JudgedArms
    sha256: str  # of the whole modes.toml
    replica_of: str | None = None  # aa: runs as that mode, the candidate is a fresh replica

    @property
    def run_mode(self):
        """The mode name in the result key: a replica shares the runs of the mode it copies."""
        return self.replica_of or self.name

    def run_parameters(self):
        """The manifest fields a cached run must share with its mode."""
        return {'samples': self.samples, 'repeats': self.repeats, 'max_calls': self.max_calls,
                'timeout_s': float(self.timeout_s),
                'max_bytes': None if self.max_bytes is None else [self.max_bytes]}

    def check_run(self, manifest, max_bytes=None):
        """Refuse a run measured under other parameters than this mode's."""
        expected = self.run_parameters()
        if max_bytes is not None:
            expected['max_bytes'] = [max_bytes]
        found = {name: manifest.get(name) for name in expected}
        if found != expected:
            differing = sorted(name for name in expected if found[name] != expected[name])
            raise ModeInvalid(f'run {manifest.get("run_id")} was measured with other {", ".join(differing)} '
                              f'than mode {self.name}; remove it from the cache to measure it again')


def _int(table, name, where, minimum=0, maximum=None, default=None):
    value = table.get(name, default)
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum \
            or (maximum is not None and value > maximum):
        raise ModeInvalid(f'{where}.{name}: an integer >= {minimum}' + (f' and <= {maximum}' if maximum else ''))
    return value


def _real(table, where):
    table = table or {}
    value = RealStrata(bool(table.get('b_real', True)),
                       _int(table, 'hard_negatives_per_stratum', where, default=0),
                       _int(table, 'guards_per_stratum', where, default=0))
    return value


def _synth(table, where):
    if not table:
        return None
    topologies = tuple(table.get('topologies', ()))
    levels = tuple(table.get('levels', ()))
    if not topologies or set(topologies) - set(TOPOLOGIES):
        raise ModeInvalid(f'{where}.topologies: mono and/or multi')
    if not levels or levels != tuple(sorted(set(levels))) or any(
            isinstance(v, bool) or not isinstance(v, int) or v < 1 for v in levels):
        raise ModeInvalid(f'{where}.levels: strictly increasing positive integers')
    sweep = tuple(table.get('max_bytes_sweep', ()))
    if any(isinstance(v, bool) or not isinstance(v, int) or v < 1 for v in sweep):
        raise ModeInvalid(f'{where}.max_bytes_sweep: positive integers')
    return SynthLadder(_int(table, 'seed', where), topologies, levels,
                       _int(table, 'batch_size', where, minimum=1), sweep,
                       _int(table, 'per_type', where, default=0))


def _judged(table, where):
    table = table or {}
    arms = tuple(table.get('retrieval_arms', ()))
    if set(arms) - set(RETRIEVAL_ARMS):
        raise ModeInvalid(f'{where}.retrieval_arms: some of {", ".join(RETRIEVAL_ARMS)}')
    return JudgedArms(arms, bool(table.get('jev', False)))


def _mode(name, table, sha, replica_of=None):
    where = f'modes.{name}'
    sections = tuple(table.get('sections', ()))
    if not sections or set(sections) - set(SECTIONS):
        raise ModeInvalid(f'{where}.sections: some of {", ".join(SECTIONS)}')
    max_bytes = table.get('max_bytes')
    if max_bytes is not None:
        max_bytes = _int(table, 'max_bytes', where, minimum=1)
    timeout = table.get('timeout_s')
    if isinstance(timeout, bool) or not isinstance(timeout, (int, float)) or timeout <= 0:
        raise ModeInvalid(f'{where}.timeout_s: a positive number of seconds')
    budget = table.get('budget_s', 0)
    if isinstance(budget, bool) or not isinstance(budget, (int, float)) or budget < 0:
        raise ModeInvalid(f'{where}.budget_s: seconds, 0 for none')
    return Mode(name=name, description=str(table.get('description', '')), budget_s=float(budget),
                max_calls=_int(table, 'max_calls', where, 1, MAX_CALLS_LIMIT),
                samples=_int(table, 'samples', where, 1), repeats=_int(table, 'repeats', where, 1),
                max_bytes=max_bytes, timeout_s=float(timeout), warmup=_int(table, 'warmup', where),
                block=_int(table, 'block', where, 1), cpus=table.get('cpus'),
                bootstrap_b=_int(table, 'bootstrap_b', where, 1), sections=sections,
                real=_real(table.get('real'), where + '.real'),
                synth=_synth(table.get('synth'), where + '.synth'),
                judged=_judged(table.get('judged'), where + '.judged'), sha256=sha, replica_of=replica_of)


@dataclass(frozen=True)
class Modes:
    sha256: str
    precedence: tuple
    modes: dict

    def get(self, name):
        if name not in self.modes:
            raise ModeInvalid(f'no mode {name!r} in modes.toml ({", ".join(sorted(self.modes))})')
        return self.modes[name]

    def combine(self, verdicts):
        """The mode's verdict: the first value in precedence order that any section reached."""
        for value in self.precedence:
            if value in verdicts:
                return value
        return None


def parse_modes(text, origin='modes.toml'):
    sha = cachekey.sha256_hex(text.encode('utf-8'))
    try:
        data = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        raise ModeInvalid(f'{origin}: not TOML: {error}') from error
    if data.get('schema') != SCHEMA:
        raise ModeInvalid(f'{origin}: schema must be {SCHEMA}')
    precedence = tuple(data.get('verdict_precedence', ()))
    if sorted(precedence) != sorted(VERDICTS):
        raise ModeInvalid(f'{origin}: verdict_precedence must order every verdict exactly once')
    tables = data.get('modes') or {}
    modes = {}
    for name, table in tables.items():
        if 'replica_of' not in table:
            modes[name] = _mode(name, table, sha)
    for name, table in tables.items():
        if 'replica_of' in table:
            source = tables.get(table['replica_of'])
            if source is None or 'replica_of' in source:
                raise ModeInvalid(f'modes.{name}.replica_of: a mode with parameters of its own')
            modes[name] = _mode(name, {**source, 'description': table.get('description', '')}, sha,
                                 table['replica_of'])
    return Modes(sha, precedence, modes)


def load_modes(path=MODES_PATH):
    return parse_modes(Path(path).read_text(encoding='utf-8'), str(path))
