"""Evidence assertions for the authored history; these are not LLM scores."""


from copy import deepcopy


def check(saved, client, authored):
    refs = {name: saved[name]['generated_refs'][0] for name in ('offline', 'sqlite', 'shared', 'postgres')}
    assert len(set(refs.values())) == 4
    for name in refs:
        assert saved[name]['accepted'] is True
    for name, kind in [('offline', 'constraint'), ('shared', 'constraint'),
                       ('sqlite', 'decision'), ('postgres', 'decision')]:
        inspected = saved[name + '_read']
        assert inspected['object']['kind'] == kind
        assert inspected['object']['ref'] == refs[name]
        assert inspected['object']['text'] == authored[name]['current']['summary']
        assert any(e['text'] == authored[name]['current']['evidence'] for e in inspected['evidence'])
    outgoing = saved['postgres_read']['links']['outgoing']
    for relation, target, relation_class in [('chosen_because', 'shared', 'motivational'),
                                             ('supersedes', 'sqlite', 'evidential')]:
        link = next(link for link in outgoing if link['rel'] == relation and link['to'] == refs[target])
        assert link['class'] == relation_class
        assert link['why'] and link['evidence'] and link['why'] != link['evidence']
    # The rest of the assertions depend on the public response shapes. Keep
    # them explicit so a changed contract cannot silently weaken the lesson.
    def entry_refs(name):
        return {entry['ref'] for entry in saved[name]['entries']}

    assert entry_refs('start') == {refs['offline']}
    assert entry_refs('later') == {refs['sqlite'], refs['shared'], refs['postgres']}
    assert entry_refs('earlier') == {refs['offline'], refs['sqlite'], refs['shared']}
    day3 = saved['day3']['proof']
    day6 = saved['day6']['proof']
    assert day3['axis'] == day6['axis'] == 'validity'
    assert day3['as_of'] == '2026-09-03T12:00:00Z'
    assert day6['as_of'] == '2026-09-06T12:00:00Z'
    assert any(e['text'] == saved['sqlite_read']['object']['text'] for e in day3['evidence'])
    assert all(refs['postgres'] not in str(e) for e in day3['evidence'])
    assert not day3['superseded']
    assert any(e['text'] == saved['postgres_read']['object']['text'] for e in day6['evidence'])
    assert refs['sqlite'] in str(day6['superseded'])
    for name, relation in [('replacement', 'supersedes'), ('motivation', 'chosen_because')]:
        assert any(edge['rel'] == relation for edge in saved[name]['trace'])
    state = saved['view_state']['state']
    assert state['selection'] == refs['postgres']
    assert state['projection']['semantic_zoom'] == 'moment'
    assert state['clock'] == 'occurred'
    assert not saved['framed']['unhonored']
    for removed in ('evidence', 'class'):
        invalid = deepcopy(authored['postgres'])
        invalid['idempotency_key'] = 'guide-history:negative:' + removed
        invalid['options'] = {'dry_run': True}
        if removed == 'evidence':
            del invalid['connect_to'][0]['evidence']
        else:
            invalid['connect_to'][0]['class'] = 'structural'
        client.call('kmp_write_memory', invalid, expect_error='invalid_argument')
    retry = client.call('kmp_write_memory', authored['postgres'])
    assert retry['accepted'] and retry['generated_refs'] == [refs['postgres']]
    return ['four distinct typed memories', 'literal sources retained', 'motivational and evidential links',
            'inclusive start plus exclusive forward', 'rewind preserves history',
            'historical validity before and after replacement', 'both proof paths', 'shared view intent',
            'strict rejection of missing evidence and unsupported class', 'exact retry keeps the same ref']
