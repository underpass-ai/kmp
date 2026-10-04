"""Determinism control (BENCH_SPEC section 10): fresh processes on identical stores agree.

One binary answers the same work items `runs` times (default 3). Every run
starts new processes on new disposable stores: a copy of one template store for
items that read an existing store (B-real questions, full wakes), and an empty
store per item for items that bring their own memory (the judged retrieval
cases). Each item's journey follows `projection.next_action` verbatim, as
token_harness' driver does, with `max_calls` up to 256. Run k is then compared
with run 0 by the parity oracle, and a call is deterministic when it is equal
(after the documented volatile fields) in every pair.

Each run directory is a flat set of token_harness trace lines (`<item>.jsonl`,
`<item>.setup.jsonl` for a fresh store's own writes, `process-<n>.jsonl` for
handshakes), so `parity.py` and token_harness' readers take it as is. The
token_harness synthetic scenarios (W01-W04, A01, E01) run through
token_harness' own `capture` instead (`harness_determinism`).

The report records the machine architecture, so two reports (or two run
directories) from an x86_64 and an aarch64 host compare with `parity.py` too.

Private material (a B-real template, private questions) is written only
outside the repository; reports carry counts and JSON paths, never values.
"""
from dataclasses import dataclass
import hashlib
import itertools
import json
from pathlib import Path
import platform
import shutil
import sys

from ...token_harness.domain.errors import HarnessError
from ...token_harness.native.capture import CaptureOptions, capture
from ...token_harness.native.driver import MAX_CALLS_LIMIT, check_max_calls, run_journey
from ...token_harness.native.isolation import create_store, tree_digest
from ...token_harness.native.recorder import TraceRecorder
from ...token_harness.native.transport import TIMEOUT_SECONDS, StdioSession
from ..domain import cachekey
from ..domain.jsonl import REPO_ROOT, guard_private_path
from . import parity

SCHEMA = 'kmp.bench.determinism.v1'
RUN_SCHEMA = 'kmp.bench.determinism_run.v1'
MAX_REVIEW_ROUNDS = 3


class DeterminismError(HarnessError):
    code = 'DETERMINISM_SETUP_FAILED'


@dataclass(frozen=True)
class Journey:
    """What token_harness' `run_journey` needs: the first call."""
    tool: str
    arguments: dict


@dataclass(frozen=True)
class WorkItem:
    id: str  # names its trace file
    tool: str
    arguments: dict
    setup: tuple = ()  # (tool, arguments) writes into the item's own empty store first
    fresh_store: bool = False  # True: an empty store of its own; False: the run's template copy

    def as_dict(self):
        return {'id': self.id, 'tool': self.tool, 'arguments': self.arguments,
                'setup': [list(call) for call in self.setup], 'fresh_store': self.fresh_store}


def items_digest(items):
    ids = [item.id for item in items]
    if len(set(ids)) != len(ids):
        raise DeterminismError('work item ids must be unique')
    return cachekey.digest([item.as_dict() for item in items])


def question_items(questions, max_bytes=None):
    """kmp.bench.question.v1 records, each read from the run's template store."""
    return tuple(WorkItem(q.id, *q.first_call(max_bytes=max_bytes)) for q in questions)


def wake_items(abouts, budget=None):
    """A complete wake of each about; the id hides nothing private beyond the about's slug."""
    items = []
    for about in abouts:
        arguments = {'about': about}
        if budget is not None:
            arguments['budget'] = {'max_bytes': budget}
        slug = ''.join(c if c.isalnum() or c in '._-' else '_' for c in about)
        items.append(WorkItem(f'wake.{slug}', 'kmp_wake', arguments))
    return tuple(items)


def retrieval_items(cases_path, subset='original', max_entries=10):
    """The judged retrieval cases, set up and asked exactly as `retrieval_kmp_scorecard` does.

    `original` keeps the 35 cases without a `kind` (retrieval35); `all` keeps every case.
    """
    cases = json.loads(Path(cases_path).read_text(encoding='utf-8'))['cases']
    items = []
    for case in cases:
        if subset == 'original' and case.get('kind') is not None:
            continue
        setup = [('kmp_ingest', {'about': case['about'], 'idempotency_key': f'judged:{case["id"]}',
                                 'memory': case['memory']})]
        setup += [('kmp_ingest', {'about': seeded['about'], 'memory': seeded['memory'],
                                  'idempotency_key': f'judged:{case["id"]}:{seeded["about"]}'})
                  for seeded in case.get('memories', [])]
        setup += [('kmp_write_memory', write) for write in case.get('writes', [])]
        arguments = {'about': case['about'], 'question': case['question'],
                     'answer_policy': case['answer_policy'], 'depth': 3,
                     'budget': {'tokens': 2048, 'detail': 'balanced', 'max_entries': max_entries}}
        for name in ('as_of', 'interval', 'axis', 'dimensions'):
            if case.get(name) is not None:
                arguments[name] = case[name]
        items.append(WorkItem(f'retrieval.{case["id"]}', 'kmp_ask', arguments, tuple(setup), True))
    return tuple(items)


def _setup_call(session, tool, arguments, item):
    """One seeding write; a review is resolved through the continuation it returned."""
    for _ in range(MAX_REVIEW_ROUNDS + 1):
        response = session.call(tool, arguments)
        result = response.get('result') or {}
        structured = result.get('structuredContent') or {}
        if 'error' in response or result.get('isError'):
            raise DeterminismError(f'{item.id}: setup {tool} failed')
        if structured.get('status') != 'needs_review':
            return
        action = (structured.get('next_actions') or [None])[0]
        if not action:
            raise DeterminismError(f'{item.id}: review without a continuation')
        tool, arguments = action['tool'], action['arguments']
    raise DeterminismError(f'{item.id}: setup still under review')


class _Runner:
    """One run: its own stores, processes and trace files."""

    def __init__(self, binary, out, work_dir, template, max_calls, timeout_seconds):
        self.binary, self.out, self.work_dir, self.template = binary, out, work_dir, template
        self.max_calls, self.timeout_seconds = max_calls, timeout_seconds
        self.ids, self.processes, self.outcomes = itertools.count(1), itertools.count(), {}

    def _open(self, store):
        name = f'process-{next(self.processes)}'
        recorder = TraceRecorder(self.out / f'{name}.jsonl', store.redact)
        session = StdioSession(self.binary, store, recorder, name, self.ids,
                               timeout_seconds=self.timeout_seconds, probe_resources=False)
        session.handshake('memory-bench-determinism')
        return session

    def _close(self, session):
        session.close()
        session.recorder.close()

    def _journey(self, session, item):
        recorder = TraceRecorder(self.out / f'{item.id}.jsonl', session.store.redact)
        previous = session.record_into(recorder)
        try:
            outcome = run_journey(session, Journey(item.tool, item.arguments), None, self.max_calls)
        finally:
            session.record_into(previous)
            recorder.close()
        self.outcomes[item.id] = outcome.as_dict()

    def _fresh(self, item):
        store = create_store(self.work_dir, 'fresh')
        try:
            session = self._open(store)
            try:
                recorder = TraceRecorder(self.out / f'{item.id}.setup.jsonl', store.redact)
                previous = session.record_into(recorder)
                try:
                    for tool, arguments in item.setup:
                        _setup_call(session, tool, arguments, item)
                finally:
                    session.record_into(previous)
                    recorder.close()
                self._journey(session, item)
            finally:
                self._close(session)
        finally:
            shutil.rmtree(store.root)

    def _shared(self, items):
        if not items:
            return
        store = create_store(self.work_dir, 'shared', template=self.template)
        session = None
        try:
            for item in items:
                if session is None or session.broken:
                    if session is not None:
                        self._close(session)
                    session = self._open(store)
                self._journey(session, item)
        finally:
            if session is not None:
                self._close(session)
            shutil.rmtree(store.root)

    def run(self, items):
        self._shared([item for item in items if not item.fresh_store])
        for item in items:
            if item.fresh_store:
                self._fresh(item)
        return self.outcomes


def _sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def machine():
    return {'arch': platform.machine(), 'system': platform.system(),
            'kernel': platform.release(), 'python': platform.python_version()}


def check_template(template):
    """A template is a KMP data directory: FORMAT_VERSION beside store/."""
    template = Path(template)
    if not (template / 'FORMAT_VERSION').is_file() or not (template / 'store').is_dir():
        raise DeterminismError(f'{template} is not a KMP data directory (FORMAT_VERSION and store/)')
    return template


def run_once(binary, items, out, work_dir, template=None, max_calls=MAX_CALLS_LIMIT,
             timeout_seconds=TIMEOUT_SECONDS):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=False)
    if template is None and any(not item.fresh_store for item in items):
        raise DeterminismError('items reading a shared store need a template')
    outcomes = _Runner(Path(binary).resolve(), out, Path(work_dir), template, max_calls,
                       timeout_seconds).run(items)
    record = {'schema': RUN_SCHEMA, 'binary_sha256': _sha256(binary), 'machine': machine(),
              'items_digest': items_digest(items), 'max_calls': max_calls,
              'timeout_s': float(timeout_seconds), 'outcomes': outcomes}
    (out / 'determinism-run.json').write_text(json.dumps(record, indent=2, sort_keys=True) + '\n')
    return record


def _rate(keys, differing):
    total = len(keys)
    return {'calls': total, 'deterministic': total - len(keys & differing),
            'determinism_rate': (total - len(keys & differing)) / total if total else None}


def combine(pairs):
    """determinism over every pair: a call counts once and only if no pair differs on it.

    `by_kind` splits measured journeys from setup traces (a fresh store's own
    writes, token_harness fixture and oracle read-back sessions)."""
    keys, differing = set(), set()
    for report in pairs:
        keys.update((row['trace'], row['index']) for row in report.get('_calls', ()))
        differing.update((row['trace'], row['index']) for row in report['differences'])
    by_kind = {}
    for kind in ('journey', 'setup'):
        members = {key for key in keys if parity.item_of(key[0])[1] == (1 if kind == 'journey' else 0)}
        if members:
            by_kind[kind] = _rate(members, differing)
    return {**_rate(keys, differing), 'by_kind': by_kind}


def _call_keys(capture_map):
    return [{'trace': trace, 'index': e.index} for trace, exchanges in capture_map.items()
            for e in exchanges]


def compare_runs(run_dirs):
    """Parity of every run against the first, plus the combined determinism rate."""
    first = parity.load_capture(run_dirs[0])
    pairs = []
    for other in run_dirs[1:]:
        second = parity.load_capture(other)
        report = parity.compare_captures(first, second)
        report['_calls'] = _call_keys(first) + _call_keys(second)
        report['left'], report['right'] = Path(run_dirs[0]).name, Path(other).name
        pairs.append(report)
    combined = combine(pairs)
    for report in pairs:
        report.pop('_calls')
    return combined, pairs


def _outcome_counts(record):
    counts = {}
    for outcome in record['outcomes'].values():
        counts[outcome['status']] = counts.get(outcome['status'], 0) + 1
    return {'statuses': dict(sorted(counts.items())),
            'censored': sum(1 for o in record['outcomes'].values() if o['censored']),
            'calls': sum(o['calls'] for o in record['outcomes'].values())}


def determinism(binary, items, root, runs=3, template=None, max_calls=MAX_CALLS_LIMIT,
                timeout_seconds=TIMEOUT_SECONDS, private=False, label=None):
    """Run `items` `runs` times into `root/run-<k>` and write `root/determinism.json`."""
    if runs < 2:
        raise DeterminismError('determinism needs at least two runs')
    check_max_calls(max_calls)
    root = Path(root)
    if private:
        guard_private_path(root)
    template = check_template(template) if template is not None else None
    root.mkdir(parents=True, exist_ok=False)
    work = root / 'work'
    records = [run_once(binary, items, root / f'run-{k}', work, template, max_calls,
                        timeout_seconds) for k in range(runs)]
    combined, pairs = compare_runs([root / f'run-{k}' for k in range(runs)])
    report = {'schema': SCHEMA, 'label': label, 'binary_sha256': records[0]['binary_sha256'],
              'machine': machine(), 'runs': runs, 'items': len(items),
              'items_digest': items_digest(items), 'max_calls': max_calls,
              'timeout_s': float(timeout_seconds), 'private': private,
              'template_sha256': tree_digest(template) if template is not None else None,
              'outcomes': [_outcome_counts(record) for record in records],
              **combined, 'pairs': pairs}
    (root / 'determinism.json').write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    shutil.rmtree(work, ignore_errors=True)
    return report


def harness_determinism(binary, root, runs=3, max_calls=MAX_CALLS_LIMIT, scenarios=None,
                        wake_contract='kmp.wake_claim.v2'):
    """token_harness' own capture `runs` times (W01-W04, A01, E01), compared the same way."""
    from ...token_harness.scenarios import SCENARIOS
    root = Path(root)
    root.mkdir(parents=True, exist_ok=False)
    options = CaptureOptions(max_calls=max_calls, probe_resources=False)
    dirs = []
    for k in range(runs):
        capture(binary, 'baseline', wake_contract, root / f'run-{k}', root / 'work',
                scenarios or SCENARIOS, options=options)
        dirs.append(root / f'run-{k}')
    combined, pairs = compare_runs(dirs)
    report = {'schema': SCHEMA, 'label': 'token_harness', 'binary_sha256': _sha256(binary),
              'machine': machine(), 'runs': runs, 'max_calls': max_calls, 'private': False,
              **combined, 'pairs': pairs}
    (root / 'determinism.json').write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    shutil.rmtree(root / 'work', ignore_errors=True)
    return report


def _summary(report):
    pairs = [{k: p[k] for k in ('left', 'right', 'calls', 'identical', 'normalized', 'different',
                                 'parity_rate', 'by_kind', 'volatile_reconciled', 'path_counts')}
             for p in report['pairs']]
    return {'label': report['label'], 'arch': report['machine']['arch'], 'runs': report['runs'],
            'calls': report['calls'], 'determinism_rate': report['determinism_rate'],
            'by_kind': report['by_kind'],
            'outcomes': report.get('outcomes'), 'pairs': pairs}


def main(argv=None):
    import argparse
    parser = argparse.ArgumentParser(prog='determinism', description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True, help='new directory for the runs')
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--template', type=Path, help='KMP data directory copied into every run')
    parser.add_argument('--questions', type=Path, help='kmp.bench.question.v1 JSONL')
    parser.add_argument('--wake-about', action='append', default=[], help='complete wake of this about')
    parser.add_argument('--retrieval-cases', type=Path)
    parser.add_argument('--retrieval-subset', choices=('original', 'all'), default='original')
    parser.add_argument('--harness-scenarios', action='store_true',
                        help='token_harness W01-W04, A01, E01 through its own capture')
    parser.add_argument('--max-calls', type=int, default=MAX_CALLS_LIMIT)
    parser.add_argument('--timeout', type=float, default=TIMEOUT_SECONDS)
    parser.add_argument('--private', action='store_true', help='refuse to write inside the repo')
    parser.add_argument('--label')
    args = parser.parse_args(argv)
    try:
        if args.harness_scenarios:
            report = harness_determinism(args.binary, args.out, args.runs, args.max_calls)
        else:
            items, private = (), args.private
            if args.questions:
                from ..domain.question import load_questions
                questions = load_questions(args.questions)
                private = private or any(q.private for q in questions)
                items += question_items(questions)
            items += wake_items(args.wake_about)
            if args.retrieval_cases:
                items += retrieval_items(args.retrieval_cases, args.retrieval_subset)
            if not items:
                parser.error('nothing to run: --questions, --wake-about, --retrieval-cases '
                             'or --harness-scenarios')
            if args.template is not None and REPO_ROOT.resolve() not in args.template.resolve().parents:
                private = True  # a store from outside the repository is treated as private
            report = determinism(args.binary, items, args.out, args.runs, args.template,
                                 args.max_calls, args.timeout, private, args.label)
    except HarnessError as error:
        print(str(error), file=sys.stderr)
        return 2
    print(json.dumps(_summary(report), indent=2, sort_keys=True))
    return 0 if report['determinism_rate'] == 1.0 else 1


if __name__ == '__main__':
    sys.exit(main())
