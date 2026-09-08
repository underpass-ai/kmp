"""Check the declared identity and both timelines of the fictional INC-17."""


def check(saved, client, authored):
    refs = {name: saved[name]['generated_refs'][0]
            for name in ('outage', 'recovery', 'audit', 'identity')}
    assert len(set(refs.values())) == 4
    for name, kind in [('outage', 'observation'), ('recovery', 'success_path'),
                       ('audit', 'error_path'), ('identity', 'observation')]:
        assert saved[name]['accepted'] is True
        read = saved[name + '_read']
        assert read['object']['ref'] == refs[name]
        assert read['object']['kind'] == kind
        assert read['object']['text'] == authored[name]['current']['summary']
        assert any(e['text'] == authored[name]['current']['evidence'] for e in read['evidence'])

    before = saved['related']
    proposal = before['proposed'][0]
    assert {proposal['from'], proposal['to']} == {refs['outage'], refs['audit']}
    assert 'identifier' in proposal['proposed_by']
    assert any({p['from'], p['to']} == {refs['outage'], refs['recovery']}
               and p['proposed_by'] == ['entity'] for p in before['proposed'])
    assert all(e['rel'] not in {'same_event_as', 'same_entity_as'} for e in before['declared'])
    for name in ('unlinked', 'missing_proposal', 'unsupported_cross_link'):
        assert saved[name]['error']['code'] == 'invalid_argument'

    after = saved['declared']
    assert {f['ref'] for f in after['facts']} == set(refs.values()), 'Refused writes must not add memories'
    identities = [e for e in after['declared'] if e['rel'] in {'same_event_as', 'same_entity_as'}]
    assert len(identities) == 1
    edge = identities[0]
    assert (edge['from'], edge['to'], edge['rel']) == (refs['identity'], refs['audit'], 'same_event_as')
    assert edge['class'] == 'evidential' and edge['confidence'] == 'high'
    assert edge['why'] == authored['identity']['connect_to'][0]['why']
    assert edge['evidence'] == authored['identity']['current']['evidence']
    assert edge['method'] == 'kmp_relate:' + '+'.join(proposal['proposed_by'])
    assert edge in saved['identity_path']['trace']
    assert any(e['from'] == refs['identity'] and e['to'] == refs['outage']
               and e['rel'] == 'restates' for e in after['declared'])
    assert any(e['from'] == refs['audit'] and e['to'] == refs['recovery']
               and e['rel'] == 'uses_background' for e in after['declared'])
    for fact in after['facts']:
        name = next(name for name, ref in refs.items() if ref == fact['ref'])
        assert fact['about'] == authored[name]['about']
        assert all(coordinate['occurred_at'] == authored[name]['occurred_at']
                   and coordinate['observed_at'] == authored[name]['observed_at']
                   for coordinate in fact['coordinates'])

    old, new = saved['audit_read'], saved['audit_after']
    assert old['object'] == new['object']
    assert all(e in new['evidence'] for e in old['evidence'])
    old_coordinates = [e for e in old['links']['incoming'] if e.get('coordinate')]
    assert old_coordinates and all(e in new['links']['incoming'] for e in old_coordinates)

    known = saved['known_day1']
    assert {f['ref'] for f in known['facts']} == {refs['outage'], refs['recovery']}
    assert not known['declared'] and known['proof']['axis'] == 'observed'
    assert {e['ref'] for e in saved['project_recovery']['entries']} == {refs['recovery']}
    assert {e['ref'] for e in saved['support_reconciliation']['entries']} == {refs['identity']}
    assert {e['ref'] for e in saved['occurred_start']['entries']} == {refs['outage'], refs['audit'], refs['identity']}
    assert {e['ref'] for e in saved['occurred_later']['entries']} == {refs['recovery']}
    assert not saved['observed_start']['entries']
    assert [e['ref'] for e in saved['observed_later']['entries']] == list(refs.values())

    for name, selection, end in [('initial_view_state', 'recovery', '2026-09-02T00:00:00Z'),
                                 ('audit_view_state', 'audit', '2026-09-03T00:00:00Z'),
                                 ('view_state', 'identity', '2026-09-03T00:00:00Z')]:
        state = saved[name]['state']
        assert state['about'] == authored['outage']['about'] and state['clock'] == 'observed'
        assert state['projection']['abouts'] == [authored['audit']['about']]
        assert state['projection']['semantic_zoom'] == 'moment'
        assert state['selection'] == refs[selection]
        assert state['focus']['time_range']['to'] == end
    assert all(not saved[name]['unhonored'] for name in ('initial_frame', 'audit_frame', 'framed'))
    return ['four source-backed memories in two owning abouts',
            'proposal suggests both supported and unsupported identity pairs',
            'no cross-about path before declaration', 'invalid declarations write nothing',
            'one source-backed equivalence with copied proposal provenance',
            'trace preserves the declared rationale and evidence',
            'original audit text, evidence and clocks preserved',
            'recovery remains a separate event', 'both local timelines traversed',
            'late evidence absent from the earlier observed interval',
            'inclusive boundary and forward distinguish occurred from observed',
            'initial, intermediate and final shared view states']
