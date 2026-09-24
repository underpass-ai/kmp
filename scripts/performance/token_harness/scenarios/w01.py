"""W01: orient in a small about whose sources outnumber its memories."""
from .model import FixtureWrite, JourneySpec, Obligation, ObligationKind as K, Scenario

ABOUT = 'fixture:w01'
DECISION = 'Use blue-green deploys for the ledger service.'
POOL = 'A second ledger pool is provisioned.'
CUTOVER = 'Cutover is scheduled for Friday.'
E_DECISION = 'S1: the ops review chose blue-green deploys for the ledger.'
E_POOL = 'S2: the capacity report lists pool ledger-b as ready.'
E_CUTOVER = 'S3: the change calendar lists the ledger cutover on Friday.'
E_LINK = 'S2: the second pool was provisioned for the switch.'
E_BACKS_DECISION = 'S4: the capacity sign-off cites pool ledger-b for the switch.'
E_BACKS_CUTOVER = 'S5: the cutover checklist requires pool ledger-b to be ready.'

SCENARIO = Scenario(
    case_id='w01', goal_id='orient_small_about', about=ABOUT,
    description='Three memories and many supports edges; first usable delivery of the resume packet.',
    fixture=(
        FixtureWrite({
            'about': ABOUT, 'actor': 'fixture-writer', 'observed_at': '2026-09-01T10:00:00Z',
            'idempotency_key': 'fixture-w01-memories', 'labels': {'task': ['T-1']},
            'memories': [
                {'id': 'a', 'kind': 'decision', 'summary': DECISION, 'evidence': E_DECISION,
                 'labels': {'source': ['S1']},
                 'connect_to': [{'ref': 'b', 'rel': 'depends_on', 'confidence': 'high',
                                 'why': 'Blue-green needs a second pool.', 'evidence': E_LINK}]},
                {'id': 'b', 'kind': 'observation', 'summary': POOL, 'evidence': E_POOL,
                 'labels': {'source': ['S2']}},
                {'id': 'c', 'kind': 'constraint', 'summary': CUTOVER, 'evidence': E_CUTOVER,
                 'labels': {'source': ['S3']}},
            ]}),
        FixtureWrite({
            'about': ABOUT, 'actor': 'fixture-writer', 'observed_at': '2026-09-01T10:05:00Z',
            'idempotency_key': 'fixture-w01-supports',
            'relations': [
                {'from': '${b}', 'to': '${a}', 'rel': 'supports',
                 'why': 'The ready pool backs the blue-green decision.', 'evidence': E_BACKS_DECISION},
                {'from': '${b}', 'to': '${c}', 'rel': 'supports',
                 'why': 'The ready pool backs the Friday cutover.', 'evidence': E_BACKS_CUTOVER},
            ]}),
    ),
    journey=JourneySpec('kmp_wake', {'about': ABOUT}),
    obligations=(
        Obligation(K.STATE_MENTIONS, DECISION), Obligation(K.STATE_MENTIONS, POOL),
        Obligation(K.STATE_MENTIONS, CUTOVER),
        Obligation(K.EVIDENCE_BODY, E_DECISION), Obligation(K.EVIDENCE_BODY, E_POOL),
        Obligation(K.EVIDENCE_BODY, E_CUTOVER),
    ),
    unit_bodies=(E_DECISION, E_POOL, E_CUTOVER),
    stored_bodies=(DECISION, POOL, CUTOVER, E_DECISION, E_POOL, E_CUTOVER, E_LINK,
                   E_BACKS_DECISION, E_BACKS_CUTOVER),
)
