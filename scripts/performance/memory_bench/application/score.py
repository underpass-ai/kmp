"""Score every question of a run under the pre-registered rules (BENCH_SPEC sections 6-7).

One `QuestionScore` per (question, sample), read from repeat 0 (later repeats
are the repeat/determinism control). The answer is read the way the Rust
scorecards read it (`domain/refs.py`), judged by `domain/scoring_rules.py`,
and measured with `domain/metrics.py`; this module only wires them to a run.

`values` holds per-question metrics: a bool is one Bernoulli trial of a rate, a
float is one observation of a mean, and a metric that does not apply to the
question is simply absent (never False, never 0). `counts` holds (hits, n) for
rates pooled over items inside a question (proposed hops, undeclared edges).

Generator provenance is used where gold alone cannot say it:
- `source.future_refs` (comma-separated refs recorded after the question's
  `as_of` / interval) feeds `future_leak` and `as_of_correct`;
- `source.undeclared_edges` (`a>b` hops the world never declared as relations)
  splits `path_found` into routes reachable on declared relations and routes
  that need a proposed hop, and feeds `undeclared_edge_recovery`.
"""
from dataclasses import dataclass, field

from ..domain import metrics, refs, scoring_rules, unknown_reasons
from ..domain.errors import BenchError
from .tokens import PRIMARY_ENCODING

SCORED_STATUSES = ('completed', 'call_cap_reached')


class ScoringRefused(BenchError):
    code = 'SCORING_REFUSED'


@dataclass(frozen=True)
class QuestionScore:
    question_id: str
    type: str
    corpus: str
    tool: str
    sample: int
    private: bool
    tags: tuple
    status: str  # journey status (token_harness JourneyOutcome names)
    pages: int
    response_bytes: int | None
    outcome: str | None = None  # scoring_rules outcome label (kmp_ask)
    decision: str | None = None
    confidence: str | None = None
    native_reason: str | None = None
    reason: str | None = None  # bench vocabulary, None when unmapped
    values: dict = field(default_factory=dict)
    counts: dict = field(default_factory=dict)
    tokens: dict = field(default_factory=dict)  # encoding -> {journey, first_page, ...}
    retrieval: object = None  # metrics.RetrievalOutcome (kmp_ask), for the Rust scorecard port

    @property
    def key(self):
        return (self.question_id, self.sample)

    @property
    def error(self):
        return self.status not in SCORED_STATUSES

    def as_row(self):
        """Public per-question row of a report (never emitted for private corpora)."""
        return {'question_id': self.question_id, 'sample': self.sample, 'status': self.status,
                'outcome': self.outcome, 'decision': self.decision, 'confidence': self.confidence,
                'native_reason': self.native_reason, 'reason': self.reason, 'pages': self.pages,
                'values': {k: self.values[k] for k in sorted(self.values)},
                'counts': {k: list(self.counts[k]) for k in sorted(self.counts)},
                'tokens': self.tokens.get(PRIMARY_ENCODING)}


def csv_refs(value):
    """A comma-separated ref list from `source` (empty string: none)."""
    if not isinstance(value, str) or not value:
        return ()
    return tuple(item for item in value.split(',') if item)


def undeclared_edges(source):
    return tuple(tuple(edge.split('>', 1)) for edge in csv_refs((source or {}).get('undeclared_edges'))
                 if '>' in edge)


def _strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from _strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from _strings(item)


def mentioned_refs(structured):
    """Every string of a page, normalized: what a reader could follow as a ref."""
    return {refs.normalize(text) for text in _strings(structured)}


# --- kmp_ask ------------------------------------------------------------------------------

def _first_judged_call(pages, judged):
    for index, page in enumerate(pages):
        if judged & set(refs.evidence_refs(page)):
            return index + 1
    return None


def _ask_values(question, pages, observed, scored, retrieved):
    gold, cited = question.gold, set(observed.cited)
    judged = gold.answer_refs()
    unknown = observed.decision == scoring_rules.UNKNOWN
    values = {'unknown': unknown}
    if gold.answerable == 'KNOWN':
        values.update(useful=scored.useful, answer_correct=scored.outcome == scoring_rules.ANSWER_CORRECT,
                      partial_useful=scored.outcome == scoring_rules.PARTIAL_USEFUL,
                      false_unknown=scored.false_unknown)
        if scored.facet_coverage is not None:
            values['facet_coverage'] = scored.facet_coverage
    else:
        values.update(false_answer=scored.false_answer,
                      abstain_correct=scored.outcome == scoring_rules.ABSTAIN_CORRECT)
        if unknown and gold.unknown_reason is not None:
            values['reason_mapped'] = observed.reason is not None
            if observed.reason is not None:
                values['reason_correct'] = observed.reason == gold.unknown_reason
    first = pages[0]
    budget = ((first.get('projection') or {}).get('budget') or {}) if isinstance(first, dict) else {}
    used = budget.get('used_bytes') if isinstance(budget.get('used_bytes'), int) else 0
    retrieval = metrics.RetrievalOutcome.of(judged, retrieved, cited, unknown, used)
    if judged:
        values.update(recall_at_1=retrieval.recall_at(1), recall_at_5=retrieval.recall_at(5),
                      recall_at_10=retrieval.recall_at(10),
                      mean_reciprocal_rank=retrieval.reciprocal_rank(),
                      ndcg_at_10=retrieval.ndcg_at(10))
        if not unknown:
            values['core_precision'] = retrieval.answer_cites_judged()
    if not unknown and observed.confidence is not None:
        values['confidence_correct'] = scoring_rules.confidence_correct(scored)
    if gold.wrong_subject:
        values['distractor_in_core'] = metrics.distractor_in_core(gold.wrong_subject, cited)
    if gold.excluded_refs:
        values['excluded_cited'] = bool(cited & set(gold.excluded_refs))
    if gold.chain is not None:
        values['full_chain_recovered_at_5'] = metrics.full_chain_recovered_at(gold.chain.refs, retrieved, 5)
        values['full_chain_recovered_at_10'] = metrics.full_chain_recovered_at(gold.chain.refs, retrieved, 10)
    if gold.stale_refs:
        values['stale_served'] = metrics.serves_stale(gold.stale_refs, cited)
    if gold.current_ref is not None:
        values['successor_rescued'] = gold.current_ref in cited
    source = question.source or {}
    if 'future_refs' in source and gold.answerable == 'KNOWN':
        future = csv_refs(source['future_refs'])
        values['future_leak'] = metrics.leaks_future(future, cited)
        values['as_of_correct'] = metrics.as_of_correct(judged, future, cited)
    if gold.nearest_outside is not None:
        named = refs.nearest_outside_ref(first)
        values['nearest_outside_correct'] = (named is not None
                                             and refs.normalize(named) == gold.nearest_outside)
    return values, retrieval


def _score_ask(question, pages):
    native, reason = unknown_reasons.translate(pages[0])
    observed = scoring_rules.Observed.from_structured(pages[0], reason)
    scored = scoring_rules.score(question.gold, observed)
    retrieved = [ref for page in pages for ref in refs.evidence_refs(page)]
    values, retrieval = _ask_values(question, pages, observed, scored, retrieved)
    fields = {'outcome': scored.outcome, 'decision': observed.decision,
              'confidence': observed.confidence, 'native_reason': native, 'reason': reason,
              'retrieval': retrieval}
    return values, {}, fields, _first_judged_call(pages, question.gold.answer_refs())


# --- paths --------------------------------------------------------------------------------

def _hop(item, key):
    value = item.get(key) if isinstance(item, dict) else None
    if isinstance(value, dict):
        value = value.get('ref')
    return refs.normalize(value) if isinstance(value, str) else None


def read_routes(tool, structured):
    """[(route refs, hops)] of a paths answer; hops are (from, to, declared)."""
    routes = []
    if tool == 'kmp_trace':
        items = structured.get('trace') if isinstance(structured.get('trace'), list) else []
        hops = [(_hop(item, 'from'), _hop(item, 'to'), True) for item in items]
        groups = [hops] if hops else []
    else:
        groups = []
        for path in structured.get('paths') or []:
            items = path.get('hops') if isinstance(path, dict) else None
            groups.append([(_hop(h, 'from'), _hop(h, 'to'), isinstance(h, dict) and h.get('declared') is True)
                           for h in items or []])
    for hops in groups:
        hops = [hop for hop in hops if hop[0] is not None and hop[1] is not None]
        if hops:
            routes.append(([hops[0][0]] + [hop[1] for hop in hops], hops))
    return routes


def _gold_edges(path_gold):
    return set(zip(path_gold.steps, path_gold.steps[1:])) | set(path_gold.accept_edges)


def direction_correct(path_gold, route):
    """Every hop that matches a gold edge runs the gold way round; None when none matches."""
    edges = _gold_edges(path_gold)
    matched = [(a, b) for a, b in zip(route, route[1:]) if (a, b) in edges or (b, a) in edges]
    return None if not matched else all(hop in edges for hop in matched)


def _score_path(question, pages):
    gold = question.gold.path
    routes = next((found for found in (read_routes(question.tool, page) for page in pages) if found), [])
    route = routes[0][0] if routes else ()
    undeclared = undeclared_edges(question.source)
    found = metrics.path_found(gold, route)
    values = {'path_found': found}
    values['path_found_needs_proposal' if undeclared else 'path_found_declared_reachable'] = found
    precision = metrics.step_precision(gold, route)
    if precision is not None:
        values['step_precision'] = precision
    direction = direction_correct(gold, route)
    if direction is not None:
        values['direction_correct'] = direction
    accepted = _gold_edges(gold) | {(b, a) for a, b in _gold_edges(gold)}
    proposed = [(a, b) for _, hops in routes for a, b, declared in hops if not declared]
    counts = {'proposed_hops_right': (sum(1 for hop in proposed if hop in accepted), len(proposed))}
    if undeclared:
        returned = {(a, b) for _, hops in routes for a, b, _ in hops}
        returned |= {(b, a) for a, b in returned}
        counts['undeclared_edge_recovery'] = (sum(1 for edge in undeclared if edge in returned),
                                              len(undeclared))
    return values, counts, {}, None


# --- kmp_wake -----------------------------------------------------------------------------

def _score_wake(question, pages):
    required = set(question.gold.wake_required)
    values, seen, ready = {}, set(), None
    for index, page in enumerate(pages):
        seen |= mentioned_refs(page) & required
        if ready is None and required and seen == required:
            ready = index + 1
    if required:
        values['wake_obligations_met'] = ready is not None
        values['wake_obligation_coverage'] = len(seen) / len(required)
        if ready is not None:
            values['task_ready_after_rpc'] = float(ready)
    return values, {}, {}, ready


SCORERS = {'kmp_ask': _score_ask, 'kmp_wake': _score_wake, 'kmp_trace': _score_path,
           'kmp_curate': _score_path}


def _tokens(run_tokens, journey, first_judged, ready):
    rows = {}
    for encoding in run_tokens.encodings:
        counted = run_tokens.of(journey, encoding)
        if counted is None:
            continue
        fixed = run_tokens.of_normalized(journey, encoding)
        rows[encoding] = {'journey': counted.total, 'first_page': counted.through(1),
                          'to_first_evidence': counted.through(first_judged) if first_judged else None,
                          'to_task_ready': counted.through(ready) if ready else None,
                          'journey_normalized': None if fixed is None else fixed.total,
                          'first_page_normalized': None if fixed is None else fixed.through(1)}
    return rows


def score_journey(run, record, question, run_tokens):
    calls = run.calls_of(record.journey)
    pages = [page for page in (run.structured(call) for call in calls) if page is not None]
    sizes = [call.response_bytes for call in calls]
    base = dict(question_id=question.id, type=question.type, corpus=question.corpus,
                tool=question.tool, sample=record.sample, private=question.private or run.private,
                tags=question.tags, status=record.status, pages=len(calls),
                response_bytes=None if None in sizes or not sizes else sum(sizes))
    if record.status not in SCORED_STATUSES or not pages:
        status = record.status if record.status not in SCORED_STATUSES else 'no_structured_answer'
        return QuestionScore(**{**base, 'status': status})
    if question.tool == 'kmp_ask' or question.gold.path is not None or question.tool == 'kmp_wake':
        values, counts, fields, marker = SCORERS[question.tool](question, pages)
    else:
        values, counts, fields, marker = {}, {}, {}, None
    values['censored'] = record.censored
    ready = marker if question.tool == 'kmp_wake' else None
    first_judged = marker if question.tool == 'kmp_ask' else None
    return QuestionScore(**base, **fields, values=values, counts=counts,
                         tokens=_tokens(run_tokens, record.journey, first_judged, ready))


def score_run(run, questions, run_tokens):
    """QuestionScore per (question, sample) of repeat 0, in question-file order."""
    by_id = {question.id: question for question in questions}
    order = {question.id: index for index, question in enumerate(questions)}
    rows = []
    for record in run.journeys:
        if record.repeat != 0:
            continue
        question = by_id.get(record.question_id)
        if question is None:
            raise ScoringRefused(f'{run.run_id}: journey of unknown question {record.question_id}')
        rows.append(score_journey(run, record, question, run_tokens))
    return tuple(sorted(rows, key=lambda row: (order[row.question_id], row.sample)))
