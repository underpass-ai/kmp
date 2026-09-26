"""Agreement between two labelers: Cohen's κ per facet (BENCH_SPEC section 4.1, "Acuerdo").

The unit of the headline κ is one judgment of one facet: (question, facet,
entry) with the category `answers`, `related` or `none`, over the question's
pool plus every ref either labeler found by searching. Facets are aligned by
name; a facet only one labeler listed counts as `none` for every entry on the
other side, so disagreeing about which facets were asked lowers κ instead of
disappearing. Secondary κ: `answerable_facet` per (question, facet), `shape`
and `answerable` per question, `wrong_subject` per (question, entry); plus the
distribution of κ computed facet by facet.

The gate needs at least `min_questions` doubly labeled questions and a facet κ
of at least `threshold` (0.6); below the n it says `insufficient_n`, never
`pass`. The report holds aggregates only — no question, ref or text — and the
ids of the questions that need adjudication go to a separate private file.
"""
from collections import Counter
import statistics

AGREEMENT_SCHEMA = 'kmp.bench.breal.agreement.v1'
FACET_CATEGORIES = ('answers', 'related', 'none')


def cohen_kappa(pairs):
    """κ over (a, b) category pairs; None when undefined (no pairs, or chance agreement is 1 and
    observed is not). Two labelers who always use the same single category agree fully: κ = 1."""
    pairs = list(pairs)
    if not pairs:
        return None
    n = len(pairs)
    observed = sum(1 for a, b in pairs if a == b) / n
    left, right = Counter(a for a, _ in pairs), Counter(b for _, b in pairs)
    expected = sum(left[c] * right[c] for c in set(left) | set(right)) / (n * n)
    if expected == 1:
        return 1.0 if observed == 1 else None
    return (observed - expected) / (1 - expected)


def _facet_marks(gold):
    marks = {}
    for facet in gold.facets:
        entry = {ref: 'answers' for ref in facet.answers}
        entry.update({ref: 'related' for ref in facet.related})
        marks[facet.name] = (entry, facet.answerable_facet)
    return marks


def _named(gold):
    refs = set(gold.wrong_subject)
    for facet in gold.facets:
        refs |= set(facet.answers) | set(facet.related)
    return refs


def question_units(gold_a, gold_b, pool_refs):
    """Every paired judgment of one doubly labeled question, by kind."""
    universe = sorted(set(pool_refs) | _named(gold_a) | _named(gold_b))
    marks_a, marks_b = _facet_marks(gold_a), _facet_marks(gold_b)
    units = {'facet_entry': [], 'answerable_facet': [], 'wrong_subject': [], 'per_facet': [],
             'shape': [(gold_a.shape, gold_b.shape)], 'answerable': [(gold_a.answerable, gold_b.answerable)]}
    for name in sorted(set(marks_a) | set(marks_b)):
        entries_a, ok_a = marks_a.get(name, ({}, 'absent'))
        entries_b, ok_b = marks_b.get(name, ({}, 'absent'))
        pairs = [(entries_a.get(ref, 'none'), entries_b.get(ref, 'none')) for ref in universe]
        units['facet_entry'].extend(pairs)
        units['per_facet'].append(cohen_kappa(pairs))
        units['answerable_facet'].append((ok_a, ok_b))
    wrong_a, wrong_b = set(gold_a.wrong_subject), set(gold_b.wrong_subject)
    units['wrong_subject'] = [(ref in wrong_a, ref in wrong_b) for ref in universe]
    return units


def _round(value):
    return None if value is None else round(value, 4)


def agreement_report(paired, labelers, threshold=0.6, min_questions=30, rules_sha256=None):
    """paired: [(question_id, gold_a, gold_b, pool_refs)]. Returns (report, disagreeing ids)."""
    totals = {kind: [] for kind in ('facet_entry', 'answerable_facet', 'wrong_subject', 'shape', 'answerable')}
    per_facet, disagreeing = [], []
    for question_id, gold_a, gold_b, pool_refs in paired:
        units = question_units(gold_a, gold_b, pool_refs)
        for kind in totals:
            totals[kind].extend(units[kind])
        per_facet.extend(units['per_facet'])
        if any(a != b for kind in totals for a, b in units[kind]):
            disagreeing.append(question_id)
    kappas = {kind: _round(cohen_kappa(pairs)) for kind, pairs in totals.items()}
    defined = [k for k in per_facet if k is not None]
    n = len(paired)
    facet_kappa = kappas['facet_entry']
    if n < min_questions:
        status = 'insufficient_n'
    elif facet_kappa is not None and facet_kappa >= threshold:
        status = 'pass'
    else:
        status = 'fail'
    report = {'schema': AGREEMENT_SCHEMA, 'labelers': list(labelers), 'rules_sha256': rules_sha256,
              'double_labeled_questions': n,
              'units': {kind: len(pairs) for kind, pairs in totals.items()},
              'kappa': kappas,
              'per_facet': {'facets': len(per_facet), 'defined': len(defined),
                            'median': _round(statistics.median(defined)) if defined else None,
                            'min': _round(min(defined)) if defined else None,
                            'at_or_above_threshold': sum(1 for k in defined if k >= threshold)},
              'gate': {'metric': 'kappa.facet_entry', 'threshold': threshold, 'min_questions': min_questions,
                       'status': status},
              'questions_needing_adjudication': len(disagreeing)}
    return report, sorted(disagreeing)
