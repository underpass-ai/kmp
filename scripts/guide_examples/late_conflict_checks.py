"""Check historical conflict and late proof through the native protocol."""


def check(saved, client, authored):
    kinds = {'first': 'observation', 'second': 'observation', 'pending': 'decision',
             'record': 'observation', 'resolved': 'decision'}
    refs = {name: saved[name]['generated_refs'][0] for name in kinds}
    assert len(set(refs.values())) == 5
    for name, kind in kinds.items():
        read = saved[name + '_read']
        assert saved[name]['accepted']
        assert read['object']['kind'] == kind and read['object']['ref'] == refs[name]
        assert read['object']['text'] == authored[name]['current']['summary']
        assert any(e['text'] == authored[name]['current']['evidence'] for e in read['evidence'])
        for link in read['links']['incoming']:
            if 'coordinate' in link:
                assert link['coordinate']['observed_at'] == authored[name]['observed_at']
                assert link['coordinate']['occurred_at'] == authored[name]['occurred_at']

    for name in ('unresolved', 'unresolved_again', 'ask_before'):
        proof = saved[name]['proof']
        assert proof['axis'] == 'observed' and proof['as_of'] == '2026-09-04T00:00:00Z'
        assert not proof['superseded']
        assert len(proof['conflicts']) == 1
        assert refs['first'] in str(proof['conflicts']) and refs['second'] in str(proof['conflicts'])
        assert all(refs[later] not in str(proof['evidence']) for later in ('record', 'resolved'))
        assert any(e['text'] == authored['pending']['current']['summary'] for e in proof['evidence'])
    assert saved['unresolved']['proof']['conflicts'] == saved['unresolved_again']['proof']['conflicts']
    for name in ('resolved_recall', 'ask_after'):
        proof = saved[name]['proof']
        assert proof['axis'] == 'observed' and proof['as_of'] == '2026-09-06T00:00:00Z'
        assert not proof['conflicts']
        assert {e['ref'] for e in proof['superseded']} == {refs['second'], refs['pending']}
        assert all(e['superseded_by'] == refs['resolved'] for e in proof['superseded'])
        assert any(e['text'] == authored['resolved']['current']['summary'] for e in proof['evidence'])

    assert {e['ref'] for e in saved['shift_events']['entries']} == {refs[n] for n in ('first', 'second', 'record')}
    assert {e['ref'] for e in saved['receipt_history']['entries']} == set(refs.values())
    for entry in saved['receipt_history']['entries']:
        assert all(c['dimension'] == 'shift' for c in entry['coordinates'])
    assert saved['second_after']['object'] == saved['second_read']['object']
    for link in saved['second_read']['links']['incoming']:
        if 'coordinate' in link:
            current = next(e for e in saved['second_after']['links']['incoming'] if e['from'] == link['from'])
            assert current['coordinate'] == link['coordinate']
    for path, source, target, rel in [('disagreement', 'second', 'first', 'contradicts'),
                                    ('signed_proof', 'resolved', 'record', 'verified_by'),
                                    ('replacement', 'resolved', 'second', 'supersedes'),
                                    ('closed_review', 'resolved', 'pending', 'supersedes')]:
        expected = next(e for e in authored[source]['connect_to'] if e['ref'] == refs[target] and e['rel'] == rel)
        actual = next(e for e in saved[path]['trace'] if e['from'] == refs[source] and e['to'] == refs[target] and e['rel'] == rel)
        for field in ('class', 'why', 'evidence'):
            assert actual[field] == expected[field]
        assert actual['why'] != actual['evidence']
    for name in ('initial', 'conflict', 'event', 'final'):
        assert not saved[name + '_frame']['unhonored']
        state = saved[name + '_state']['state']
        assert state['clock'] == ('occurred' if name == 'event' else 'observed')
        assert state['projection']['semantic_zoom'] == 'moment'
    assert saved['view_state']['state']['selection'] == refs['resolved']
    return ['five attributed sources with observation and decision kinds',
            'conflicting assignments remain unresolved before the signed record',
            'earlier observed proof is unchanged after the late resolution',
            'later proof replaces only R2 and P1 and exposes no live conflict',
            'event time and receipt time place the late source differently',
            'shift traversal retains component and environment selector semantics',
            'the replaced report and original clocks remain intact',
            'four directed paths retain exact rationale and evidence',
            'four explicit shared-view stages']
