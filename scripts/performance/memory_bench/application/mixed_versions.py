"""Mixed versions and several processes on one store (BENCH_SPEC section 10, BT18).

Every scenario opens one data directory with one or more kmp-mcp processes and
ends in a binary result (`pass`, `fail`, or `skipped` with a reason) plus the
wall latency of what it measured:

- `write_then_read`: one binary writes, another reads the same directory. Run as
  reference -> candidate and candidate -> reference; with an old binary also
  reference -> old and old -> reference. A reader that does not understand the
  format passes only if it refuses with a format message and leaves the store
  byte-identical; the bundle path (export/import) is then tried and reported.
- `concurrent_writers`: two processes write at the same time into one about. A
  `conflict` answer (the kernel's optimistic concurrency: nothing applied) is
  retried with the same idempotency key, as the server advises, and counted; a
  fresh reader must then see every entry, the log must grow by exactly the
  accepted ingests, and the kernel database must pass `integrity_check`.
- `reads_during_writes`: `kmp_ask` and `kmp_wake` from one process while another
  writes; no read fails, and the reader sees the other process's writes without
  a restart. Latency is reported idle and under write load.
- `sidecar_catch_up` (P12): the candidate builds its lexical sidecar, a binary
  that ignores the sidecar writes, the candidate reads again; its answers must
  equal those of the same binary rebuilding the sidecar from scratch, and any
  shadow telemetry must report zero differences.
- `book_first_wins` (P5): two processes judge the same questions at once over
  one verdict book; the book holds one verdict per key, both processes answer
  with the same bytes (the first verdict won), and a third process with the
  network blocked replays everything from the book.

The last two need product features. They run only when the candidate binary
declares the capability (it names the sidecar or book file) and are otherwise
`skipped: not in this binary`; tests drive them against a fake. The book
scenario also needs Jev answers without the network: the CLI gives its store
`typesafe.json` and `rerank.json` and a stand-in cassette built from a warm-up
process (`runtime/jev_stand_in`), behind the book like the provider.

Ports (duck-typed; `runtime/shared_store.py` is the real adapter):

- Store: `open(binary, name) -> Server` (raises `OpenRefused`), `fingerprint()`,
  `format_stamp()`, `has_file(rel)`, `remove_file(rel)`, `query(rel, sql)`,
  `integrity()`, `events(binary) -> (count, digest)`, `transfer(writer, reader,
  label) -> Store`, `fork(label, offline=False) -> Store`, `close()`.
- Server: `call(tool, arguments) -> Reply`, `close() -> ServerExit`.
"""
from dataclasses import dataclass, field
import json
import re
import threading

from ...token_harness.domain.errors import HarnessError
from ..domain import refs
from ..domain.stats import quantile
from ..runtime.shared_store import OpenRefused

SCHEMA = 'kmp.bench.mixed_versions.v1'
PASS, FAIL, SKIPPED = 'pass', 'fail', 'skipped'
NOT_IN_BINARY = 'not in this binary'
SIDECAR, BOOK = 'sidecar', 'book'
# Static capability probe: a binary that implements the feature names its file.
CAPABILITY_MARKERS = {SIDECAR: b'lexical-index.sqlite3', BOOK: b'judgements.sqlite3'}
# DESIGN L3 (a) and L4 4a say "store/<file>"; the bench's Jev fixtures put the book in the
# data directory itself. Both places are looked at, first match wins.
SIDECAR_FILES = ('store/lexical-index.sqlite3', 'lexical-index.sqlite3')
BOOK_FILES = ('store/judgements.sqlite3', 'judgements.sqlite3')
BOOK_ROWS_SQL = 'SELECT hex(key), hex(value) FROM verdicts'
FORMAT_REFUSAL = re.compile(r'format version|format \d+ is not supported|unsupported format|'
                            r'newer than this binary', re.IGNORECASE)
VOLATILE_KEYS = ('continuation', 'next_cursor')
SHADOW_COUNTERS = ('differences', 'diffs', 'mismatches')
MAX_READ_LOOPS = 400
MAX_CONFLICT_RETRIES = 5
CONFLICT = re.compile(r'"code": "conflict"')
SCENARIOS = ('write_then_read', 'concurrent_writers', 'reads_during_writes', 'sidecar_catch_up',
             'book_first_wins')


# -- values -------------------------------------------------------------------------------

@dataclass(frozen=True)
class BinaryRef:
    role: str  # reference | candidate | old
    path: object
    sha256: str
    version: str | None
    capabilities: frozenset = frozenset()

    @property
    def label(self):
        return f'{self.role} ({self.version or "unknown version"})'

    def as_dict(self):
        return {'role': self.role, 'path': str(self.path), 'sha256': self.sha256,
                'version': self.version, 'capabilities': sorted(self.capabilities)}


@dataclass(frozen=True)
class Settings:
    about: str = 'synth:mono-a000'
    writes: int = 6  # ingests per writing process
    reads: int = 4  # idle asks and wakes before the concurrent phase
    questions: tuple = ('Which gateway fronts every public endpoint of the platform?',
                        'Who mapped the tumi and roro inbox into one?')

    def __post_init__(self):
        if not 1 <= self.writes <= 200 or not 1 <= self.reads <= 200:
            raise ValueError('writes and reads must be in 1..200')

    def as_dict(self):
        return {'about': self.about, 'writes': self.writes, 'reads': self.reads,
                'questions': len(self.questions)}


@dataclass(frozen=True)
class Check:
    name: str
    ok: bool
    detail: str = ''
    gating: bool = True

    def as_dict(self):
        return {'name': self.name, 'ok': self.ok, 'detail': self.detail, 'gating': self.gating}


@dataclass(frozen=True)
class ScenarioResult:
    scenario: str
    case: str  # e.g. "reference->candidate"
    status: str
    reason: str | None
    checks: tuple = ()
    latency: dict = field(default_factory=dict)  # name -> summary()
    facts: dict = field(default_factory=dict)

    def as_dict(self):
        return {'scenario': self.scenario, 'case': self.case, 'status': self.status,
                'reason': self.reason, 'checks': [c.as_dict() for c in self.checks],
                'latency_ms': self.latency, 'facts': self.facts}


def conclude(scenario, case, checks, latency=None, facts=None):
    failed = [c.name for c in checks if c.gating and not c.ok]
    return ScenarioResult(scenario, case, FAIL if failed else PASS,
                          ('failed: ' + ', '.join(failed)) if failed else None, tuple(checks),
                          {k: summary(v) for k, v in (latency or {}).items()}, dict(facts or {}))


def skipped(scenario, case, reason, facts=None):
    return ScenarioResult(scenario, case, SKIPPED, reason, (), {}, dict(facts or {}))


def summary(values):
    """n, p50, p95 and max of wall ms (p95 by interpolation: indicative below n=20)."""
    values = list(values)
    if not values:
        return {'n': 0, 'p50': None, 'p95': None, 'max': None}
    return {'n': len(values), 'p50': round(quantile(values, 0.5), 3),
            'p95': round(quantile(values, 0.95), 3), 'max': round(max(values), 3)}


@dataclass(frozen=True)
class Environment:
    """What the scenarios run against; `new_store(label, seeded=True, jev=False)` makes a Store."""
    new_store: object
    reference: BinaryRef
    candidate: BinaryRef
    old: BinaryRef | None = None
    jev_available: bool = False  # a replay setup exists for the book scenario
    # (store, binary, settings) -> None: makes a jev store's judgements answerable offline
    prepare_jev: object = None


# -- writes and reads ---------------------------------------------------------------------

def _letters(index, width=3):
    out = ''
    for _ in range(width):
        index, digit = divmod(index, 26)
        out = chr(ord('a') + digit) + out
    return out


@dataclass(frozen=True)
class PlannedWrite:
    ref: str
    token: str
    arguments: dict

    @property
    def question(self):
        return f'Where did the {self.token} quorum move?'


def planned_writes(tag, count, about, declare_dimensions=False):
    """`count` one-entry ingests with a word no other entry holds (letters only: one token)."""
    if not re.fullmatch(r'[a-z]{2,8}', tag):
        raise ValueError('tag must be 2-8 lowercase letters')
    writes = []
    for index in range(count):
        token = f'zor{tag}{_letters(index)}'
        ref = f'{about}:bt18-{tag}-{index:03d}'
        entry = {'id': ref, 'kind': 'decision',
                 'text': f'The {token} quorum moved to the {tag} ledger after review.',
                 'coordinates': [{'dimension': 'work', 'scope_id': 'work:main',
                                  'occurred_at': '2026-09-01T00:00:00Z',
                                  'valid_from': '2026-09-01T00:00:00Z', 'sequence': 900000 + index}]}
        dimensions = [{'id': 'work:main', 'kind': 'work'}] if declare_dimensions and index == 0 else []
        writes.append(PlannedWrite(ref, token, {
            'about': about, 'idempotency_key': f'bt18:{tag}:{index}',
            'memory': {'dimensions': dimensions, 'entries': [entry], 'relations': []}}))
    return tuple(writes)


def accepted(reply):
    memory = (reply.structured or {}).get('memory') or {}
    return reply.ok and (memory.get('accepted') or {}).get('entries') == 1


def ask(server, about, question):
    return server.call('kmp_ask', {'about': about, 'question': question,
                                   'answer_policy': 'evidence_or_unknown'})


def inspect(server, about, ref):
    """kmp_inspect by ref: v0.23.0 requires `about`, 0.2.x refuses it, so retry without."""
    reply = server.call('kmp_inspect', {'about': about, 'ref': ref})
    if not reply.ok and 'no argument `about`' in (reply.error or ''):
        return server.call('kmp_inspect', {'ref': ref})
    return reply


def is_conflict(reply):
    """The kernel's optimistic-concurrency refusal: nothing applied, retry with the same key."""
    return not reply.ok and CONFLICT.search(reply.error or '') is not None


def write_all(server, writes, latency, conflicts=None):
    """Ingest each write as an agent would: a `conflict` is retried with the same
    idempotency key (the server's own advice), at most MAX_CONFLICT_RETRIES times.
    `latency` gets one wall time per logical write, retries included; `conflicts`, when
    given, one item per conflict answered."""
    replies = []
    for write in writes:
        spent, attempt = 0.0, 0
        while True:
            reply = server.call('kmp_ingest', write.arguments)
            spent += reply.wall_ms
            if not is_conflict(reply) or attempt == MAX_CONFLICT_RETRIES:
                break
            attempt += 1
            if conflicts is not None:
                conflicts.append(write.ref)
        latency.append(spent)
        replies.append(reply)
    return replies


def read_back(server, about, writes, latency=None):
    """(inspected, cited, errors): each write found by ref, and cited by an ask for its word.

    `latency`, when given, is a dict whose `inspect` and `ask` lists receive wall ms.
    """
    latency = latency if latency is not None else {'inspect': [], 'ask': []}
    inspected = cited = 0
    errors = []
    for write in writes:
        found = inspect(server, about, write.ref)
        latency['inspect'].append(found.wall_ms)
        if found.ok and ((found.structured or {}).get('object') or {}).get('ref') == write.ref:
            inspected += 1
        elif not found.ok:
            errors.append(found.error)
        answer = ask(server, about, write.question)
        latency['ask'].append(answer.wall_ms)
        if answer.ok and write.ref in refs.cited_refs(answer.structured):
            cited += 1
        elif not answer.ok:
            errors.append(answer.error)
    return inspected, cited, errors


def comparable(value):
    """The structured content with process-minted handles set aside."""
    if isinstance(value, dict):
        return {k: ('<volatile>' if k in VOLATILE_KEYS and isinstance(v, str) else comparable(v))
                for k, v in value.items()}
    if isinstance(value, list):
        return [comparable(v) for v in value]
    return value


def answers(server, about, questions, latency=None):
    replies = [ask(server, about, q) for q in questions]
    if latency is not None:
        latency.extend(r.wall_ms for r in replies)
    return replies


def differing(left, right):
    """Indexes where two answer lists differ (a failed reply always differs)."""
    return [i for i, (a, b) in enumerate(zip(left, right))
            if not (a.ok and b.ok) or comparable(a.structured) != comparable(b.structured)]


def run_parallel(jobs):
    """Run callables at the same instant (one thread each, one barrier); re-raise the first error."""
    barrier = threading.Barrier(len(jobs))
    results, errors = [None] * len(jobs), [None] * len(jobs)

    def runner(index, job):
        try:
            barrier.wait()
            results[index] = job()
        except BaseException as failure:  # handed back to the caller below
            errors[index] = failure

    threads = [threading.Thread(target=runner, args=(i, job), daemon=True) for i, job in enumerate(jobs)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    for failure in errors:
        if failure is not None:
            raise failure
    return results


def locate(store, candidates):
    return next((name for name in candidates if store.has_file(name)), None)


def _count(ok, total):
    return f'{ok}/{total}'


# -- write_then_read ----------------------------------------------------------------------

def _refusal(store, reader):
    """(server or None, refusal message or None) for `reader` on `store`."""
    try:
        server = store.open(reader, 'reader')
    except OpenRefused as failure:
        return None, str(failure)
    return server, None


def _format_refused(server, about):
    """The refusal a reader gives on its first store access (kmp_wake exists in every version)."""
    probe = server.call('kmp_wake', {'about': about})
    if not probe.ok and FORMAT_REFUSAL.search(probe.error or ''):
        return probe.error
    return None


def _bundle_check(store, writer, reader, settings, writes):
    try:
        moved = store.transfer(writer, reader, 'bundle')
    except OpenRefused as failure:
        return Check('bundle_path', False, f'refused: {str(failure)[:200]}', gating=False)
    try:
        server = moved.open(reader, 'bundle-reader')
        inspected, cited, _ = read_back(server, settings.about, writes)
        server.close()
        ok = inspected == cited == len(writes)
        return Check('bundle_path', ok, f'imported; inspected {_count(inspected, len(writes))}, '
                     f'cited {_count(cited, len(writes))}', gating=False)
    except OpenRefused as failure:
        return Check('bundle_path', False, f'imported but not served: {str(failure)[:200]}',
                     gating=False)
    finally:
        moved.close()


def write_then_read(env, writer, reader, settings):
    case = f'{writer.role}->{reader.role}'
    seeded = writer.role != 'old'  # an old binary cannot write the current template's format
    store = env.new_store(f'wtr-{writer.role}-{reader.role}', seeded=seeded)
    checks, facts = [], {}
    latency = {'write': [], 'writer_ask': [], 'reader_inspect': [], 'reader_ask': []}
    try:
        writes = planned_writes('wr', settings.writes, settings.about, declare_dimensions=not seeded)
        server = store.open(writer, 'writer')
        replies = write_all(server, writes, latency['write'])
        writer_answers = answers(server, settings.about, settings.questions, latency['writer_ask'])
        server.close()
        ok_writes = sum(map(accepted, replies))
        checks.append(Check('writes_accepted', ok_writes == len(writes), _count(ok_writes, len(writes))))
        if ok_writes != len(writes):
            facts['first_error'] = next((r.error for r in replies if not r.ok), None)
            return conclude('write_then_read', case, checks, latency, facts)
        before, facts['format_before'] = store.fingerprint(), store.format_stamp()
        server, refusal = _refusal(store, reader)
        if server is not None:
            refusal = _format_refused(server, settings.about)
        if refusal is not None:
            if server is not None:
                server.close()
            facts['path'] = 'refused_by_format'
            checks.append(Check('refused_with_format_message', bool(FORMAT_REFUSAL.search(refusal)),
                                refusal[:300]))
            checks.append(Check('store_untouched', store.fingerprint() == before,
                                'engine files and FORMAT_VERSION unchanged'))
            checks.append(_bundle_check(store, writer, reader, settings, writes))
        else:
            facts['path'] = 'direct'
            inspected, cited, errors = read_back(server, settings.about, writes, {
                'inspect': latency['reader_inspect'], 'ask': latency['reader_ask']})
            reader_answers = answers(server, settings.about, settings.questions, latency['reader_ask'])
            server.close()
            checks.append(Check('writes_visible', inspected == len(writes), _count(inspected, len(writes))))
            checks.append(Check('writes_retrievable', cited == len(writes), _count(cited, len(writes))))
            checks.append(Check('reads_without_error', not errors, '; '.join(map(str, errors))[:300]))
            diffs = differing(writer_answers, reader_answers)
            checks.append(Check('answers_equal_writer', not diffs,
                                f'{len(diffs)} of {len(settings.questions)} answers differ', gating=False))
        facts['format_after'] = store.format_stamp()
        return conclude('write_then_read', case, checks, latency, facts)
    finally:
        store.close()


# -- concurrent_writers -------------------------------------------------------------------

def concurrent_writers(env, first, second, settings):
    case = f'{first.role}+{second.role}'
    store = env.new_store(f'cw-{first.role}-{second.role}', seeded=True)
    latency = {f'write_{first.role}_a': [], f'write_{second.role}_b': []}
    checks, facts = [], {}
    try:
        events_before, _ = store.events(env.candidate)
        plans = (planned_writes('ca', settings.writes, settings.about),
                 planned_writes('cb', settings.writes, settings.about))
        servers = (store.open(first, 'writer-a'), store.open(second, 'writer-b'))
        buckets, conflicts = list(latency.values()), ([], [])
        try:
            replies = run_parallel([lambda i=i: write_all(servers[i], plans[i], buckets[i],
                                                          conflicts[i]) for i in range(2)])
        finally:
            for server in servers:
                server.close()
        total = sum(len(plan) for plan in plans)
        ok_writes = sum(accepted(r) for batch in replies for r in batch)
        checks.append(Check('writes_accepted', ok_writes == total, _count(ok_writes, total)))
        errors = [r.error for batch in replies for r in batch if not r.ok]
        facts.update(write_errors=errors[:3], conflicts_retried=[len(c) for c in conflicts])
        reader = store.open(env.candidate, 'reader')
        inspected, cited, read_errors = read_back(reader, settings.about, plans[0] + plans[1])
        reader.close()
        checks.append(Check('writes_visible', inspected == total, _count(inspected, total)))
        checks.append(Check('writes_retrievable', cited == total, _count(cited, total)))
        checks.append(Check('reads_without_error', not read_errors, '; '.join(map(str, read_errors))[:300]))
        events_after, _ = store.events(env.candidate)
        facts.update(events_before=events_before, events_after=events_after)
        checks.append(Check('log_grew_by_accepted', events_after - events_before == ok_writes,
                            f'{events_before} -> {events_after} for {ok_writes} ingests'))
        integrity = store.integrity()
        checks.append(Check('integrity_check', integrity == 'ok', integrity))
        return conclude('concurrent_writers', case, checks, latency, facts)
    finally:
        store.close()


# -- reads_during_writes ------------------------------------------------------------------

def _read_step(server, settings, step, latency, prefix):
    if step % 2 == 0:
        reply = ask(server, settings.about, settings.questions[(step // 2) % len(settings.questions)])
        latency[f'ask_{prefix}'].append(reply.wall_ms)
        return reply, 'ask'
    reply = server.call('kmp_wake', {'about': settings.about})
    latency[f'wake_{prefix}'].append(reply.wall_ms)
    return reply, 'wake'


def _answered(reply, kind):
    body = reply.structured or {}
    return reply.ok and ('answer' in body if kind == 'ask' else 'projection' in body)


def reads_during_writes(env, writer, reader, settings):
    case = f'{writer.role} writes, {reader.role} reads'
    store = env.new_store(f'rdw-{writer.role}-{reader.role}', seeded=True)
    latency = {name: [] for name in ('ask_idle', 'wake_idle', 'ask_under_write',
                                     'wake_under_write', 'write_under_reads')}
    checks, facts = [], {}
    try:
        reading, writing = store.open(reader, 'reader'), store.open(writer, 'writer')
        try:
            _read_step(reading, settings, 0, {'ask_warm': []}, 'warm')  # first-call costs out
            _read_step(reading, settings, 1, {'wake_warm': []}, 'warm')
            idle = [_read_step(reading, settings, step, latency, 'idle')
                    for step in range(2 * settings.reads)]
            writes = planned_writes('rw', settings.writes, settings.about)
            done, conflicts = threading.Event(), []

            def write_job():
                try:
                    return write_all(writing, writes, latency['write_under_reads'], conflicts)
                finally:
                    done.set()

            def read_job():
                seen, step = [], 0
                while step < MAX_READ_LOOPS and (not done.is_set() or step < 2 * settings.reads):
                    seen.append(_read_step(reading, settings, step, latency, 'under_write'))
                    step += 1
                return seen

            write_replies, loaded = run_parallel([write_job, read_job])
            ok_writes = sum(map(accepted, write_replies))
            checks.append(Check('writes_accepted', ok_writes == len(writes), _count(ok_writes, len(writes))))
            reads = idle + loaded
            good = sum(_answered(reply, kind) for reply, kind in reads)
            checks.append(Check('reads_answered', good == len(reads), _count(good, len(reads))))
            facts['first_read_error'] = next((r.error for r, _ in reads if not r.ok), None)
            facts.update(reads_under_write=len(loaded), conflicts_retried=len(conflicts))
            inspected, cited, _ = read_back(reading, settings.about, writes)
            checks.append(Check('fresh_without_restart', inspected == cited == len(writes),
                                f'inspected {_count(inspected, len(writes))}, cited '
                                f'{_count(cited, len(writes))} from the process opened before'))
        finally:
            reading.close()
            writing.close()
        integrity = store.integrity()
        checks.append(Check('integrity_check', integrity == 'ok', integrity))
        return conclude('reads_during_writes', case, checks, latency, facts)
    finally:
        store.close()


# -- sidecar_catch_up (P12) ---------------------------------------------------------------

def shadow_differences(events):
    """Sum of the difference counters on shadow telemetry lines, and how many lines."""
    total = lines = 0
    for event in events:
        fields = event.get('fields') or {}
        if 'shadow' not in str(fields.get('event', '')):
            continue
        lines += 1
        total += sum(v for k, v in fields.items() if k in SHADOW_COUNTERS and isinstance(v, int))
    return total, lines


def sidecar_catch_up(env, settings):
    candidate, writer = env.candidate, env.reference
    case = f'{writer.role} writes behind {candidate.role} sidecar'
    if SIDECAR not in candidate.capabilities:
        return skipped('sidecar_catch_up', case, NOT_IN_BINARY)
    store = env.new_store('sidecar', seeded=True)
    checks, latency = [], {'ask_after_catch_up': [], 'ask_after_rebuild': []}
    facts = {'writer_ignores_sidecar': SIDECAR not in writer.capabilities}
    rebuilt = None
    try:
        indexer = store.open(candidate, 'indexer')
        answers(indexer, settings.about, settings.questions)
        indexer.close()
        sidecar = locate(store, SIDECAR_FILES)
        checks.append(Check('sidecar_written', sidecar is not None, sidecar or 'no sidecar file'))
        if sidecar is None:
            return conclude('sidecar_catch_up', case, checks, latency, facts)
        writes = planned_writes('sc', settings.writes, settings.about)
        old = store.open(writer, 'old-writer')
        ok_writes = sum(map(accepted, write_all(old, writes, [])))
        old.close()
        checks.append(Check('writes_accepted', ok_writes == len(writes), _count(ok_writes, len(writes))))
        probes = tuple(settings.questions) + tuple(w.question for w in writes)
        reader = store.open(candidate, 'catch-up')
        caught = answers(reader, settings.about, probes)
        events = reader.close().log_events
        latency['ask_after_catch_up'] = [r.wall_ms for r in caught]
        cited = sum(w.ref in refs.cited_refs(r.structured) for w, r in zip(writes, caught[-len(writes):]))
        checks.append(Check('catch_up_retrieves_new_writes', cited == len(writes),
                            _count(cited, len(writes))))
        rebuilt = store.fork('sidecar-rebuild')
        rebuilt.remove_file(sidecar)
        fresh_reader = rebuilt.open(candidate, 'rebuild')
        fresh = answers(fresh_reader, settings.about, probes)
        events += fresh_reader.close().log_events
        latency['ask_after_rebuild'] = [r.wall_ms for r in fresh]
        diffs = differing(caught, fresh)
        checks.append(Check('equal_to_rebuild', not diffs, f'{len(diffs)} of {len(probes)} answers differ'))
        total, lines = shadow_differences(events)
        facts['shadow_lines'] = lines
        checks.append(Check('no_shadow_differences', total == 0,
                            f'{total} differences on {lines} shadow lines'))
        checks.append(Check('sidecar_rebuilt', rebuilt.has_file(sidecar), sidecar, gating=False))
        return conclude('sidecar_catch_up', case, checks, latency, facts)
    finally:
        if rebuilt is not None:
            rebuilt.close()
        store.close()


# -- book_first_wins (P5) -----------------------------------------------------------------

def judgement_lines(events):
    return [e.get('fields') or {} for e in events
            if (e.get('fields') or {}).get('event') == 'kmp_judgement']


def book_first_wins(env, settings):
    candidate = env.candidate
    case = f'{candidate.role} x2 on one book'
    if BOOK not in candidate.capabilities:
        return skipped('book_first_wins', case, NOT_IN_BINARY)
    if not env.jev_available:
        return skipped('book_first_wins', case, 'needs a Jev replay setup for this store')
    store = env.new_store('book', seeded=True, jev=True)
    checks, latency, facts = [], {'ask_a': [], 'ask_b': [], 'ask_replay': []}, {}
    replay = None
    try:
        if env.prepare_jev is not None:
            env.prepare_jev(store, candidate, settings)
        servers = (store.open(candidate, 'book-a'), store.open(candidate, 'book-b'))
        try:
            got = run_parallel([lambda s=s: answers(s, settings.about, settings.questions)
                                for s in servers])
        finally:
            lines = [judgement_lines(server.close().log_events) for server in servers]
        latency['ask_a'], latency['ask_b'] = [r.wall_ms for r in got[0]], [r.wall_ms for r in got[1]]
        book = locate(store, BOOK_FILES)
        checks.append(Check('book_written', book is not None, book or 'no book file'))
        if book is None:
            return conclude('book_first_wins', case, checks, latency, facts)
        rows = store.query(book, BOOK_ROWS_SQL)
        keys = [row[0] for row in rows]
        facts.update(verdicts=len(rows), judged_a=len(lines[0]), judged_b=len(lines[1]))
        checks.append(Check('no_duplicate_verdicts', bool(rows) and len(keys) == len(set(keys)),
                            f'{len(rows)} rows, {len(set(keys))} keys'))
        diffs = differing(got[0], got[1])
        checks.append(Check('first_verdict_wins', not diffs,
                            f'{len(diffs)} of {len(settings.questions)} answers differ between processes'))
        replay = store.fork('book-replay', offline=True)
        server = replay.open(candidate, 'book-replay')
        again = answers(server, settings.about, settings.questions)
        replayed = judgement_lines(server.close().log_events)
        latency['ask_replay'] = [r.wall_ms for r in again]
        from_book = bool(replayed) and all(line.get('status') == 'ok' and line.get('http_requests') == 0
                                           and line.get('source') != 'cassette_miss' for line in replayed)
        checks.append(Check('replay_from_book', from_book and not differing(got[0], again),
                            f'{len(replayed)} evaluations, answers equal: {not differing(got[0], again)}'))
        return conclude('book_first_wins', case, checks, latency, facts)
    finally:
        if replay is not None:
            replay.close()
        store.close()


# -- the whole matrix ---------------------------------------------------------------------

def plan(env):
    """(scenario, callable(settings)) in run order; the old binary adds its two directions."""
    ref, cand, old = env.reference, env.candidate, env.old
    steps = [('write_then_read', lambda s: write_then_read(env, ref, cand, s)),
             ('write_then_read', lambda s: write_then_read(env, cand, ref, s))]
    if old is not None:
        steps += [('write_then_read', lambda s: write_then_read(env, ref, old, s)),
                  ('write_then_read', lambda s: write_then_read(env, old, ref, s))]
    steps += [('concurrent_writers', lambda s: concurrent_writers(env, ref, cand, s)),
              ('concurrent_writers', lambda s: concurrent_writers(env, cand, cand, s)),
              ('reads_during_writes', lambda s: reads_during_writes(env, ref, cand, s)),
              ('reads_during_writes', lambda s: reads_during_writes(env, cand, ref, s)),
              ('sidecar_catch_up', lambda s: sidecar_catch_up(env, s)),
              ('book_first_wins', lambda s: book_first_wins(env, s))]
    return steps


def run(env, settings, scenarios=None, progress=None):
    """Run the selected scenarios; one failing scenario never stops the others."""
    chosen = set(scenarios or SCENARIOS)
    unknown = chosen - set(SCENARIOS)
    if unknown:
        raise ValueError(f'unknown scenarios: {", ".join(sorted(unknown))}')
    results = []
    for name, step in plan(env):
        if name not in chosen:
            continue
        try:
            result = step(settings)
        except HarnessError as failure:  # a setup step broke: this scenario fails, the run goes on
            result = ScenarioResult(name, '?', FAIL, f'{type(failure).__name__}: {str(failure)[:300]}')
        results.append(result)
        if progress is not None:
            progress(result)
    return report(env, settings, results)


def report(env, settings, results):
    binaries = [b.as_dict() for b in (env.reference, env.candidate, env.old) if b is not None]
    counts = {status: sum(r.status == status for r in results) for status in (PASS, FAIL, SKIPPED)}
    return {'schema': SCHEMA, 'binaries': binaries, 'settings': settings.as_dict(),
            'counts': counts, 'results': [r.as_dict() for r in results]}


def render_line(result):
    lat = ', '.join(f'{k} p50 {v["p50"]} ms' for k, v in result.latency.items() if v['n'])
    return (f'{result.status:7} {result.scenario} [{result.case}]'
            + (f' - {result.reason}' if result.reason else '') + (f' ({lat})' if lat else ''))


def dumps(value):
    return json.dumps(value, indent=1, sort_keys=True, ensure_ascii=False) + '\n'
