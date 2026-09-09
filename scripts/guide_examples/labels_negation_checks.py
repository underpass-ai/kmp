"""Check source-backed catalogue changes and scoped negation through MCP."""


def check(saved, client, authored):
    kinds = {'original': 'observation', 'glossary': 'constraint',
             'restated': 'observation', 'negative': 'observation',
             'other_environment': 'observation', 'later_decision': 'decision'}
    refs = {name: saved[name]['generated_refs'][0] for name in kinds}
    assert len(set(refs.values())) == 6
    for name, kind in kinds.items():
        read = saved[name + '_read']
        assert saved[name]['accepted']
        assert read['object']['kind'] == kind and read['object']['ref'] == refs[name]
        assert read['object']['text'] == authored[name]['memories'][0]['summary']
        assert any(e['text'] == authored[name]['memories'][0]['evidence'] for e in read['evidence'])
        coords = [e['coordinate'] for e in read['links']['incoming'] if e.get('coordinate')]
        assert coords
        assert all(c['occurred_at'] == authored[name]['occurred_at'] and
                   c['observed_at'] == authored[name]['observed_at'] for c in coords)

    early = saved['english_summary']
    assert early['answer'] != 'UNKNOWN'
    assert any(e['text'] == authored['original']['memories'][0]['summary'] and
               e.get('metadata', {}).get('matched_via') == 'summary'
               for e in early['proof']['evidence'])
    assert saved['canonical_label']['accepted']
    now = {(e['key'], e['value']) for e in saved['canonical_label']['labels']['now']}
    assert ('component', 'journal') in now and ('component', 'bitácora') not in now
    before, after = saved['original_before'], saved['original_read']
    assert before['object'] == after['object']
    assert all(e in after['evidence'] for e in before['evidence'])
    old = [e['coordinate'] for e in before['links']['incoming'] if e.get('coordinate')]
    new = [e['coordinate'] for e in after['links']['incoming'] if e.get('coordinate')]
    assert len(old) == len(new)
    old_component = next(c for c in old if c['dimension'] == 'component')
    new_component = next(c for c in new if c['dimension'] == 'component')
    for clock in ('occurred_at', 'observed_at', 'ingested_at', 'valid_from', 'valid_until'):
        assert old_component.get(clock) == new_component.get(clock)
    for c in old:
        if c['dimension'] != 'component':
            assert c in new

    expected = {
        'component_history': set(refs),
        'same_conditions': {'original', 'restated', 'negative'},
        'staging_only': {'other_environment'},
        'later_only': {'later_decision'},
        'not_staging': set(refs) - {'other_environment'},
        'known_not_staging': set(refs) - {'other_environment', 'glossary'},
    }
    for name, selected in expected.items():
        entries = saved[name]['entries']
        assert {e['ref'] for e in entries} == {refs[n] for n in selected}, name
        assert all(c['dimension'] == 'component' for e in entries for c in e['coordinates'])
    catalogue = {(e['key'], e['value']): e['entries'] for e in saved['catalogue']['labels']}
    assert catalogue[('component', 'journal')] == 6

    for path, source, rel in [('restatement_path', 'restated', 'restates'),
                              ('contradiction_path', 'negative', 'contradicts')]:
        wanted = next(e for e in authored[source]['memories'][0]['connect_to'] if e['rel'] == rel)
        actual = next(e for e in saved[path]['trace'] if e['from'] == refs[source] and
                      e['to'] == refs['original'] and e['rel'] == rel)
        for field in ('class', 'why', 'evidence'):
            assert actual[field] == wanted[field]
        assert actual['why'] != actual['evidence']
    for name in ('other_environment', 'later_decision'):
        read = saved[name + '_read']
        assert all(e['rel'] not in ('restates', 'same_entity_as', 'contradicts', 'supersedes')
                   for e in read['links']['outgoing'])
    proof = saved['conflict_after_later_decision']['proof']
    assert proof['axis'] == 'observed' and proof['as_of'] == '2026-09-03T00:00:00Z'
    assert not proof['superseded'] and len(proof['conflicts']) == 1
    assert refs['original'] in str(proof['conflicts']) and refs['negative'] in str(proof['conflicts'])
    for name in ('conflict', 'staging', 'catalogue'):
        assert not saved[name + '_frame']['unhonored']
        state = saved[name + '_state']['state']
        assert state['clock'] == 'observed' and state['projection']['semantic_zoom'] == 'moment'
    assert saved['view_state']['state']['selection'] == refs['glossary']
    return ['six literal sources with observation, constraint and decision kinds',
            'English summary retrieval returns the original Spanish evidence',
            'glossary-backed relabel preserves source and all original clocks',
            'component grouping does not equate six distinct memories',
            'restatement and negation retain separate directed proof paths',
            'matching environment, owner and check keeps both opposite reports',
            'different environment and later decision create no false contradiction',
            'notin and exists distinguish missing environment from known scope',
            'later maintenance decision does not resolve the earlier disagreement',
            'three shared scenes expose the selected conditions']
