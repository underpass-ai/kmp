"""Check identity evidence separately from the timeline of account assignment."""


def check(saved, client, authored):
    refs = {name: saved[name]['generated_refs'][0] for name in ('elena', 'jon', 'alias', 'visitor', 'rui')}
    assert len(set(refs.values())) == 5
    for name, kind in [('elena', 'observation'), ('alias', 'observation'), ('visitor', 'observation'),
                       ('jon', 'decision'), ('rui', 'decision')]:
        assert saved[name]['accepted'] is True
        inspected = saved[name + '_read']
        assert inspected['object']['ref'] == refs[name]
        assert inspected['object']['kind'] == kind
        assert inspected['object']['text'] == authored[name]['memories'][0]['summary']
        assert any(e['text'] == authored[name]['memories'][0]['evidence'] for e in inspected['evidence'])
    assert saved['alias_label']['accepted'] is True
    before, after = saved['elena_before'], saved['elena_read']
    assert before['object'] == after['object'], 'Relabel must preserve the canonical memory'
    # Later incoming relations may add proof beside the original source.
    assert all(evidence in after['evidence'] for evidence in before['evidence']), 'Original proof must remain intact'
    for link in before['links']['incoming']:
        if link.get('coordinate'):
            current = next(item for item in after['links']['incoming'] if item['from'] == link['from'])
            assert current['coordinate'] == link['coordinate'], 'Existing clocks must remain unchanged'
    labels = {(label['key'], label['value']): label['entries'] for label in saved['catalogue']['labels']}
    assert labels[('alias', 'Nora')] == 3
    assert labels[('account', '@oak')] == 2
    assert {entry['ref'] for entry in saved['name_at_alias']['entries']} == {refs['elena'], refs['alias']}
    assert {entry['ref'] for entry in saved['name_slice']['entries']} == {refs['elena'], refs['alias'], refs['visitor']}
    related = saved['alias_dependencies']
    assert [entry['ref'] for entry in related['entries']] == [refs['alias']]
    group = related['proof']['groups'][0]
    assert set(group['member_refs']) == {refs['alias'], refs['elena'], refs['jon']}
    assert refs['visitor'] not in group['member_refs'] and refs['rui'] not in group['member_refs']
    assert group['omitted_by_limit'] == 0
    assert any(edge['from'] == refs['jon'] and edge['to'] == refs['elena'] and edge['rel'] == 'uses_background'
               for edge in related['proof']['path'])
    assert any(entry['ref'] == refs['elena'] and entry['text'] == authored['elena']['memories'][0]['summary']
               and entry['coordinates'] for entry in related['proof']['entries'])
    assert any(item['text'] == authored['elena']['memories'][0]['evidence'] for item in related['proof']['evidence'])
    assert {entry['ref'] for entry in saved['account_history']['entries']} == {refs['jon'], refs['rui']}
    for name, source, target, relation in [('identity_path', 'alias', 'elena', 'same_entity_as'),
                                          ('assignment_path', 'rui', 'jon', 'supersedes')]:
        assert any(edge['from'] == refs[source] and edge['to'] == refs[target] and edge['rel'] == relation
                   and edge['class'] == 'evidential' and edge['why'] and edge['evidence']
                   for edge in saved[name]['trace'])
    visitor = saved['visitor_read']
    assert all(link['rel'] != 'same_entity_as' for link in visitor['links']['incoming'] + visitor['links']['outgoing'])
    assert all(link.get('coordinate', {}).get('dimension') != 'person' for link in visitor['links']['incoming'])
    for name in ('jon', 'rui'):
        node = saved[name + '_read']
        assert all(link['rel'] != 'same_entity_as' for link in node['links']['incoming'] + node['links']['outgoing'])
    day3, day6 = saved['day3']['proof'], saved['day6']['proof']
    assert day3['axis'] == day6['axis'] == 'validity'
    assert day3['as_of'] == '2026-09-03T12:00:00Z' and day6['as_of'] == '2026-09-06T12:00:00Z'
    assert any(e['text'] == authored['jon']['memories'][0]['summary'] for e in day3['evidence'])
    assert all(refs['rui'] not in str(e) for e in day3['evidence'])
    assert not day3['superseded']
    assert any(e['text'] == authored['rui']['memories'][0]['summary'] for e in day6['evidence'])
    assert any(item['ref'] == refs['jon'] and item['superseded_by'] == refs['rui'] for item in day6['superseded'])
    unknown = saved['before_alias_known']
    assert unknown['answer'] == 'UNKNOWN' and not unknown['proof']['evidence']
    assert unknown['proof']['axis'] == 'observed' and unknown['proof']['as_of'] == '2026-09-01T12:00:00Z'
    state = saved['view_state']['state']
    assert state['selection'] == refs['alias']
    assert state['projection']['semantic_zoom'] == 'moment'
    assert not saved['framed']['unhonored']
    return ['five distinct memories with literal sources', 'supported person identity',
            'unresolved same-name visitor retained without identity or person label',
            'relabel preserves original text, evidence and clocks', 'label slice does not imply identity',
            'account history remains two assignments', 'dated assignment before and after replacement',
            'later alias label does not supply earlier identity evidence',
            'no person/account or Jon/Rui identity link', 'shared view frames the distinction']
