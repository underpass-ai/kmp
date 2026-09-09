"""Check persisted proof and clocks from the authored packet's real store."""


def check(saved, client, authored):
    refs = saved['written']['local_refs']
    assert saved['written']['accepted'] and saved['retry']['accepted']
    assert refs == saved['retry']['local_refs']
    assert saved['rejected']['feedback'][0]['code'] == 'MEMORY_EVIDENCE_REQUIRED'
    assert saved['rejected']['feedback'][0]['field'] == 'memories[1].evidence'
    assert saved['rejected']['feedback'][0]['action'] is None
    assert saved['written']['coverage']['source_coverage'] == 'not_assessed'
    assert saved['written']['coverage']['label_memberships'] == 6
    action = saved['written']['receipt']['action']
    detail = client.call(action['tool'], action['arguments'])
    import json
    receipt = json.loads(detail['object']['text'])['receipt']
    assert receipt['writer']['local_refs'] == refs
    assert receipt['writer']['coverage'] == saved['written']['coverage']
    assert receipt['canonical_memory']['relations'][0]['evidence'] == 'R2 cites R1 as the reason for the retry.'
    for index, name in enumerate(('choice', 'logs')):
        expected = authored['written']['memories'][index]
        record = next(row for row in saved[name]['raw'] if row['ref'] == refs[name])
        assert record['text'] == expected['summary']
        assert len(record['coordinates']) == (2 if name == 'choice' else 4)
        for coordinate in record['coordinates']:
            assert coordinate['observed_at'] == expected['observed_at']
            assert coordinate.get('occurred_at') == expected.get('occurred_at')
        assert expected['evidence'] in str(saved[name])
    assert 'R2 cites R1 as the reason for the retry.' in str(saved['proof'])
    assert {e['ref'] for e in saved['before_choice']['entries']} == {refs['logs']}
    assert {e['ref'] for e in saved['after_choice']['entries']} == set(refs.values())
    request = saved['request_read']
    assert saved['request']['accepted'] and request['object']['kind'] == 'observation'
    assert request['object']['text'] == authored['request']['memories'][0]['summary']
    assert any(e['text'] == authored['request']['memories'][0]['evidence'] for e in request['evidence'])
    outgoing = request['links']['outgoing']
    assert len(outgoing) == 1 and outgoing[0]['rel'] == 'uses_background'
    assert outgoing[0]['to'] == refs['choice']
    for key in ('why', 'evidence'):
        assert outgoing[0][key] == authored['request']['memories'][0]['connect_to'][0][key]
    assert saved['choice_after_request']['object'] == saved['choice']['object']
    assert not saved['request_time']['proof']['superseded']
    assert all(r['rel'] not in ('updates_state', 'supersedes') for r in saved['request_time']['proof']['path'])
    for key, value, expected_ref in [
            ('source', 'R1', refs['logs']), ('source', 'R2', refs['choice']),
            ('source', 'R3', request['object']['ref']),
            ('alias', 'NC', refs['logs']), ('alias', 'Nebula cache', refs['logs'])]:
        selected = client.call('kmp_forward', {
            'about': authored['written']['about'], 'axis': 'observed',
            'interval': {'start': '2026-09-01T09:00:00Z', 'end': '2026-09-01T11:00:00Z'},
            'dimensions': {'selectors': [{'key': key, 'op': 'in', 'values': [value]}]},
            'include': {'evidence': False, 'relations': False}, 'limit': {'entries': 10},
            'budget': {'max_bytes': 30000}})
        assert not selected['page']['has_more'] and not selected['selection']['has_more']
        assert [e['ref'] for e in selected['entries']] == [expected_ref]
    assert not saved['frame']['unhonored']
    assert saved['view_state']['state']['selection'] == refs['choice']
    return ['accepted receipt action recovers immutable canonical proof and declared coverage',
            'invalid packet leaves the about absent',
            'local forward link resolves to stored proof without invented prior reads',
            'shared component, individual source and aliases retain distinct membership',
            'unapproved request preserves the decision with an honest context relation',
            'all declared labels and individual clocks survive',
            'temporal selection distinguishes observation from receipt',
            'exact retry retains refs and shared view selects the decision']
