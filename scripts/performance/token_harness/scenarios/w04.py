"""W04: a dated wake must not present a later memory as state at as_of."""
from .model import FixtureWrite, JourneySpec, Obligation, ObligationKind as K, Scenario

ABOUT = 'fixture:w04'
FREEZE = 'Freeze schema changes during the audit.'
LIFT = 'Lift the schema freeze after the audit closed.'
E_FREEZE = 'S1: the audit kickoff notes freeze all schema changes.'
E_LIFT = 'S2: the audit closing memo lifts the schema freeze.'
WHY = 'The closing memo ends the freeze.'

SCENARIO = Scenario(
    case_id='w04', goal_id='dated_wake', about=ABOUT,
    description='as_of 2026-09-10 sits between the freeze (09-02) and its lift (09-20).',
    fixture=(
        FixtureWrite({
            'about': ABOUT, 'actor': 'fixture-writer', 'idempotency_key': 'fixture-w04-freeze',
            'memories': [{'id': 'freeze', 'kind': 'decision', 'summary': FREEZE, 'evidence': E_FREEZE,
                          'observed_at': '2026-09-02T09:00:00Z', 'occurred_at': '2026-09-02T09:00:00Z',
                          'labels': {'source': ['S1']}}]}),
        FixtureWrite({
            'about': ABOUT, 'actor': 'fixture-writer', 'idempotency_key': 'fixture-w04-lift',
            'memories': [{'id': 'lift', 'kind': 'decision', 'summary': LIFT, 'evidence': E_LIFT,
                          'observed_at': '2026-09-20T09:00:00Z', 'occurred_at': '2026-09-20T09:00:00Z',
                          'labels': {'source': ['S2']},
                          'connect_to': [{'ref': '${freeze}', 'rel': 'supersedes', 'confidence': 'high',
                                          'why': WHY, 'evidence': E_LIFT}]}]}),
    ),
    journey=JourneySpec('kmp_wake', {'about': ABOUT, 'as_of': {'time': '2026-09-10T00:00:00Z'}}),
    obligations=(Obligation(K.STATE_MENTIONS, FREEZE), Obligation(K.EVIDENCE_BODY, E_FREEZE),
                 Obligation(K.AS_OF_EXCLUDES, LIFT, 'lift')),
    unit_bodies=(E_FREEZE,),
    stored_bodies=(FREEZE, LIFT, E_FREEZE, E_LIFT, WHY),
)
