"""report.json: every figure derived from verified captures, measurements and oracles."""
from collections import Counter

from ..application.provenance import harness_code_sha256
from .inputs import check_encoders, configuration_mismatch, load_side
from .pairing import pair_rows
from .summary import cohorts

SCHEMA = 'kmp.token.compare.v1'
LIMITATIONS = (
    'L1 native replay of synthetic fixtures; no agent, host context or billing was observed.',
    'Reference tokens are whole-unit counts under the declared encoder and representation.',
    'One deterministic capture per variant; ids and clocks differ between fresh stores.',
    'Scenarios run: W01-W04, A01, E01. T01, G03 and the 512-byte recovery budget of annex A.12 '
    'are not implemented yet.',
    'requested_scope_exhausted is reported, not required: an orientation may end with items '
    'excluded by detail.',
    'support_bookkeeping_in_state_count (supports lines in current_state) is descriptive; the '
    'verdict uses support_displacement_count, supports lines present while a required memory is '
    'missing from state (annex A.11.2).',
    'No latency, cache temperature, rehydration or host (H4/H5) measurement.',
)


def startup_catalogue(side):
    """Startup exchanges (initialize, initialized, tools/list) per method, encoding and representation.

    Every session pays them once. `distinct_totals` lists each different total seen across the
    run's complete journeys, so a catalogue that varied between sessions is visible, not averaged.
    """
    per_journey = {}
    for journey in side.metrics['journeys']:
        if not journey.get('complete'):
            continue
        stage = {e['exposure_id']: e for e in journey['exposures'] if e['stage'] == 'startup_catalogue'}
        for m in journey['measurements']:
            exposure = stage.get(m['exposure_id'])
            if exposure is None or m['tokens'] is None:
                continue
            key = (m['encoding'], m['representation'])
            methods = per_journey.setdefault(key, {}).setdefault(journey['journey'], {})
            slot = methods.setdefault(exposure['method'], {'tokens': 0, 'utf8_bytes': 0})
            slot['tokens'] += m['tokens']
            slot['utf8_bytes'] += m['utf8_bytes']
    result = []
    for (encoding, representation), journeys in sorted(per_journey.items()):
        first = journeys[sorted(journeys)[0]]
        totals = sorted({sum(v['tokens'] for v in methods.values()) for methods in journeys.values()})
        result.append({'encoding': encoding, 'representation_id': representation,
                       'journeys': len(journeys), 'methods': dict(sorted(first.items())),
                       'tokens': sum(v['tokens'] for v in first.values()),
                       'utf8_bytes': sum(v['utf8_bytes'] for v in first.values()),
                       'distinct_totals': totals})
    return result


def _side_header(side):
    capture = side.capture
    return {'run': side.verified.root.name, 'manifest_sha256': side.verified.manifest_sha256,
            'variant': capture.get('variant'), 'binary_sha256': capture.get('binary_sha256'),
            'build_provenance': capture.get('build_provenance'),
            'wake_contract': capture.get('wake_contract'), 'driver_version': capture.get('driver_version'),
            'scenarios_digest': capture.get('scenarios_digest'),
            'harness_code_sha256': capture.get('harness_code_sha256'),
            'capture_failures': capture.get('failures', []),
            'journeys': len(side.verified.manifest.get('journeys', [])),
            'startup': startup_catalogue(side),
            'incomplete_journeys': sorted(j['journey'] for j in side.metrics['journeys']
                                          if not j.get('complete'))}


def compare_runs(baseline_paths, candidate_paths, max_file_bytes, label):
    baseline = load_side('baseline', *baseline_paths, max_file_bytes)
    candidate = load_side('candidate', *candidate_paths, max_file_bytes)
    check_encoders(baseline, candidate)
    config = configuration_mismatch(baseline, candidate)
    rows = pair_rows(baseline, candidate, config)
    cases = sorted({row['case_id'] for row in rows if row['case_id']})
    return {'schema_version': SCHEMA, 'label': label, 'evidence_scope': 'native_replay',
            'harness_code_sha256': harness_code_sha256(), 'model_calls': 0,
            'tokenizers': baseline.metrics['tokenizers'],
            'representations': baseline.metrics['representations'],
            'baseline': _side_header(baseline), 'candidate': _side_header(candidate),
            'configuration_mismatch': config, 'limitations': list(LIMITATIONS),
            'summary': {'tasks': len(cases), 'journeys': len({row['journey'] for row in rows}),
                        'attempts_per_journey': 1, 'rows': len(rows),
                        'conclusions': dict(sorted(Counter(r['conclusion'] for r in rows).items()))},
            'cohorts': cohorts(rows), 'rows': rows,
            'oracle': {'baseline': _oracle_digest(baseline), 'candidate': _oracle_digest(candidate)}}


def _oracle_digest(side):
    """Per-journey oracle verdicts retained next to the numbers they qualify."""
    return [{'journey': j['journey'], 'quality_pass': j['quality_pass'],
             'quality_failures': j['quality_failures'], 'completeness': j['completeness'],
             'metrics': j['metrics'],
             'unmet_obligations': [o for o in j['obligations'] if o['satisfied'] is not True],
             'detail': j['detail']} for j in side.oracle['journeys']]
