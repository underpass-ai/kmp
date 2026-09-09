"""Check persisted proof and clocks from the authored packet's real store."""


def check(saved, client, authored):
    refs = saved['written']['local_refs']
    assert saved['written']['accepted'] and saved['retry']['accepted']
    assert refs == saved['retry']['local_refs']
    assert saved['rejected']['feedback'][0]['code'] == 'MEMORY_EVIDENCE_REQUIRED'
    assert saved['rejected']['feedback'][0]['field'] == 'memories[1].evidence'
    assert saved['rejected']['feedback'][0]['action'] is None
    for index, name in enumerate(('choice', 'logs')):
        expected = authored['written']['memories'][index]
        record = next(row for row in saved[name]['raw'] if row['ref'] == refs[name])
        assert record['text'] == expected['summary']
        assert len(record['coordinates']) == (1 if name == 'choice' else 3)
        for coordinate in record['coordinates']:
            assert coordinate['observed_at'] == expected['observed_at']
            assert coordinate.get('occurred_at') == expected.get('occurred_at')
        assert expected['evidence'] in str(saved[name])
    assert 'R2 cites R1 as the reason for the retry.' in str(saved['proof'])
    assert {e['ref'] for e in saved['before_choice']['entries']} == {refs['logs']}
    assert {e['ref'] for e in saved['after_choice']['entries']} == set(refs.values())
    assert not saved['frame']['unhonored']
    assert saved['view_state']['state']['selection'] == refs['choice']
    return ['invalid packet leaves the about absent',
            'local forward link resolves to stored proof without invented prior reads',
            'all declared labels and individual clocks survive',
            'temporal selection distinguishes observation from receipt',
            'exact retry retains refs and shared view selects the decision']
