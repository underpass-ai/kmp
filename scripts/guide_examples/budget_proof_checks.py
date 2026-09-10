from recall_action_checks import check as check_recall_actions
from short_continuation_checks import check as check_short_continuations

"""Audit actual partial pages without treating their success as completeness."""


def check(saved, client, authored):
    names = ('constraint', 'decision', 'test', 'result', 'unrelated')
    refs = {name: saved[name]['generated_refs'][0] for name in names}
    for name in names:
        source = authored[name]
        read = saved[name + '_read']
        assert read['object']['ref'] == refs[name]
        assert read['object']['kind'] == source['memories'][0]['kind']
        assert read['object']['text'] == source['memories'][0]['summary']
        assert any(e['text'] == source['memories'][0]['evidence'] for e in read['evidence'])

    boundary = saved['boundary']['entries']
    historical = saved['historical_context']
    supported = {ref for item in historical['proof']['evidence'] for ref in item['supports']}
    assert supported.intersection(refs.values()) == {refs['constraint']}
    source = authored['constraint']['memories'][0]
    assert {item['text'] for item in historical['proof']['evidence']} == {
        source['summary'], source['evidence']}
    assert 'proof' in historical['scope']['selection']
    assert 'wake.current_state' in historical['scope']['context']
    assert historical['scope']['context_time'] == 'unbounded'
    assert any(refs['decision'] in line for line in historical['wake']['current_state'])
    empty = saved['empty_history']
    assert not empty['proof']['evidence'] and empty['resume_cursor'] is None
    assert empty['proof']['abouts_empty_in_selection'] == [authored['empty_history']['about']]
    assert any(refs['unrelated'] in line for line in empty['wake']['current_state'])
    assert all(label['value'] != 'export' for label in empty['labels'])
    assert empty['scope']['selection'] == historical['scope']['selection']
    assert empty['scope']['context'] == historical['scope']['context']
    assert empty['scope']['dimensions']['selectors'] == authored['empty_history']['dimensions']['selectors']
    assert {e['ref'] for e in boundary} == {refs['constraint']}
    first, second = saved['temporal_first'], saved['temporal_second']
    assert not first['page']['has_more'] and not second['page']['has_more']
    assert first['selection']['has_more'] and not second['selection']['has_more']
    assert authored['temporal_second'] == first['next_actions'][0]['arguments']
    assert first['next_actions'][0]['tool'] == 'kmp_forward'
    assert {e['ref'] for e in first['entries']} == {refs['decision'], refs['test']}
    assert {e['ref'] for e in second['entries']} == {refs['result'], refs['unrelated']}
    entries = boundary + first['entries'] + second['entries']
    assert len({e['ref'] for e in entries}) == 5
    # The selected packet lane has one observed coordinate per entry.
    retained = {e['ref'] for e in entries if any(
        '2026-09-01T08:00:00Z' <= c['observed_at'] < '2026-09-01T12:00:00Z'
        for c in e['coordinates'])}
    assert retained == {refs[n] for n in names if n != 'unrelated'}

    pages = [saved['trace_' + n] for n in ('first', 'second', 'third')]
    assert [p['page']['has_more'] for p in pages] == [True, True, False]
    assert all(p['page']['returned'] == 1 and p['page']['total'] == 3 for p in pages)
    assert pages[0]['page']['next_cursor'] != pages[1]['page']['next_cursor']
    edges = [edge for page in pages for edge in page['trace']]
    assert [(e['from'], e['to'], e['rel']) for e in edges] == [
        (refs['result'], refs['test'], 'verified_by'),
        (refs['test'], refs['decision'], 'uses_background'),
        (refs['decision'], refs['constraint'], 'chosen_because')]
    for name, edge in zip(('result', 'test', 'decision'), edges):
        expected = authored[name]['memories'][0]['connect_to'][0]
        for field in ('why', 'evidence', 'class'):
            assert edge[field] == expected[field]
    for prior, current in zip(('first', 'second'), ('second', 'third')):
        old, new = authored['trace_' + prior], authored['trace_' + current]
        assert new == saved['trace_' + prior]['next_actions'][0]['arguments']
        assert new['page']['cursor'] == saved['trace_' + prior]['page']['next_cursor']
        assert {k: v for k, v in old.items() if k != 'page'} == {
            k: v for k, v in new.items() if k != 'page'}
        assert new['page']['entries'] == old['page']['entries'] == 1

    partial, complete = saved['inspect_partial'], saved['inspect_complete']
    assert partial['page']['has_more'] and partial['page']['returned'] == 0
    assert partial['warnings'] and partial['page']['required_bytes'] > 512
    assert not complete['page']['has_more']
    assert partial['object'] == saved['result_read']['object']
    assert complete['object_reused'] and complete['object'] == {'ref': partial['object']['ref']}
    assert complete['evidence'] and complete['links']['outgoing']
    assert authored['inspect_complete']['page']['cursor'] == partial['page']['next_cursor']
    assert authored['inspect_complete']['budget']['max_bytes'] == partial['page']['required_bytes']
    # Execute the server's calls unchanged as a separate path through the same
    # stored source. The authored two-call alternative above exercises reuse.
    paths = (('evidence',), ('links', 'incoming'), ('links', 'outgoing'), ('raw',))
    def section(value, path):
        for key in path:
            value = value[key]
        return value
    whole_arguments = dict(authored['inspect_partial'], budget={'max_bytes': 100000})
    whole = client.call('kmp_inspect', whole_arguments)
    assert not whole['page']['has_more']
    accumulated = {path: list(section(partial, path)) for path in paths}
    page, pages = partial, [partial]
    while page['page']['has_more']:
        assert len(pages) < 100
        action = page['next_actions'][0]
        assert action['tool'] == 'kmp_inspect'
        assert action['arguments']['about'] == authored['inspect_partial']['about']
        assert action['arguments']['ref'] == authored['inspect_partial']['ref']
        stalled = page['page']['returned'] == 0
        page = client.call(action['tool'], action['arguments'])
        if stalled:
            assert page['page']['returned'] > 0
        assert page['object'] == whole['object']
        for path in paths:
            accumulated[path].extend(section(page, path))
        pages.append(page)
    assert not page['next_actions']
    for path in paths:
        assert accumulated[path] == section(whole, path)
    saved['inspection_action_walk'] = {
        'pages': len(pages), 'additional_native_calls': len(pages),
        'complete_expansion_equal': True,
        'full_required_bytes': whole['page']['required_bytes']}
    capped = saved['capped_recall']['projection']
    available = saved['catalogue']['projection']['sections']['proof.evidence']['total']
    assert capped['selection_omitted'] == available - 1 > 0
    assert not capped['page']['has_more']
    assert saved['unrelated_read']['links']['outgoing'] == []
    assert not saved['no_path']['trace']
    for name in ('unknown_english', 'unknown_original'):
        assert saved[name]['answer'] == 'UNKNOWN'
        assert saved[name]['proof']['missing']
    assert list(authored)[-2:] == ['unknown_english', 'unknown_original']
    assert sum(a.get('question') is not None for a in authored.values()) == 3
    saved['recall_action_walks'] = check_recall_actions(saved, client, authored)
    saved['short_inspection_walk'] = check_short_continuations(saved, client, authored)
    for name in ('packet', 'export_proof', 'unrelated_source'):
        assert not saved[name + '_frame']['unhonored']
    saved['pending_checkpoints'] = {
        'interval_after_first_page': {
            'status': 'partial', 'covered_refs': [e['ref'] for e in boundary + first['entries']],
            'next_call': {'tool': 'kmp_forward', 'arguments': authored['temporal_second']}},
        'inspection_at_512_bytes': {
            'status': 'partial', 'required_bytes': partial['page']['required_bytes'],
            'minimum_progress_bytes': partial['page']['minimum_progress_bytes'],
            'next_call': partial['next_actions'][0]}}
    return ['Ask and Wake execute returned calls, restore shortened core and reconstruct all selected sections',
            'five source records retain text, kinds and evidence',
            'historical proof and dimensionally filtered about context declare their distinct scope',
            'inclusive boundary and two temporal pages recover the half-open interval',
            'opaque continuation cursors preserve all bound selection arguments',
            'three trace pages retain the directed relation proof exactly',
            'stable-floor inspection resumes through returned calls and reconstructs the complete selected proof',
            'selection omission is distinguished from pending pagination',
            'an unrelated source has no manufactured semantic path',
            'two semantic UNKNOWN selections are terminal',
            'intermediate pending continuations are explicit in the result',
            'three shared frames preserve packet and proof context']
