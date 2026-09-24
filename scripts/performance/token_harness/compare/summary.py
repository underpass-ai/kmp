"""Cohort figures: weighted reduction, median delta, worst regression, denominators."""
from collections import Counter


def quantile(values, p):
    """Linear (type 7) quantile: position (n-1)*p, interpolated."""
    ordered = sorted(values)
    if not ordered:
        return None
    position = (len(ordered) - 1) * p
    low = int(position)
    high = min(low + 1, len(ordered) - 1)
    return ordered[low] + (ordered[high] - ordered[low]) * (position - low)


def cohort(rows):
    measured = [r for r in rows if r['delta_tokens'] is not None]
    base = sum(r['baseline_tokens'] for r in measured)
    cand = sum(r['candidate_tokens'] for r in measured)
    worst = max(measured, key=lambda r: r['delta_tokens'], default=None)
    return {'n': len(measured), 'excluded_without_both_counts': len(rows) - len(measured),
            'baseline_tokens': base, 'candidate_tokens': cand,
            'weighted_reduction': (1 - cand / base) if base else None,
            'median_delta_tokens': quantile([r['delta_tokens'] for r in measured], 0.5),
            'worst_regression': ({'journey': worst['journey'], 'delta_tokens': worst['delta_tokens']}
                                 if worst else None),
            'conclusions': dict(sorted(Counter(r['conclusion'] for r in rows).items()))}


def cohorts(rows):
    groups = {}
    for row in rows:
        budget = 'write' if row['budget'] is None else f'b{row["budget"]}'
        for key in ((row['encoding'], row['representation_id'], 'all'),
                    (row['encoding'], row['representation_id'], budget)):
            groups.setdefault(key, []).append(row)
    return [{'encoding': e, 'representation_id': r, 'budget_group': b, **cohort(group)}
            for (e, r, b), group in sorted(groups.items())]
