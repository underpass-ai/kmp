"""Verify a fresh reader, idempotent writes and a changing shared frame."""


def check(saved, authored, sessions):
    assert [s['name'] for s in sessions] == ['writer', 'reader']
    assert sessions[0]['pid'] != sessions[1]['pid'] and sessions[0]['exit_code'] == 0
    refs = {name: saved[name]['generated_refs'][0] for name in ('constraint', 'decision', 'handoff', 'feedback')}
    assert saved['decision_retry']['generated_refs'] == saved['decision']['generated_refs']
    assert authored['decision_retry'] == {k: v for k, v in authored['decision'].items() if k != 'review_token'}
    assert saved['resume']['resume_cursor']['ref'] == refs['handoff']
    for name in ('constraint', 'decision', 'handoff'):
        read = saved['recovered_' + name]
        assert read['object']['ref'] == refs[name]
        assert read['object']['text'] == authored[name]['memories'][0]['summary']
        assert read['object']['kind'] == authored[name]['memories'][0]['kind']
        assert any(e['text'] == authored[name]['memories'][0]['evidence'] for e in read['evidence'])
    # Reader navigation binds to its own wake/inspect responses, not injected writer refs.
    assert authored['recovered_handoff']['ref'] == saved['resume']['resume_cursor']['ref']
    assert authored['recovered_decision']['ref'] == saved['recovered_handoff']['links']['outgoing'][0]['to']
    assert authored['recovered_constraint']['ref'] == saved['recovered_decision']['links']['outgoing'][0]['to']
    edges = saved['handoff_proof']['trace']
    assert [(e['from'], e['to'], e['rel']) for e in edges] == [
        (refs['handoff'], refs['decision'], 'uses_background'),
        (refs['decision'], refs['constraint'], 'chosen_because')]
    for name, edge in zip(('handoff', 'decision'), edges):
        for field in ('why', 'evidence', 'class'):
            assert edge[field] == authored[name]['memories'][0]['connect_to'][0][field]
    before, human = saved['before_human']['state'], saved['human_state']['state']
    assert before['clock'] == 'occurred' and before['selection'] == refs['decision']
    assert human['clock'] == 'observed' and human['selection'] == refs['constraint']
    assert human['last_change']['actor'] == 'human' and human['view_revision'] > before['view_revision']
    replay = saved['old_intent_replay']
    assert not replay['applied'] and replay['state'] == human
    assert authored['old_intent_replay'] == authored['initial_frame']
    assert saved['stale_intent']['error']['code'] == 'conflict'
    assert saved['after_conflict']['state'] == human
    assert authored['human_cursor']['axis'] == 'observed'
    assert refs['constraint'] in {e['ref'] for e in saved['human_cursor']['entries']}
    assert len(saved['decision_proof']['trace']) == 1
    prior, rebased = saved['ready_to_rebase']['state'], saved['rebased']['state']
    assert saved['rebased']['applied'] and not saved['rebased']['unhonored']
    for field in ('about', 'clock', 'focus', 'projection', 'search'):
        assert rebased[field] == prior[field]
    assert rebased['selection'] == refs['decision']
    assert rebased['view_revision'] == prior['view_revision'] + 1
    assert saved['rebased_state']['state'] == rebased
    assert {e['ref'] for e in saved['since_handoff']['entries']} == {refs['feedback']}
    feedback = saved['feedback_read']
    assert feedback['object']['kind'] == 'feedback'
    assert feedback['object']['text'] == authored['feedback']['memories'][0]['summary']
    assert {(e['rel'], e['to']) for e in feedback['links']['outgoing']} == {
        ('answers', refs['handoff']), ('confirms_selection', refs['decision'])}
    for edge in feedback['links']['outgoing']:
        source = next(e for e in authored['feedback']['memories'][0]['connect_to'] if e['rel'] == edge['rel'])
        for field in ('why', 'evidence', 'class'):
            assert edge[field] == source[field]
    entries = saved['final_records']['entries']
    assert len(entries) == 4 and {e['ref'] for e in entries} == set(refs.values())
    assert saved['final_memory']['resume_cursor']['ref'] == refs['feedback']
    assert saved['final_memory']['projection']['selection_omitted'] == 0
    assert saved['view_state']['state'] == rebased
    return ['fresh reader process recovers persisted records through its own returned refs',
            'same logical decision retry omits only the review token and keeps one stored entry',
            'constraint, decision, handoff turn and feedback preserve literal sources',
            'handoff and decision paths preserve relation direction, why and evidence',
            'human gesture changes clock, selection and revision without writing memory',
            'accepted-intent retry returns current state without restoring an old frame',
            'stale new intent conflicts and leaves the human frame intact',
            'rebased proof focus preserves the current clock, range, filters and layers',
            'explicit feedback confirms selection, not completion of the pending test',
            'observed temporal delta finds only feedback after the recovered handoff']
