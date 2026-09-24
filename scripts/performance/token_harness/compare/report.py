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
)


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
