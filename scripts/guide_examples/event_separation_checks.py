"""Native admission checks for two events described by one later report."""
from datetime import datetime


def check(saved, client, authored):
    assert saved['pending']['status'] == 'needs_review'
    assert not saved['pending']['accepted']
    assert saved['written']['accepted']
    refs = saved['written']['local_refs']
    for memory in authored['pending']['memories']:
        inspected = saved[memory['id'] + '_read']
        assert inspected['object']['text'] == memory['summary']
        assert any(e['text'] == memory['evidence'] for e in inspected['evidence'])
        coords = [e['coordinate'] for e in inspected['links']['incoming'] if 'coordinate' in e]
        assert coords
        assert all(c['occurred_at'] == memory['occurred_at'] and
                   c['observed_at'] == authored['pending']['observed_at'] for c in coords)
    expected = {'occurred_before': {'execution'}, 'occurred_after': set(refs),
                'observed_before': set(), 'observed_after': set(refs)}
    for name, names in expected.items():
        result, request = saved[name], authored[name]
        assert not result['page']['has_more'] and not result['selection']['has_more']
        assert {e['ref'] for e in result['entries']} == {refs[n] for n in names}
        cutoff = datetime.fromisoformat(request['at']['time'])
        for entry in result['entries'] + result['proof'].get('entries', []):
            assert any(c.get(request['axis'] + '_at') and
                       datetime.fromisoformat(c[request['axis'] + '_at']) <= cutoff
                       for c in entry['coordinates'])
    edge = saved['path']['trace'][0]
    declared = authored['pending']['memories'][0]['connect_to'][0]
    assert (edge['from'], edge['rel'], edge['to']) == (refs['execution'], 'verified_by', refs['check'])
    assert edge['why'] == declared['why'] and edge['evidence'] == declared['evidence']
    assert edge['clocks']['observed_at'] == authored['pending']['observed_at']
    assert edge['clocks'].get('ingested_at')
    assert 'occurred_at' not in edge['clocks'] and 'valid_from' not in edge['clocks']
    assert saved['written']['clocks']['relations']['observed'] == 1
    for name, selected in [('before_frame', 'execution'), ('after_frame', 'check')]:
        assert saved[name]['applied'] and not saved[name]['unhonored']
        assert saved[name]['state']['selection'] == refs[selected]
        assert saved[name]['state']['clock'] == 'occurred'
    assert saved['view_state']['state'] == saved['after_frame']['state']
    return ['separate literal event summaries and evidence fragments',
            'shared observation and distinct occurrence coordinates',
            'review before materialization of a local rich link',
            'four cutoffs admit exactly the intended primary entries',
            'no later dependency is admitted at an earlier cutoff',
            'directed verification proof preserves rationale and evidence',
            'two viewer stages preserve separate occurrence selection']
