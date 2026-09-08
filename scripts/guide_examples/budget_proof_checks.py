"""Audit actual partial pages without treating their success as completeness."""


def check(saved, client, authored):
    names = ('constraint', 'decision', 'test', 'result', 'unrelated')
    refs = {name: saved[name]['generated_refs'][0] for name in names}
    for name in names:
        source = authored[name]
        read = saved[name + '_read']
        assert read['object']['ref'] == refs[name]
        assert read['object']['kind'] == source['current']['kind']
        assert read['object']['text'] == source['current']['summary']
        assert any(e['text'] == source['current']['evidence'] for e in read['evidence'])

    boundary = saved['boundary']['entries']
    assert {e['ref'] for e in boundary} == {refs['constraint']}
    first, second = saved['temporal_first'], saved['temporal_second']
    assert first['page']['has_more'] and not second['page']['has_more']
    assert first['page']['next_cursor']
    assert authored['temporal_second']['from'] == {'ref': first['page']['next_cursor']}
    assert {k: v for k, v in authored['temporal_first'].items() if k != 'from'} == {
        k: v for k, v in authored['temporal_second'].items() if k != 'from'}
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
        expected = authored[name]['connect_to'][0]
        for field in ('why', 'evidence', 'class'):
            assert edge[field] == expected[field]
    for prior, current in zip(('first', 'second'), ('second', 'third')):
        old, new = authored['trace_' + prior], authored['trace_' + current]
        assert new['page']['cursor'] == saved['trace_' + prior]['page']['next_cursor']
        assert {k: v for k, v in old.items() if k != 'page'} == {
            k: v for k, v in new.items() if k != 'page'}
        assert new['page']['entries'] == old['page']['entries'] == 1

    partial, complete = saved['inspect_partial'], saved['inspect_complete']
    assert partial['page']['has_more'] and partial['page']['returned'] == 0
    assert partial['warnings'] and partial['page']['required_bytes'] > 512
    assert not complete['page']['has_more']
    assert partial['object'] == complete['object'] == saved['result_read']['object']
    assert complete['evidence'] and complete['links']['outgoing']
    assert authored['inspect_complete']['page']['cursor'] == partial['page']['next_cursor']
    assert authored['inspect_complete']['budget']['max_bytes'] == partial['page']['required_bytes']
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
    assert sum(a.get('question') is not None for a in authored.values()) == 2
    for name in ('packet', 'export_proof', 'unrelated_source'):
        assert not saved[name + '_frame']['unhonored']
    saved['pending_checkpoints'] = {
        'interval_after_first_page': {
            'status': 'partial', 'covered_refs': [e['ref'] for e in boundary + first['entries']],
            'next_call': {'tool': 'kmp_forward', 'arguments': authored['temporal_second']}},
        'inspection_at_512_bytes': {
            'status': 'partial', 'required_bytes': partial['page']['required_bytes'],
            'next_call': {'tool': 'kmp_inspect', 'arguments': authored['inspect_complete']}}}
    return ['five source records retain text, kinds and evidence',
            'inclusive boundary and two temporal pages recover the half-open interval',
            'opaque continuation cursors preserve all bound selection arguments',
            'three trace pages retain the directed relation proof exactly',
            'stable-floor inspection resumes without pretending omitted proof was read',
            'selection omission is distinguished from pending pagination',
            'an unrelated source has no manufactured semantic path',
            'two semantic UNKNOWN selections are terminal',
            'intermediate pending continuations are explicit in the result',
            'three shared frames preserve packet and proof context']
