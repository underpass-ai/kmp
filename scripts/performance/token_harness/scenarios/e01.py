"""E01: one simple write and its receipt; effects checked by an oracle read-back."""
from .model import JourneySpec, Obligation, ObligationKind as K, Scenario

ABOUT = 'fixture:e01'
SUMMARY = 'The nightly backup job moved to 02:30 UTC.'
BODY = 'S1: the scheduler change log moves the nightly backup to 02:30 UTC.'

SCENARIO = Scenario(
    case_id='e01', goal_id='simple_write_receipt', about=ABOUT,
    description='Independent observation: one call, no preview, receipt and persisted source.',
    fixture=(),
    journey=JourneySpec('kmp_write_memory', {
        'about': ABOUT, 'actor': 'journey-writer', 'observed_at': '2026-09-05T07:00:00Z',
        'idempotency_key': 'journey-e01',
        'memories': [{'id': 'backup', 'kind': 'observation', 'summary': SUMMARY, 'evidence': BODY,
                      'labels': {'source': ['S1']}}]}, budgets=(None,)),
    obligations=(Obligation(K.WRITE_ACCEPTED, local_id='backup'), Obligation(K.READBACK_BODY, BODY)),
    unit_bodies=(BODY,),
    stored_bodies=(SUMMARY, BODY),
    readback=True,
)
