"""A01: ask a question one short stored source answers."""
from .model import FixtureWrite, JourneySpec, Obligation, ObligationKind as K, Scenario

ABOUT = 'fixture:a01'
SUMMARY = 'The staging database runs PostgreSQL 16.4.'
BODY = 'S1: the staging inventory lists PostgreSQL 16.4 on db-stg-1.'

SCENARIO = Scenario(
    case_id='a01', goal_id='ask_short_source', about=ABOUT,
    description='One short source answers; a descriptor-then-body detour is a cost, not a gain.',
    fixture=(FixtureWrite({
        'about': ABOUT, 'actor': 'fixture-writer', 'observed_at': '2026-09-04T08:00:00Z',
        'idempotency_key': 'fixture-a01',
        'memories': [{'id': 'pg', 'kind': 'observation', 'summary': SUMMARY, 'evidence': BODY,
                      'labels': {'source': ['S1']}}]}),),
    journey=JourneySpec('kmp_ask', {'about': ABOUT,
                                    'question': 'Which PostgreSQL version runs on the staging database?'}),
    obligations=(Obligation(K.EVIDENCE_BODY, BODY),),
    unit_bodies=(BODY,),
    stored_bodies=(SUMMARY, BODY),
)
