"""Quality metrics of the bench, as pure functions over refs.

Three families:

- **Ported 1:1** from `crates/kmp-testkit/src/retrieval_scorecard.rs` (lines
  24-151 at v0.23.0): `RetrievalOutcome` and `RetrievalScorecard`. Same
  arithmetic, same summation order, same quirks (recall de-duplicates what came
  back, nDCG does not; an empty judged set scores 0.0, an empty collection
  averages to 0.0). `crates/kmp-testkit/judged/metric_parity.json` pins both
  implementations to the same expected numbers.
- **Bench-only** per-question metrics: chains, paths, temporal and decision.
  They take plain refs so a scorer (BT10) can feed them from gold
  (`domain/gold.py`) and from `refs.evidence_refs` / `refs.cited_refs`.
- **Aggregates** return a `Rate` (hits over n). Unlike the ported scorecard, a
  bench rate over nothing is `None`, never an invented zero (SCHEMAS.md §0).
"""
from dataclasses import dataclass
import math


# --- Ported: retrieval_scorecard.rs ---------------------------------------------------------

@dataclass(frozen=True)
class RetrievalOutcome:
    """`RetrievalOutcome`: judged refs, what came back in order, what the answer cited."""
    judged: frozenset
    retrieved: tuple
    cited: frozenset = frozenset()
    unknown: bool = False
    used_bytes: int = 0
    elapsed_millis: int = 0

    @classmethod
    def of(cls, judged, retrieved, cited=(), unknown=False, used_bytes=0, elapsed_millis=0):
        return cls(frozenset(judged), tuple(retrieved), frozenset(cited), bool(unknown),
                   int(used_bytes), int(elapsed_millis))

    def has_complete_support_at(self, k):
        top = self.retrieved[:k]
        return bool(self.judged) and all(required in top for required in self.judged)

    def recall_at(self, k):
        if not self.judged:
            return 0.0
        seen = {item for item in self.retrieved[:k] if item in self.judged}
        return len(seen) / len(self.judged)

    def reciprocal_rank(self):
        for index, item in enumerate(self.retrieved):
            if item in self.judged:
                return 1.0 / (index + 1)
        return 0.0

    def ndcg_at(self, k):
        if not self.judged:
            return 0.0

        def gain(index):
            return 1.0 / math.log2(index + 2)

        earned = 0.0
        for index, item in enumerate(self.retrieved[:k]):
            if item in self.judged:
                earned += gain(index)  # duplicates earn again, as in Rust
        ideal = 0.0
        for index in range(min(len(self.judged), k)):
            ideal += gain(index)
        if earned <= 0.0 or ideal <= 0.0:
            return 0.0
        return earned / ideal

    def answer_cites_judged(self):
        return any(item in self.judged for item in self.cited)

    def is_false_unknown(self):
        return self.unknown and bool(self.judged)


QUALITY_COLUMNS = ('recall_at_1', 'recall_at_5', 'recall_at_10', 'mean_reciprocal_rank',
                   'ndcg_at_10', 'answer_core_precision')


@dataclass(frozen=True)
class RetrievalScorecard:
    """`RetrievalScorecard::score`: plain means over the collection, 0.0 when empty."""
    cases: int
    recall_at_1: float
    recall_at_5: float
    recall_at_10: float
    mean_reciprocal_rank: float
    ndcg_at_10: float
    answer_core_precision: float
    false_unknown_rate: float
    mean_used_bytes: float
    mean_elapsed_millis: float

    @classmethod
    def score(cls, outcomes):
        outcomes = tuple(outcomes)
        cases = len(outcomes)

        def mean(value):
            if cases == 0:
                return 0.0
            total = 0.0
            for outcome in outcomes:  # sequential sum, the order Iterator::sum uses
                total += value(outcome)
            return total / cases

        return cls(
            cases=cases,
            recall_at_1=mean(lambda o: o.recall_at(1)),
            recall_at_5=mean(lambda o: o.recall_at(5)),
            recall_at_10=mean(lambda o: o.recall_at(10)),
            mean_reciprocal_rank=mean(RetrievalOutcome.reciprocal_rank),
            ndcg_at_10=mean(lambda o: o.ndcg_at(10)),
            answer_core_precision=mean(lambda o: float(o.answer_cites_judged())),
            false_unknown_rate=mean(lambda o: float(o.is_false_unknown())),
            mean_used_bytes=mean(lambda o: float(o.used_bytes)),
            mean_elapsed_millis=mean(lambda o: float(o.elapsed_millis)),
        )

    def quality_columns(self):
        return tuple((name, getattr(self, name)) for name in QUALITY_COLUMNS)


# --- Aggregates -----------------------------------------------------------------------------

@dataclass(frozen=True)
class Rate:
    """A proportion with its counts. `value` is None over an empty denominator."""
    hits: int
    n: int

    def __post_init__(self):
        if self.n < 0 or not 0 <= self.hits <= self.n:
            raise ValueError(f'invalid rate {self.hits}/{self.n}')

    @property
    def value(self):
        return None if self.n == 0 else self.hits / self.n

    @classmethod
    def of(cls, flags):
        """From an iterable of booleans; `None` entries (not applicable) are skipped."""
        flags = [bool(flag) for flag in flags if flag is not None]
        return cls(sum(flags), len(flags))


def mean_or_none(values):
    values = [value for value in values if value is not None]
    return None if not values else math.fsum(values) / len(values)


# --- Multi-hop chains -----------------------------------------------------------------------

def full_chain_recovered_at(chain, retrieved, k):
    """Every ref of the evidence chain is among the first k retrieved."""
    top = set(tuple(retrieved)[:k])
    return bool(chain) and all(ref in top for ref in chain)


def full_chain_recovery_at_k(cases, k):
    """Rate over `(chain_refs, retrieved)` pairs."""
    return Rate.of(full_chain_recovered_at(chain, retrieved, k) for chain, retrieved in cases)


# --- Paths ----------------------------------------------------------------------------------

def _accepted_edges(path_gold):
    edges = set(path_gold.accept_edges)
    edges.update(zip(path_gold.steps, path_gold.steps[1:]))
    if not path_gold.directed:
        edges.update((b, a) for a, b in list(edges))
    return edges


def path_target(path_gold):
    """Where an accepted route must end: `to`, else the reference route's end, else anywhere."""
    if path_gold.end is not None:
        return path_gold.end
    return path_gold.steps[-1] if path_gold.steps else None


def path_found(path_gold, route):
    """Whether a returned route (ordered refs, start first) is one a reader accepts.

    - it runs from `from` to the target (`path_target`), with at least one hop;
    - it passes through every `required` ref;
    - without `accept_alternatives` it must be the reference route exactly;
    - with them, every hop must be accepted (reference hops or `accept_edges`), so a
      route that jumps through unrelated entries is not a found path.
    """
    route = tuple(route)
    if len(route) < 2 or route[0] != path_gold.start:
        return False
    target = path_target(path_gold)
    if target is not None and route[-1] != target:
        return False
    if not set(path_gold.required) <= set(route):
        return False
    if not path_gold.accept_alternatives:
        return bool(path_gold.steps) and route == tuple(path_gold.steps)
    edges = _accepted_edges(path_gold)
    return all(hop in edges for hop in zip(route, route[1:]))


def step_precision(path_gold, route):
    """Share of the route's hops a reader accepts; None for a route without hops."""
    route = tuple(route)
    hops = list(zip(route, route[1:]))
    if not hops:
        return None
    edges = _accepted_edges(path_gold)
    return sum(1 for hop in hops if hop in edges) / len(hops)


# --- Temporal -------------------------------------------------------------------------------

def serves_stale(stale_refs, cited):
    """The core cites something a successor superseded."""
    return bool(set(stale_refs) & set(cited))


def stale_serve_rate(cases):
    """Rate over `(stale_refs, cited)` pairs of supersession questions."""
    return Rate.of(serves_stale(stale, cited) for stale, cited in cases)


def leaks_future(future_refs, cited):
    """The core cites a ref recorded after the question's `as_of` / outside its interval.

    Gold does not date refs; the caller passes the refs the world (or labeler) knows to be
    later than the selection.
    """
    return bool(set(future_refs) & set(cited))


def future_leak_rate(cases):
    """Rate over `(future_refs, cited)` pairs."""
    return Rate.of(leaks_future(future, cited) for future, cited in cases)


def as_of_correct(answer_refs, future_refs, cited):
    """An as_of / interval answer is right when it cites an answer and nothing from the future."""
    cited = set(cited)
    return bool(cited & set(answer_refs)) and not leaks_future(future_refs, cited)


def as_of_accuracy(cases):
    """Rate over `(answer_refs, future_refs, cited)` triples."""
    return Rate.of(as_of_correct(answers, future, cited) for answers, future, cited in cases)


# --- Decision -------------------------------------------------------------------------------

def distractor_in_core(distractor_refs, cited):
    return bool(set(distractor_refs) & set(cited))


def distractor_in_core_rate(cases):
    """Rate over `(distractor_refs, cited)` pairs (hard_distractor, wrong_subject)."""
    return Rate.of(distractor_in_core(distractors, cited) for distractors, cited in cases)


@dataclass(frozen=True)
class FalseUnknown:
    """The two definitions BENCH_SPEC §7 asks for, side by side.

    - `scorecard`: the Rust column, UNKNOWN on a question with judged answers over every
      question scored (negatives included in the denominator);
    - `given_known`: P(UNKNOWN | KNOWN), over the answerable questions only.
    """
    scorecard: Rate
    given_known: Rate


def false_unknown_rate(cases):
    """`cases`: `(known, unknown)` boolean pairs, one per question."""
    cases = [(bool(known), bool(unknown)) for known, unknown in cases]
    return FalseUnknown(
        scorecard=Rate(sum(1 for known, unknown in cases if known and unknown), len(cases)),
        given_known=Rate(sum(1 for known, unknown in cases if known and unknown),
                         sum(1 for known, _ in cases if known)))


def false_answer_rate_by_type(cases):
    """`cases`: `(question_type, false_answer)` over negative questions; one Rate per type."""
    grouped = {}
    for kind, flag in cases:
        grouped.setdefault(kind, []).append(flag)
    return {kind: Rate.of(flags) for kind, flags in sorted(grouped.items())}


def facet_coverage(facets, cited):
    """Answerable facets with at least one answer cited, over answerable facets asked.

    `facets` are gold `Facet`s. None when no asked facet is answerable.
    """
    cited = set(cited)
    answerable = [facet for facet in facets if facet.answerable_facet]
    if not answerable:
        return None
    covered = sum(1 for facet in answerable if cited & set(facet.answers))
    return covered / len(answerable)


def useful_rate(useful_flags):
    """ANSWER correct + PARTIAL useful over positive questions (see scoring_rules)."""
    return Rate.of(useful_flags)


CONFIDENCE_ORDER = {'high': 3, 'medium': 2, 'low': 1, 'unknown': 0}


def precision_by_confidence(cases):
    """`cases`: `(confidence, correct)`; one Rate per confidence level seen."""
    grouped = {}
    for level, correct in cases:
        grouped.setdefault(level, []).append(correct)
    return {level: Rate.of(flags) for level, flags in sorted(grouped.items())}


@dataclass(frozen=True)
class RiskCoverage:
    points: tuple  # ((coverage, risk), ...) at each confidence threshold, highest first
    aurc: float | None


def aurc(cases, order=CONFIDENCE_ORDER):
    """Area under the risk-coverage curve, answered questions ranked by confidence.

    `cases`: `(confidence, correct)` for every answered (non-abstained) question.
    Confidence is a level (`order` maps it to a rank) or a number. Ties are resolved as
    groups: accepting a level accepts every question at it, so the curve has one point
    per distinct level, `(coverage_j, risk_j)` with risk = errors / accepted so far, and
    AURC = sum_j (n_j / n) * risk_j (the step integral; with no ties it is the usual
    (1/n) * sum_i risk(top i)). Lower is better. None with no cases.
    """
    cases = list(cases)
    if not cases:
        return RiskCoverage((), None)

    def rank(confidence):
        return order[confidence] if isinstance(confidence, str) else float(confidence)

    groups = {}
    for confidence, correct in cases:
        groups.setdefault(rank(confidence), []).append(bool(correct))
    n = len(cases)
    accepted = errors = 0
    points = []
    area = 0.0
    for key in sorted(groups, reverse=True):
        group = groups[key]
        accepted += len(group)
        errors += sum(1 for correct in group if not correct)
        risk = errors / accepted
        points.append((accepted / n, risk))
        area += len(group) / n * risk
    return RiskCoverage(tuple(points), area)
