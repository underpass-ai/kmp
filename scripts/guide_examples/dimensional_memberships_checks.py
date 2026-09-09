"""Check the new lesson against native responses; no model calls."""

def check(saved, client, authored):
    ref = saved['original']['generated_refs'][0]
    assert saved['original']['accepted'] and saved['change']['accepted']
    assert len(saved['two_aliases']['entries']) == 1
    assert saved['retired_alias']['entries'] == []
    for name in ('component', 'new_alias'):
        assert [e['ref'] for e in saved[name]['entries']] == [ref]
    now = {(e['key'], e['value']) for e in saved['change']['labels']['now']}
    assert now == {('agentic_process', 'registry'), ('component', 'neb'),
                   ('alias', 'Nébula Cache'), ('alias', 'NC'), ('alias', 'Nebula service')}
    assert saved['before']['object'] == saved['after']['object']
    record = next(r for r in saved['after']['raw'] if r['ref'] == ref)
    assert len(record['coordinates']) == 5
    assert all(c['occurred_at'] == authored['original']['occurred_at'] and
               c['observed_at'] == authored['original']['observed_at']
               for c in record['coordinates'])
    assert not saved['frame']['unhonored']
    assert saved['view_state']['state']['selection'] == ref
    return ['distinct facets and multiple aliases', 'one memory per selection',
            'one-membership correction preserves source and clocks', 'shared visual selection']
