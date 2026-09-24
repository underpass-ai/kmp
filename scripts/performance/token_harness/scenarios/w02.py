"""W02: resume an explicit task label while a more recent unrelated task exists."""
from .model import FixtureWrite, JourneySpec, Obligation, ObligationKind as K, Scenario

ABOUT = 'fixture:w02'
A_DECISION = 'Export billing records as parquet files.'
A_OBSERVATION = 'The parquet exporter passed the schema check.'
B_DECISION = 'Rotate the staging TLS certificates.'
B_OBSERVATION = 'Two staging certificates expire this week.'
E_A_DECISION = 'S1: the billing sync notes choose parquet for exports.'
E_A_OBSERVATION = 'S2: the exporter CI run shows the schema check passing.'
E_B_DECISION = 'S3: the security rota assigns the staging certificate rotation.'
E_B_OBSERVATION = 'S4: the certificate inventory flags two staging expiries.'

SCENARIO = Scenario(
    case_id='w02', goal_id='resume_explicit_label', about=ABOUT,
    description='Resume task T-A by label; the newer task T-B must not displace it.',
    fixture=(
        FixtureWrite({
            'about': ABOUT, 'actor': 'fixture-writer', 'observed_at': '2026-09-02T09:00:00Z',
            'idempotency_key': 'fixture-w02-task-a', 'labels': {'task': ['T-A']},
            'memories': [
                {'id': 'a1', 'kind': 'decision', 'summary': A_DECISION, 'evidence': E_A_DECISION,
                 'labels': {'source': ['S1']}},
                {'id': 'a2', 'kind': 'observation', 'summary': A_OBSERVATION,
                 'evidence': E_A_OBSERVATION, 'labels': {'source': ['S2']}},
            ]}),
        FixtureWrite({
            'about': ABOUT, 'actor': 'fixture-writer', 'observed_at': '2026-09-12T09:00:00Z',
            'idempotency_key': 'fixture-w02-task-b', 'labels': {'task': ['T-B']},
            'memories': [
                {'id': 'b1', 'kind': 'decision', 'summary': B_DECISION, 'evidence': E_B_DECISION,
                 'labels': {'source': ['S3']}},
                {'id': 'b2', 'kind': 'observation', 'summary': B_OBSERVATION,
                 'evidence': E_B_OBSERVATION, 'labels': {'source': ['S4']}},
            ]}),
    ),
    journey=JourneySpec('kmp_wake', {'about': ABOUT, 'dimensions': {
        'selectors': [{'key': 'task', 'op': 'in', 'values': ['T-A']}]}}),
    obligations=(
        Obligation(K.STATE_MENTIONS, A_DECISION), Obligation(K.STATE_MENTIONS, A_OBSERVATION),
        Obligation(K.STATE_EXCLUDES, B_DECISION, 'b1'),
        Obligation(K.STATE_EXCLUDES, B_OBSERVATION, 'b2'),
    ),
    unit_bodies=(E_A_DECISION, E_A_OBSERVATION),
    stored_bodies=(A_DECISION, A_OBSERVATION, B_DECISION, B_OBSERVATION, E_A_DECISION,
                   E_A_OBSERVATION, E_B_DECISION, E_B_OBSERVATION),
)
