"""Check source and relation preservation in the basic teaching cases."""


def check(saved, client, authored):
    expected = set()
    for name, args in authored.items():
        for index, current in enumerate(args.get('memories', [])):
            ref = saved[name]['local_refs'][current['id']]
            expected.add(ref)
            inspected = saved[name + ('_read' if index == 0 else '_delta_read')]
            assert inspected['object']['ref'] == ref
            assert inspected['object']['kind'] == current['kind']
            assert inspected['object']['text'] == current['summary']
            assert any(e['text'] == current['evidence'] for e in inspected['evidence'])
            links = inspected['links']['outgoing']
            for link in current.get('connect_to', []):
                target = saved[name]['local_refs'][link['ref'][1:]] if link['ref'].startswith('@') else link['ref']
                actual = next(e for e in links if e['to'] == target and e['rel'] == link['rel'])
                for field in ('class', 'why', 'evidence', 'confidence'):
                    if field in link:
                        assert actual[field] == link[field]
    records = saved['records']['entries']
    assert len(records) == len(expected) and {e['ref'] for e in records} == expected
    assert saved['view_state']['state']['selection'] == authored['frame']['selection']
    assert saved['view_state']['state']['clock'] == 'observed'
    assert saved['frame']['applied'] and not saved['frame']['unhonored']
    checks = ['every source keeps its declared kind, literal text and evidence',
              'every authored relation preserves target, class, rationale, evidence and confidence',
              'complete temporal enumeration contains exactly the intended memories',
              'the shared view selects the intended source on the observed clock']
    if 'answer' in saved:
        assert any(saved['decision']['generated_refs'][0] in e.get('supports', []) and e['text'] == authored['decision']['memories'][0]['summary'] for e in saved['answer']['proof']['evidence'])
        assert saved['packet_proof']['trace'][0]['rel'] == 'chosen_because'
        checks.append('the first answer cites the decision and its trace reaches the stored constraint')
    if 'new_policy' in saved:
        assert not saved['past']['proof']['superseded']
        assert saved['present']['proof']['superseded']
        assert saved['delta_proof']['trace'][0]['to'] == saved['old_policy']['generated_refs'][0]
        checks.append('the packet stores a distinct typed delta and names the inspected previous policy')
    if 'preview' in saved:
        assert saved['preview']['dry_run'] and not saved['preview']['accepted']
        assert saved['not_committed']['error']['code'] == 'not_found'
        packet = saved['preview']['ingest_preview']
        assert authored['ingested'] == {**packet, 'dry_run': False}
        assert authored['ingest_retry'] == authored['ingested']
        canonical = packet['memory']['entries'][0]
        assert saved['preview_read']['object']['ref'] == canonical['id']
        actual = next(e for e in records if e['ref'] == canonical['id'])
        for coordinate in canonical['coordinates']:
            for field in ('occurred_at', 'observed_at'):
                assert any(c.get(field) == coordinate[field] for c in actual['coordinates'])
        checks.append('canonical commit changes only dry_run; exact retry preserves one decision and its clocks')
    return checks
