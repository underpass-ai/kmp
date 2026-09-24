"""W03: two sources with the same body and different provenance stay two units."""
from .model import FixtureWrite, JourneySpec, Obligation, ObligationKind as K, Scenario

ABOUT = 'fixture:w03'
SUMMARY = 'Disk usage on node-7 reached 91 percent.'
BODY = 'Monitoring export: node-7 disk at 91 percent at 09:00 UTC.'


def _write(actor, observed_at, source):
    return FixtureWrite({
        'about': ABOUT, 'actor': actor, 'observed_at': observed_at,
        'idempotency_key': 'fixture-w03-' + actor,
        'memories': [{'id': 'm', 'kind': 'observation', 'summary': SUMMARY, 'evidence': BODY,
                      'labels': {'source': [source]}}]})


SCENARIO = Scenario(
    case_id='w03', goal_id='orient_identical_bodies', about=ABOUT,
    description='Two writers store an identical body; identities and provenance must not merge.',
    fixture=(_write('writer-alpha', '2026-09-03T09:00:00Z', 'S1'),
             _write('writer-beta', '2026-09-03T09:30:00Z', 'S2')),
    journey=JourneySpec('kmp_wake', {'about': ABOUT}),
    obligations=(Obligation(K.STATE_MENTIONS, SUMMARY),
                 Obligation(K.DISTINCT_PROVENANCE, BODY, count=2)),
    unit_bodies=(BODY,),
    stored_bodies=(SUMMARY, BODY),
)
