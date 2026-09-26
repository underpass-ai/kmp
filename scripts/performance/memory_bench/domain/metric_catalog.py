"""Names, kinds and directions of the metrics a report carries (BENCH_SPEC section 7).

A report metric is computed from one per-question value of `application/score.py`
(`source`) as a rate (a bool per question: Wilson interval, McNemar when paired),
a mean (a float per question: bootstrap interval when paired) or a pooled rate
(`(hits, n)` per question, summed). `higher_is_better` tells a guard which way is
worse and a target which way is an improvement. Variant targets and guards name
metrics by their report name; an unknown name fails loudly instead of scoring
nothing.
"""
from dataclasses import dataclass

RATE = 'rate'
MEAN = 'mean'
POOLED = 'pooled'


@dataclass(frozen=True)
class MetricSpec:
    name: str  # report name, what variants target
    source: str  # key in QuestionScore.values / counts / tokens
    kind: str
    higher_is_better: bool
    description: str


def _rate(name, source, higher, description):
    return MetricSpec(name, source, RATE, higher, description)


def _mean(name, source, higher, description):
    return MetricSpec(name, source, MEAN, higher, description)


CATALOG = (
    _rate('useful_rate', 'useful', True, 'ANSWER correct + PARTIAL useful over positives'),
    _rate('answer_correct_rate', 'answer_correct', True, 'ANSWER correct over positives'),
    _rate('partial_useful_rate', 'partial_useful', True, 'PARTIAL useful over positives'),
    _rate('false_unknown_rate', 'false_unknown', False, 'P(UNKNOWN | KNOWN)'),
    _rate('false_answer_rate', 'false_answer', False, 'ANSWER or naming-nothing PARTIAL over negatives'),
    _rate('abstention_rate', 'abstain_correct', True, 'correct abstention over negatives'),
    _rate('reason_accuracy', 'reason_correct', True,
          'UNKNOWN reason equals the labelled cause (mapped reasons only)'),
    _rate('reason_mapped_rate', 'reason_mapped', True,
          'UNKNOWNs with an expected cause whose binary reason maps to bench vocabulary'),
    _mean('facet_coverage', 'facet_coverage', True, 'answerable facets cited over asked'),
    _rate('core_precision', 'core_precision', True, 'answered positives whose core cites an answer'),
    _mean('recall_at_1', 'recall_at_1', True, 'R@1 over positives (retrieval_scorecard.rs)'),
    _mean('recall_at_5', 'recall_at_5', True, 'R@5 over positives'),
    _mean('recall_at_10', 'recall_at_10', True, 'R@10 over positives'),
    _mean('mean_reciprocal_rank', 'mean_reciprocal_rank', True, 'MRR over positives'),
    _mean('ndcg_at_10', 'ndcg_at_10', True, 'nDCG@10 over positives'),
    _rate('distractor_in_core_rate', 'distractor_in_core', False, 'core cites a wrong-subject entry'),
    _rate('excluded_cited_rate', 'excluded_cited', False, 'negated_anchor: core cites an excluded entry'),
    _rate('full_chain_recovery_at_5', 'full_chain_recovered_at_5', True, 'whole chain in top 5'),
    _rate('full_chain_recovery_at_10', 'full_chain_recovered_at_10', True, 'whole chain in top 10'),
    _rate('stale_serve_rate', 'stale_served', False, 'core cites a superseded entry'),
    _rate('successor_rescue_rate', 'successor_rescued', True, 'core cites the current successor'),
    _rate('future_leak_rate', 'future_leak', False, 'core cites an entry after as_of/interval'),
    _rate('as_of_accuracy', 'as_of_correct', True, 'cites an answer and nothing from the future'),
    _rate('nearest_outside_correct', 'nearest_outside_correct', True,
          'interval UNKNOWN names the labelled nearest match outside'),
    _rate('path_found', 'path_found', True, 'returned route accepted by the reader'),
    _rate('path_found_declared_reachable', 'path_found_declared_reachable', True,
          'path_found where every gold hop is a declared relation'),
    _rate('path_found_needs_proposal', 'path_found_needs_proposal', True,
          'path_found where a gold hop is undeclared (source.undeclared_edges)'),
    _mean('step_precision', 'step_precision', True, 'accepted hops over route hops'),
    _rate('direction_correct', 'direction_correct', True, 'gold hops of the route run the gold way'),
    MetricSpec('proposed_hops_right', 'proposed_hops_right', POOLED, True,
               'undeclared hops returned that a reader accepts, pooled over hops'),
    MetricSpec('undeclared_edge_recovery', 'undeclared_edge_recovery', POOLED, True,
               'undeclared gold edges returned as hops, pooled over edges'),
    _rate('wake_obligations_met', 'wake_obligations_met', True, 'every wake_required ref seen'),
    _mean('wake_obligation_coverage', 'wake_obligation_coverage', True, 'wake_required refs seen'),
    _mean('task_ready_after_rpc', 'task_ready_after_rpc', False, 'pages until obligations are met'),
    _rate('censored_rate', 'censored', False, 'journeys stopped by max_calls or a timeout'),
    _rate('unknown_rate', 'unknown', False, 'UNKNOWN over kmp_ask questions'),
)
BY_NAME = {spec.name: spec for spec in CATALOG}

# Per-question numbers read from QuestionScore itself rather than `values`.
TOKEN_METRICS = {
    'tokens_journey': 'journey', 'tokens_first_page': 'first_page',
    'tokens_to_first_evidence': 'to_first_evidence', 'tokens_to_task_ready': 'to_task_ready',
    # The same journey with every continuation handle replaced by a fixed-length placeholder
    # (application/tokens.py): what the A/A control holds to Δ = 0.
    'tokens_journey_normalized': 'journey_normalized',
    'tokens_first_page_normalized': 'first_page_normalized'}
LOWER_IS_BETTER_EXTRA = ('pages', 'response_bytes', 'tokens_per_useful', 'aurc', 'jev_usd') + tuple(TOKEN_METRICS)


def spec(name):
    if name in BY_NAME:
        return BY_NAME[name]
    if name in TOKEN_METRICS or name in ('pages', 'response_bytes'):
        return MetricSpec(name, name, MEAN, False, 'agent load per journey')
    raise KeyError(f'unknown metric {name!r}')


def higher_is_better(name):
    if name in BY_NAME:
        return BY_NAME[name].higher_is_better
    if name in LOWER_IS_BETTER_EXTRA:
        return False
    raise KeyError(f'unknown metric {name!r}')
