"""Regenerate crates/kmp-testkit/judged/metric_parity.json from the Python port.

The fixture pins `domain/metrics.py` and `crates/kmp-testkit/src/retrieval_scorecard.rs`
to the same numbers (`cargo test -p kmp-testkit --test metric_parity` and
`tests/test_metrics.py` both read it). Run it from anywhere:

    python3 scripts/performance/memory_bench/tools/gen_metric_parity.py [--out PATH]

and check that the Rust test still passes before committing a changed fixture.
"""
import argparse
import json
from pathlib import Path
import sys

REPO_ROOT = Path(__file__).resolve().parents[4]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from scripts.performance.memory_bench.domain.metrics import (  # noqa: E402
    RetrievalOutcome, RetrievalScorecard)

DEFAULT_OUT = REPO_ROOT / 'crates' / 'kmp-testkit' / 'judged' / 'metric_parity.json'
ABOUT = ('Shared fixture: the same ranking outcomes scored by crates/kmp-testkit/src/retrieval_scorecard.rs '
         'and scripts/performance/memory_bench/domain/metrics.py must give these numbers. Floats compare '
         'within `tolerance`.')
TWELVE = [f'j{i:02d}' for i in range(12)]
SEVEN = [f'j{i:02d}' for i in range(7)]

# (name, judged, retrieved, cited, unknown, used_bytes, elapsed_millis)
CASES = (
    ('first_rank_hit', ['a'], ['a', 'x'], ['a'], False, 4096, 12),
    ('third_rank_hit', ['a'], ['x', 'y', 'a'], [], False, 2048, 30),
    ('absent_from_results', ['a'], ['x', 'y'], [], False, 1000, 5),
    ('half_of_two_judged', ['a', 'b'], ['a', 'x'], ['b'], False, 300, 7),
    ('duplicate_hit_earns_twice_in_ndcg', ['a'], ['a', 'a', 'x'], ['a'], False, 0, 0),
    ('duplicate_counts_once_in_recall', ['a', 'b'], ['a', 'a'], [], False, 0, 0),
    ('honest_unknown_without_judged', [], [], [], True, 120, 3),
    ('false_unknown_with_judged', ['a'], [], [], True, 150, 4),
    ('cited_but_not_judged', ['a'], ['a'], ['z'], False, 10, 1),
    ('hits_beyond_cutoffs', ['k', 'l', 'm'], [f'x{i}' for i in range(9)] + ['k', 'l', 'm'], ['m'],
     False, 9999, 250),
    ('three_judged_all_in_top_five', ['c', 'a', 'b'], ['a', 'q', 'b', 'c', 'r'], ['a', 'c'], False, 512, 2),
    ('colon_refs_as_the_scorecard_sees_them', ['project:x:e2', 'project:x:e1'],
     ['project:x:e2', 'project:x:e9', 'project:x:e1'], ['project:x:e1'], False, 4011, 46),
    # More judged refs than any cutoff: recall@k tops out at k / |judged| and the nDCG
    # ideal is truncated at k, so a perfect top ten scores 1.0.
    ('judged_beyond_every_cutoff', TWELVE, TWELVE, ['j00'], False, 64, 1),
    ('judged_beyond_five_found_late', SEVEN, ['x', 'j00', 'j01', 'y', 'j02', 'j03', 'j04', 'z', 'j05', 'j06'],
     [], False, 32, 1),
    # The judged set is a set on both sides: a repeated judged ref is one ref, so it
    # neither lowers recall nor raises the nDCG ideal.
    ('duplicate_judged_counts_once', ['a', 'a', 'b'], ['a', 'b'], ['a'], False, 16, 1),
    ('duplicate_judged_half_found', ['b', 'a', 'b'], ['x', 'b'], [], False, 8, 1),
)
SCORECARD_FIELDS = ('cases', 'recall_at_1', 'recall_at_5', 'recall_at_10', 'mean_reciprocal_rank',
                    'ndcg_at_10', 'answer_core_precision', 'false_unknown_rate', 'mean_used_bytes',
                    'mean_elapsed_millis')


def expected(outcome):
    return {'recall_at_1': outcome.recall_at(1), 'recall_at_5': outcome.recall_at(5),
            'recall_at_10': outcome.recall_at(10), 'reciprocal_rank': outcome.reciprocal_rank(),
            'ndcg_at_10': outcome.ndcg_at(10), 'answer_cites_judged': outcome.answer_cites_judged(),
            'is_false_unknown': outcome.is_false_unknown(),
            'has_complete_support_at_5': outcome.has_complete_support_at(5)}


def fixture():
    cases, outcomes = [], []
    for name, judged, retrieved, cited, unknown, used, elapsed in CASES:
        outcome = RetrievalOutcome.of(judged, retrieved, cited, unknown, used, elapsed)
        outcomes.append(outcome)
        cases.append({'name': name, 'judged': judged, 'retrieved': retrieved, 'cited': cited,
                      'unknown': unknown, 'used_bytes': used, 'elapsed_millis': elapsed,
                      'expected': expected(outcome)})
    card = RetrievalScorecard.score(outcomes)
    return {'schema': 'kmp.bench.metric_parity.v1', 'about': ABOUT, 'tolerance': 1e-12, 'cases': cases,
            'scorecard': {name: getattr(card, name) for name in SCORECARD_FIELDS}}


def render():
    return json.dumps(fixture(), indent=2) + '\n'


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--out', type=Path, default=DEFAULT_OUT)
    args = parser.parse_args(argv)
    args.out.write_text(render(), encoding='utf-8')
    return 0


if __name__ == '__main__':
    sys.exit(main())
