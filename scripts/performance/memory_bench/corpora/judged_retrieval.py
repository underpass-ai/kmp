"""The judged retrieval collection (`crates/kmp-testkit/judged/retrieval_cases.json`) over stdio.

A port of `crates/kmp-testkit/src/bin/retrieval_kmp_scorecard.rs` that runs
every case on the release binary through MCP stdio instead of the in-process
server, and scores it with the same arithmetic:

- each case gets a fresh store (one process), because the BM25 collection is
  whatever the store holds and one case must not weight another's terms;
- the case's ingests and writes land first, a write that asks for a review is
  resolved through the continuation it returned (the fixture making its own
  judgement, #691), then one `kmp_ask` with the scorecard's exact arguments;
- the answer is read with `domain.refs` and scored with `domain.metrics`
  (`RetrievalOutcome` / `RetrievalScorecard`, ported 1:1); the guarded cases
  with `AskVerdict` / `GuardedDecision` / `GuardedScorecard` below, ported from
  `crates/kmp-testkit/src/retrieval_scorecard.rs`;
- the rows are those of `docs/development/retrieval-baseline.tsv`: the original
  35 (cases without `kind`) unprefixed, `all_*` over every case with a judged
  answer, `guarded_*` and `false_answer_rate_<kind>` over the guarded cases.

An arm (`rerank='narrow'|'wide'`, `max_entries != 10`) mirrors
`RETRIEVAL_RERANK` / `RETRIEVAL_MAX_ENTRIES`: it runs only the original cases
(its cassette holds only their requests) and is reported, never gated.
"""
from dataclasses import dataclass
import json
import time

from ..domain import metrics, refs
from ..domain.errors import BenchError
from .judged_stdio import commit_with_review, compact, inline_store_file, require_accepted

CORPUS = 'retrieval-judged'
DEFAULT_CASES = 'crates/kmp-testkit/judged/retrieval_cases.json'
DEFAULT_BASELINE = 'docs/development/retrieval-baseline.tsv'
DEFAULT_BRIDGE = 'crates/kmp-testkit/judged/lexical-bridge.kmpb'
DEFAULT_ARM_CASSETTE = 'crates/kmp-testkit/judged/retrieval.jev.cassette.json'
TYPESAFE = '{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.13.0","timeout_ms":20000}'
RERANK = {'narrow': '{"pool_size":40}', 'wide': '{"pool_size":400,"excerpt_chars":300}'}
UNAVAILABLE = ('rerank unavailable', 'rerank disabled')
TOLERANCE = 1e-9  # baseline_failures' hair: one ulp off never fails a change that moved nothing


class JudgedCaseInvalid(BenchError):
    code = 'JUDGED_CASE_INVALID'


class RerankNotApplied(BenchError):
    """An arm's ask warned that re-ranking did not run: the arm measured nothing."""
    code = 'RERANK_NOT_APPLIED'


@dataclass(frozen=True)
class RetrievalCase:
    id: str
    probes: str
    about: str
    question: str
    answer_policy: str
    judged: tuple
    memory: dict
    as_of: object = None
    interval: object = None
    axis: str | None = None
    nearest_outside: str | None = None
    dimensions: object = None
    memories: tuple = ()  # ({about, memory}, ...)
    writes: tuple = ()
    kind: str | None = None
    forbidden: tuple = ()
    absent: tuple = ()

    @classmethod
    def from_dict(cls, data):
        try:
            return cls(
                id=data['id'], probes=data['probes'], about=data['about'],
                question=data['question'], answer_policy=data['answer_policy'],
                judged=tuple(data['judged']), memory=data['memory'], as_of=data.get('as_of'),
                interval=data.get('interval'), axis=data.get('axis'),
                nearest_outside=data.get('nearest_outside'), dimensions=data.get('dimensions'),
                memories=tuple(data.get('memories') or ()), writes=tuple(data.get('writes') or ()),
                kind=data.get('kind'), forbidden=tuple(data.get('forbidden') or ()),
                absent=tuple(data.get('absent') or ()))
        except (KeyError, TypeError) as error:
            raise JudgedCaseInvalid(f'case {data.get("id") if isinstance(data, dict) else "?"}: '
                                    f'{type(error).__name__}: {error}') from error

    @property
    def original(self):
        """One of the 35 cases the unprefixed baseline rows score."""
        return self.kind is None

    @property
    def guarded(self):
        """A right outcome includes declining to answer or to cite."""
        return not self.judged or bool(self.forbidden)

    def ask_arguments(self, max_entries=10):
        arguments = {'about': self.about, 'question': self.question,
                     'answer_policy': self.answer_policy, 'depth': 3,
                     'budget': {'tokens': 2048, 'detail': 'balanced', 'max_entries': max_entries}}
        for name in ('as_of', 'interval', 'axis', 'dimensions'):
            if getattr(self, name) is not None:
                arguments[name] = getattr(self, name)
        return arguments


def load_cases(path):
    with open(path, encoding='utf-8') as handle:
        collection = json.load(handle)
    cases = tuple(RetrievalCase.from_dict(item) for item in collection['cases'])
    ids = [case.id for case in cases]
    if len(set(ids)) != len(ids):
        raise JudgedCaseInvalid('duplicate case id')
    return cases


# --- Guarded decisions: retrieval_scorecard.rs, ported ----------------------------------------

ANSWER, PARTIAL, UNKNOWN = 'ANSWER', 'PARTIAL', 'UNKNOWN'


@dataclass(frozen=True)
class AskVerdict:
    """`AskVerdict::read`: UNKNOWN, PARTIAL (naming an absent word or not) or an answer."""
    label: str
    names_absent: bool = False

    @classmethod
    def read(cls, answer, absent):
        value = answer.get('answer') if isinstance(answer, dict) else None
        if value == UNKNOWN:
            return cls(UNKNOWN)
        if value == PARTIAL:
            proof = answer.get('proof')
            missing = compact(proof.get('missing') if isinstance(proof, dict) else None).lower()
            return cls(PARTIAL, any(word.lower() in missing for word in absent))
        return cls(ANSWER)

    def abstains(self):
        return self.label == UNKNOWN or (self.label == PARTIAL and self.names_absent)


@dataclass(frozen=True)
class GuardedDecision:
    kind: str
    negative: bool
    verdict: AskVerdict
    cited_forbidden: bool
    high_confidence: bool

    def is_false_answer(self):
        return (self.negative and not self.verdict.abstains()) or self.cited_forbidden


@dataclass(frozen=True)
class GuardedScorecard:
    cases: int
    false_answer_rate: float
    high_false_answer_rate: float
    by_kind: tuple  # ((kind, cases, false answers), ...) ordered by kind

    @classmethod
    def score(cls, decisions):
        decisions = tuple(decisions)
        cases = len(decisions)

        def rate(count):
            return 0.0 if cases == 0 else count / cases

        grouped = {}
        for decision in decisions:
            slot = grouped.setdefault(decision.kind, [0, 0])
            slot[0] += 1
            slot[1] += int(decision.is_false_answer())
        false_answers = sum(1 for d in decisions if d.is_false_answer())
        high = sum(1 for d in decisions if d.is_false_answer() and d.high_confidence)
        # Rust orders the BTreeMap by the kind's bytes; Python str order is the same code-point order.
        by_kind = tuple((kind, total, bad) for kind, (total, bad) in sorted(grouped.items()))
        return cls(cases, rate(false_answers), rate(high), by_kind)

    def kind_rates(self):
        return tuple((kind, bad / max(total, 1)) for kind, total, bad in self.by_kind)


# --- One case -----------------------------------------------------------------------------------

@dataclass(frozen=True)
class CaseResult:
    case: RetrievalCase
    outcome: metrics.RetrievalOutcome
    verdict: AskVerdict
    confidence: str
    to_judged: int

    def decision(self):
        if not self.case.guarded:
            return None
        return GuardedDecision(self.case.kind or 'original', not self.case.judged, self.verdict,
                               any(item in self.outcome.cited for item in self.case.forbidden),
                               self.confidence == 'high')


def _bytes_to_judged(answer, judged):
    """What a reader takes in before the first judged memory (all proof items when none is)."""
    proof = answer.get('proof') if isinstance(answer, dict) else None
    items = proof.get('evidence') if isinstance(proof, dict) else None
    items = items if isinstance(items, list) else []
    stop = len(items)
    for index, item in enumerate(items):
        if refs.memory_ref(item) in judged:
            stop = index + 1
            break
    return sum(len(compact(item).encode('utf-8')) for item in items[:stop])


def read_answer(case, answer, elapsed_millis=0):
    """The scorecard's reading of one `kmp_ask` answer (`run_case` after the call)."""
    verdict = AskVerdict.read(answer, case.absent)
    proof = answer.get('proof') if isinstance(answer, dict) else None
    confidence = proof.get('confidence') if isinstance(proof, dict) else None
    confidence = confidence if isinstance(confidence, str) else 'none'
    projection = answer.get('projection') if isinstance(answer, dict) else None
    budget = projection.get('budget') if isinstance(projection, dict) else None
    used = budget.get('used_bytes') if isinstance(budget, dict) else None
    used = used if isinstance(used, int) and not isinstance(used, bool) and used >= 0 else 0
    to_judged = _bytes_to_judged(answer, case.judged)
    if case.nearest_outside is not None:
        named = refs.nearest_outside_ref(answer) if refs.is_unknown(answer) else None
        named = () if named is None else (named,)
        outcome = metrics.RetrievalOutcome.of((case.nearest_outside,), named, named, False, used,
                                              elapsed_millis)
    else:
        outcome = metrics.RetrievalOutcome.of(case.judged, refs.evidence_refs(answer),
                                              refs.cited_refs(answer), refs.is_unknown(answer),
                                              used, elapsed_millis)
    return CaseResult(case, outcome, verdict, confidence, to_judged)


def _warnings(answer):
    return compact(answer.get('warnings') if isinstance(answer, dict) else None)


def run_case(stores, case, rerank=None, max_entries=10, clock=time.monotonic):
    """Seed a fresh store with the case, ask once, read the answer."""
    files = () if rerank is None else (inline_store_file('typesafe.json', TYPESAFE),
                                       inline_store_file('rerank.json', RERANK[rerank]))
    with stores.open(f'retrieval-{case.id}', files) as server:
        require_accepted(server.call('kmp_ingest', {
            'about': case.about, 'idempotency_key': f'judged:{case.id}', 'memory': case.memory}),
            f'case {case.id} kmp_ingest')
        for seeded in case.memories:
            require_accepted(server.call('kmp_ingest', {
                'about': seeded['about'], 'idempotency_key': f'judged:{case.id}:{seeded["about"]}',
                'memory': seeded['memory']}), f'case {case.id} kmp_ingest {seeded["about"]}')
        for index, write in enumerate(case.writes):
            commit_with_review(server, write, f'case {case.id} write {index}')
        started = clock()
        answer = server.call('kmp_ask', case.ask_arguments(max_entries))
        elapsed = int((clock() - started) * 1000)
    warnings = _warnings(answer)
    if any(marker in warnings for marker in UNAVAILABLE):
        raise RerankNotApplied(f'case {case.id}: {warnings}')
    return read_answer(case, answer, elapsed)


# --- The collection -----------------------------------------------------------------------------

@dataclass(frozen=True)
class BaselineRow:
    name: str
    value: float
    bound: str  # exact | floor | ceiling
    extended: bool


def baseline_rows(original, every, guarded):
    """`baseline_rows`: file order, the original 35 first."""
    rows = [BaselineRow('cases', float(original.cases), 'exact', False)]
    rows += [BaselineRow(name, value, 'floor', False) for name, value in original.quality_columns()]
    rows.append(BaselineRow('false_unknown_rate', original.false_unknown_rate, 'ceiling', False))
    rows.append(BaselineRow('all_positives', float(every.cases), 'exact', True))
    rows += [BaselineRow(f'all_{name}', value, 'floor', True) for name, value in every.quality_columns()]
    rows.append(BaselineRow('all_false_unknown_rate', every.false_unknown_rate, 'ceiling', True))
    rows.append(BaselineRow('guarded_cases', float(guarded.cases), 'exact', True))
    rows.append(BaselineRow('guarded_false_answer_rate', guarded.false_answer_rate, 'ceiling', True))
    rows.append(BaselineRow('guarded_high_false_answer_rate', guarded.high_false_answer_rate,
                            'ceiling', True))
    rows += [BaselineRow(f'false_answer_rate_{kind}', value, 'ceiling', True)
             for kind, value in guarded.kind_rates()]
    return tuple(rows)


@dataclass(frozen=True)
class CollectionScore:
    results: tuple  # CaseResult, collection order
    original: metrics.RetrievalScorecard
    every: metrics.RetrievalScorecard
    guarded: GuardedScorecard
    rows: tuple
    arm: str | None
    max_entries: int

    @property
    def gated(self):
        return self.arm is None and self.max_entries == 10

    def row_map(self):
        return {row.name: row.value for row in self.rows}

    def mean_bytes_to_judged(self):
        values = [r.to_judged for r in self.results if r.case.original]
        return sum(values) / max(len(values), 1)


def score(results, arm=None, max_entries=10):
    results = tuple(results)
    original = metrics.RetrievalScorecard.score(r.outcome for r in results if r.case.original)
    every = metrics.RetrievalScorecard.score(r.outcome for r in results if r.case.judged)
    guarded = GuardedScorecard.score(d for d in (r.decision() for r in results) if d is not None)
    return CollectionScore(results, original, every, guarded,
                           baseline_rows(original, every, guarded), arm, max_entries)


def run_collection(stores, cases, rerank=None, max_entries=10, progress=None):
    """Every case (only the original ones under an arm, as the scorecard does), scored."""
    chosen = tuple(case for case in cases if rerank is None or case.original)
    results = []
    for case in chosen:
        result = run_case(stores, case, rerank, max_entries)
        results.append(result)
        if progress is not None:
            progress(result)
    arm = rerank if rerank is not None else ('off' if max_entries != 10 else None)
    return score(results, arm, max_entries)


# --- The recorded baseline ----------------------------------------------------------------------

def parse_baseline(text):
    """`metric<TAB>bound` rows of a baseline TSV, comments and header skipped."""
    rows = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith('#') or line.startswith('metric\t'):
            continue
        name, sep, bound = line.partition('\t')
        if not sep:
            raise JudgedCaseInvalid(f'malformed baseline row: {line}')
        try:
            rows[name] = float(bound)
        except ValueError as error:
            raise JudgedCaseInvalid(f'malformed baseline bound: {line}') from error
    return rows


@dataclass(frozen=True)
class RowCheck:
    name: str
    recorded: float | None
    measured: float | None
    delta: float | None
    within: bool


def compare_to_recorded(measured, recorded, tolerance=1e-4, names=None):
    """Reproduction check: |measured - recorded| <= tolerance for every recorded row.

    The recorded file truncates floors and rounds ceilings up to 4 decimals, so
    parity at 1e-4 is the tightest check the file supports. `names` restricts it.
    """
    checks = []
    for name in (names if names is not None else sorted(set(recorded) | set(measured))):
        got, want = measured.get(name), recorded.get(name)
        if got is None or want is None:
            checks.append(RowCheck(name, want, got, None, False))
            continue
        delta = got - want
        checks.append(RowCheck(name, want, got, delta, abs(delta) <= tolerance + TOLERANCE))
    return tuple(checks)


ORIGINAL_ROWS = ('cases', 'recall_at_1', 'recall_at_5', 'recall_at_10', 'mean_reciprocal_rank',
                 'ndcg_at_10', 'answer_core_precision', 'false_unknown_rate')
