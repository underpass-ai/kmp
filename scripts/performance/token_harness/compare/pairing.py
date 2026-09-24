"""One row per journey x encoding x representation, with the annex A.14 columns."""
import json

from ..scenarios.model import journey_id
from .conclusion import Side, conclude

CAPTURE_FAILURES = ('transport_error', 'rpc_error')


def expected_journeys(*captures):
    """Every journey a capture declared by its scenarios, present or not."""
    from ..scenarios import BY_CASE
    names = []
    for capture in captures:
        for case_id in capture.get('scenarios', {}):
            scenario = BY_CASE.get(case_id)
            for budget in scenario.journey.budgets if scenario else ():
                name = journey_id(case_id, budget)
                if name not in names:
                    names.append(name)
    return names


def _tokens_until(measured, point, encoding, representation):
    """Reference tokens of every exposure up to and including the point's response."""
    if not measured or not point:
        return None
    index = {e['exposure_id']: e['event_index'] for e in measured['exposures']}
    rows = [m for m in measured['measurements'] if m['encoding'] == encoding
            and m['representation'] == representation
            and index[m['exposure_id']] <= point['event_index']]
    return sum(m['tokens'] for m in rows if m['tokens'] is not None)


def _stage_tokens(series, stage):
    kinds = (series or {}).get('by_stage', {}).get(stage, {})
    return sum(kind.get('tokens', 0) for kind in kinds.values()) if series else None


def _side(measured, judged, record, encoding, representation):
    series = (measured or {}).get('totals', {}).get(encoding, {}).get(representation)
    captured = bool(measured and measured.get('complete') and judged and series
                    and (record or {}).get('driver_status') not in CAPTURE_FAILURES)
    metrics = (judged or {}).get('metrics', {})
    return {
        'captured': captured,
        'tokens': series['all']['tokens'] if series else None,
        'memory_tokens': _stage_tokens(series, 'memory'),
        'startup_tokens': _stage_tokens(series, 'startup_catalogue'),
        'utf8_bytes': series['all']['utf8_bytes'] if series else None,
        'absent_units': series['all']['absent'] if series else None,
        'rpc': metrics.get('rpc_calls'),
        'first_supported_unit': (metrics.get('first_supported_unit') or {}).get('after_rpc'),
        'tokens_until_first_supported_unit': _tokens_until(
            measured, metrics.get('first_supported_unit'), encoding, representation),
        'tokens_until_task_ready': _tokens_until(measured, metrics.get('task_ready'),
                                                 encoding, representation),
        'quality_pass': judged.get('quality_pass') if judged else None,
        'quality_failures': judged.get('quality_failures') if judged else None,
    }


def _incompatibility(config_reason, base_judged, cand_judged):
    if config_reason:
        return config_reason
    if base_judged and cand_judged:
        if base_judged['goal_id'] != cand_judged['goal_id']:
            return 'different goals'
        if json.dumps(base_judged['selection'], sort_keys=True) != \
                json.dumps(cand_judged['selection'], sort_keys=True):
            return 'different initial selection (scope, detail, budget or time)'
    return None


def pair_rows(baseline, candidate, config_reason):
    base_m, cand_m = baseline.journey_metrics(), candidate.journey_metrics()
    base_o, cand_o = baseline.journey_oracle(), candidate.journey_oracle()
    base_r = {row['lesson']: row for row in baseline.verified.manifest.get('journeys', [])}
    cand_r = {row['lesson']: row for row in candidate.verified.manifest.get('journeys', [])}
    encodings = [t['encoding'] for t in baseline.metrics['tokenizers']]
    rows = []
    names = expected_journeys(baseline.capture, candidate.capture)
    names += [name for name in list(base_r) + list(cand_r) if name not in names]
    for name in dict.fromkeys(names):
        record = base_r.get(name) or cand_r.get(name) or {}
        judged = base_o.get(name) or cand_o.get(name) or {}
        for encoding in encodings:
            for representation in baseline.metrics['representations']:
                base = _side(base_m.get(name), base_o.get(name), base_r.get(name), encoding, representation)
                cand = _side(cand_m.get(name), cand_o.get(name), cand_r.get(name), encoding, representation)
                reason = _incompatibility(config_reason, base_o.get(name), cand_o.get(name))
                state, why = conclude(Side(base['captured'], base['tokens'], base['quality_pass']),
                                      Side(cand['captured'], cand['tokens'], cand['quality_pass']), reason)
                delta = None if None in (base['tokens'], cand['tokens']) else cand['tokens'] - base['tokens']
                rows.append({
                    'case_id': record.get('case_id') or judged.get('case_id'), 'journey': name,
                    'goal_id': judged.get('goal_id'), 'budget': record.get('budget'),
                    'representation_id': representation, 'encoding': encoding,
                    'baseline_tokens': base['tokens'], 'candidate_tokens': cand['tokens'],
                    'delta_tokens': delta,
                    'reduction_fraction': (None if delta is None or not base['tokens']
                                           else -delta / base['tokens']),
                    'baseline_rpc': base['rpc'], 'candidate_rpc': cand['rpc'],
                    'first_supported_unit': {'baseline': base['first_supported_unit'],
                                             'candidate': cand['first_supported_unit']},
                    'quality_baseline': base['quality_pass'], 'quality_candidate': cand['quality_pass'],
                    'comparable': reason is None, 'conclusion': state.value, 'conclusion_reason': why,
                    'baseline': base, 'candidate': cand})
    return rows
