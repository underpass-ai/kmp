"""Jev fixtures: what each sample of a Jev arm judged, recorded once and replayed offline.

BENCH_SPEC section 9. Jev's answers are not derivable state, so they never live
in a store template; they are fixtures under `jev/<fixture key>/sample-<i>/`
(SCHEMAS.md section 7), one per sample, so the verdicts of sample 1 cannot
answer samples 2 and 3 and flatten their variance.

Two backends, chosen by the binary:

- `CASSETTE` (every binary up to P5): the per-body cassette of
  `cassette_judgement.rs`, driven by KMP_TYPESAFE_CASSETTE(_MODE). Record gives
  each sample an absent cassette the binary fills; replay copies the sample's
  recording (or, if none was recorded, the variant's named cassette) and runs
  it in cassette `replay` mode, which has no provider behind it.
- `BOOK` (a binary with the P5 verdict book, `<data dir>/judgements.sqlite3`):
  the book file is copied as opaque bytes, so this module does not depend on
  its schema. Record starts every sample from no book and asks the real
  provider; replay copies the recorded book into the store and puts an empty
  cassette in `replay` mode behind it, so a verdict missing from the book
  fails instead of reaching the network. P5 must therefore keep the book as a
  decorator in front of the cassette, and keep the cassette key on the body.

A sample may use several processes (a restart after a failure): the book is
harvested from each store before it is deleted and seeded into the next one,
so a sample keeps one book; the cassette file lives outside the store.

Replay never guesses: a failed evaluation, a cassette miss or a provider call
makes the sample `no_comparable`. A binary without `kmp_judgement` telemetry
cannot prove completeness, so its replay is `unverified`, not `complete`.
Replay refuses a provider key, which is the other half of the network block.
"""
from dataclasses import dataclass
import json
import os
from pathlib import Path
import shutil

from .. import BENCH_VERSION
from ..domain import cachekey
from ..domain.errors import BenchError
from . import judgement_log

SCHEMA = 'kmp.bench.jev_fixture.v1'
CASSETTE_SCHEMA = 'kmp.typesafe.cassette.v1'  # cassette_judgement.rs SCHEMA
CASSETTE_ENV = 'KMP_TYPESAFE_CASSETTE'
CASSETTE_MODE_ENV = 'KMP_TYPESAFE_CASSETTE_MODE'
JEV_ENV = (CASSETTE_ENV, CASSETTE_MODE_ENV)
API_KEY_ENV = 'TYPESAFE_API_KEY'
CASSETTE_FILE = 'cassette.json'
BLOCK_FILE = 'network-block.cassette.json'
BOOK_FILE = 'judgements.sqlite3'  # DESIGN 4a: beside kernel.sqlite3 in the data directory
BOOK_SIDECARS = ('-wal',)  # a killed process leaves verdicts there; -shm is rebuilt
MANIFEST_FILE = 'fixture.json'
MODES = ('record', 'replay')
CASSETTE, BOOK = 'cassette', 'book'
BACKENDS = (CASSETTE, BOOK)
DEFAULT_SAMPLES = 3  # jev-samples.sh
DEFAULT_MODEL = 'jev-1.13.0'
NOT_JUDGED = ('RUST_LOG',)  # environment that changes logging, not verdicts


class JevFixtureError(BenchError):
    code = 'JEV_FIXTURE_REFUSED'


class JevFixtureMissing(JevFixtureError):
    """Replay asked for a sample nobody recorded and no cassette stands in for it."""
    code = 'JEV_FIXTURE_MISSING'


def judged_config(variant):
    """The part of a variant that changes Jev's requests: not its mode, cassette or log filter."""
    config = variant.execution_config()
    env = {k: v for k, v in config['env'].items() if k not in JEV_ENV + NOT_JUDGED}
    return {'store_files': config['store_files'], 'env': env}


def fixture_key(*, binary_sha256, judged, store_key, questions_digest, backend,
                bench_version=BENCH_VERSION):
    """Shared by the record and the replay of one arm; the sample is a sub-directory."""
    if backend not in BACKENDS:
        raise JevFixtureError(f'backend {backend!r} is not one of {", ".join(BACKENDS)}')
    material = {'binary_sha256': cachekey.require_hex64(binary_sha256, 'binary_sha256'),
                'judged': judged, 'store_key': cachekey.require_hex64(store_key, 'store_key'),
                'questions_digest': cachekey.require_hex64(questions_digest, 'questions_digest'),
                'backend': backend}
    return cachekey.digest({'kind': 'jev', 'bench_version': bench_version, 'material': material})


@dataclass(frozen=True)
class SampleOutcome:
    sample: int
    mode: str
    status: str  # recorded, complete, no_comparable, unverified
    by_source: dict
    missing: tuple  # (site, source or 'error', request_key or error_hash) of each unanswered line
    reason: str | None

    @property
    def comparable(self):
        return self.status in ('recorded', 'complete')

    def as_dict(self):
        return {'sample': self.sample, 'mode': self.mode, 'status': self.status,
                'by_source': self.by_source, 'missing': [list(m) for m in self.missing],
                'reason': self.reason}


def _copy(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    staging = target.with_name(target.name + '.partial')
    shutil.copyfile(source, staging)
    os.replace(staging, target)


def _book_files(folder):
    return [Path(folder) / (BOOK_FILE + suffix) for suffix in ('',) + BOOK_SIDECARS]


def _move_book(source_dir, target_dir):
    """Replace the book in `target_dir` by the one in `source_dir` (absent stays absent)."""
    for source, target in zip(_book_files(source_dir), _book_files(target_dir)):
        if source.exists():
            _copy(source, target)
        else:
            target.unlink(missing_ok=True)
    Path(target_dir, BOOK_FILE + '-shm').unlink(missing_ok=True)


def empty_cassette(path, model):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps({'schema': CASSETTE_SCHEMA, 'model': model, 'entries': {}},
                                     indent=2) + '\n')


def _failure(line):
    return (line.site, 'error' if line.failed else str(line.source),
            line.request_key or line.error_hash or '')


class JevSample:
    """One sample of one arm: its environment, its store hooks and its seal."""

    def __init__(self, fixture, index, work_dir):
        self.fixture, self.index, self.mode = fixture, index, fixture.mode
        self.work = Path(work_dir)
        self.recorded = fixture.sample_dir(index)
        self.logs = []
        shutil.rmtree(self.work, ignore_errors=True)
        self.work.mkdir(parents=True)
        if self.mode == 'replay':
            self._stage_replay()

    def _recorded_manifest(self):
        path = self.recorded / MANIFEST_FILE
        if not path.exists():
            return None
        try:
            manifest = json.loads(path.read_text())
        except ValueError as error:
            raise JevFixtureError(f'{path}: not JSON: {error}') from error
        expected = {'schema': SCHEMA, 'fixture_key': self.fixture.key, 'sample': self.index,
                    'backend': self.fixture.backend}
        if {k: manifest.get(k) for k in expected} != expected:
            raise JevFixtureError(f'{path}: recorded for another fixture, sample or backend')
        return manifest

    def _stage_replay(self):
        manifest = self._recorded_manifest()
        if self.fixture.backend == BOOK:
            if manifest is None:
                raise JevFixtureMissing(f'sample {self.index}: no book recorded in {self.recorded}')
            _move_book(self.recorded, self.work)
            empty_cassette(self.work / BLOCK_FILE, self.fixture.model)
            return
        recorded = self.recorded / CASSETTE_FILE
        source = recorded if manifest is not None and recorded.exists() else self.fixture.fallback_cassette
        if source is None or not Path(source).exists():
            raise JevFixtureMissing(f'sample {self.index}: no cassette recorded in {self.recorded} '
                                    'and the variant names none')
        _copy(Path(source), self.work / CASSETTE_FILE)

    def env(self, variant_env):
        """The child's allowlisted environment for this sample (the variant's cassette is replaced)."""
        env = {k: v for k, v in (variant_env or {}).items() if k not in JEV_ENV}
        if self.fixture.backend == CASSETTE:
            env.update({CASSETTE_ENV: str(self.work / CASSETTE_FILE), CASSETTE_MODE_ENV: self.mode})
        elif self.mode == 'replay':
            env.update({CASSETTE_ENV: str(self.work / BLOCK_FILE), CASSETTE_MODE_ENV: 'replay'})
        return env

    def check_secrets(self, secrets):
        if self.mode == 'replay' and API_KEY_ENV in (secrets or {}):
            raise JevFixtureError(f'replay blocks the network: {API_KEY_ENV} is not handed to the child')

    def prepare_store(self, data_dir):
        """Before the binary starts: the sample's book so far (none on a fresh record sample)."""
        if self.fixture.backend == BOOK:
            _move_book(self.work, data_dir)

    def harvest_store(self, data_dir, lines):
        """Before the store is deleted: keep the book and the process's judgement lines."""
        if self.fixture.backend == BOOK and self.mode == 'record':
            _move_book(data_dir, self.work)
        self.logs.append(judgement_log.parse(lines))

    def seal(self):
        log = judgement_log.merge(self.logs) if self.logs else judgement_log.JudgementLog((), None)
        by_source = log.by_source()
        if self.mode == 'record':
            return self._seal_record(log, by_source)
        if not log.telemetry_present:
            return SampleOutcome(self.index, 'replay', 'unverified', by_source, (),
                                 f'{log.absent_reason}: replay completeness cannot be proven')
        missing = tuple(_failure(line) for line in log.asked_provider())
        if missing:
            return SampleOutcome(self.index, 'replay', 'no_comparable', by_source, missing,
                                 f'{len(missing)} Jev evaluation(s) not answered by the recorded '
                                 f'{self.fixture.backend}')
        return SampleOutcome(self.index, 'replay', 'complete', by_source, (), None)

    def _seal_record(self, log, by_source):
        shutil.rmtree(self.recorded, ignore_errors=True)
        self.recorded.mkdir(parents=True)
        if self.fixture.backend == BOOK:
            _move_book(self.work, self.recorded)
            content = self.recorded / BOOK_FILE
        else:
            content = self.recorded / CASSETTE_FILE
            if (self.work / CASSETTE_FILE).exists():
                _copy(self.work / CASSETTE_FILE, content)
            else:  # nothing judged: the binary never wrote the cassette
                empty_cassette(content, self.fixture.model)
        failed = tuple(_failure(line) for line in log.lines if line.failed)
        manifest = {'schema': SCHEMA, 'fixture_key': self.fixture.key, 'sample': self.index,
                    'backend': self.fixture.backend, 'model': self.fixture.model,
                    'file': content.name if content.exists() else None,
                    'sha256': cachekey.sha256_hex(content.read_bytes()) if content.exists() else None,
                    'request_keys': sorted(log.request_keys()), 'by_source': by_source,
                    'telemetry': log.telemetry_present}
        (self.recorded / MANIFEST_FILE).write_text(cachekey.canonical_json(manifest) + '\n')
        if failed:
            return SampleOutcome(self.index, 'record', 'no_comparable', by_source, failed,
                                 f'{len(failed)} Jev evaluation(s) failed while recording')
        return SampleOutcome(self.index, 'record', 'recorded', by_source, (), log.absent_reason)


@dataclass(frozen=True)
class JevFixture:
    """The fixtures of one arm: `jev/<key>/sample-<i>/` under a cache layout."""
    root: Path
    key: str
    mode: str
    backend: str
    model: str = DEFAULT_MODEL
    fallback_cassette: Path | None = None  # the variant's KMP_TYPESAFE_CASSETTE, cassette replay only

    def __post_init__(self):
        if self.mode not in MODES:
            raise JevFixtureError(f'jev mode {self.mode!r}: a fixture records or replays')
        if self.backend not in BACKENDS:
            raise JevFixtureError(f'backend {self.backend!r} is not one of {", ".join(BACKENDS)}')

    def sample_dir(self, index):
        if not isinstance(index, int) or index < 0:
            raise JevFixtureError(f'sample index {index!r}')
        return self.root / f'sample-{index}'

    def sample(self, index, work_dir):
        return JevSample(self, index, work_dir)


def open_fixture(layout, key, mode, backend, model=DEFAULT_MODEL, variant_env=None):
    """The fixture of one arm under `layout` (`jev/<key>`); replay may fall back to the variant's cassette."""
    root = layout.require_inside(layout.entry('jev', key))
    cassette = (variant_env or {}).get(CASSETTE_ENV)
    fallback = Path(cassette) if cassette and mode == 'replay' and backend == CASSETTE else None
    return JevFixture(root, key, mode, backend, model, fallback)


def sample_work_dir(layout, key, index):
    return layout.require_inside(layout.scratch() / f'jev-{key}' / f'sample-{index}')
