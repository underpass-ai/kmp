"""Check distinct clocks and an evidence-preserving zoom in the native example."""
from datetime import datetime, timedelta, timezone


def clock_bindings():
    # Only this synthetic lesson moves with the day of execution. Ingestion
    # remains the kernel's real commit clock; the writer cannot set it.
    today = datetime.now(timezone.utc).replace(hour=0, minute=0, second=0, microsecond=0)
    day1 = today - timedelta(days=3)
    moments = {
        'day1': day1, 'occurred': day1 + timedelta(hours=9),
        'checked': day1 + timedelta(hours=9, minutes=15),
        'day2': day1 + timedelta(days=1),
        'day3': day1 + timedelta(days=2),
        'observed': day1 + timedelta(days=2, hours=14),
        'check_observed': day1 + timedelta(days=2, hours=14, minutes=15),
        'receipt_observed': day1 + timedelta(days=2, hours=14, minutes=30),
        'day4': today, 'day5': today + timedelta(days=1),
    }
    return {key: value.isoformat().replace('+00:00', 'Z') for key, value in moments.items()}


def check(saved, client, authored):
    clock = saved['clock']
    refs = {name: saved[name]['generated_refs'][0] for name in ('permit', 'signature', 'receipt')}
    assert len(set(refs.values())) == 3
    for name, kind in [('permit', 'decision'), ('signature', 'observation'), ('receipt', 'feedback')]:
        read = saved[name + '_read']
        assert saved[name]['accepted']
        assert read['object']['ref'] == refs[name] and read['object']['kind'] == kind
        assert read['object']['text'] == authored[name]['memories'][0]['summary']
        assert any(e['text'] == authored[name]['memories'][0]['evidence'] for e in read['evidence'])
        coords = [link['coordinate'] for link in read['links']['incoming'] if 'coordinate' in link]
        assert coords
        for coord in coords:
            assert coord['occurred_at'] == authored[name]['occurred_at']
            assert coord['observed_at'] == authored[name]['observed_at']
            assert clock['day4'] <= coord['ingested_at'] < clock['day5'], 'Run crossed UTC midnight; record a new run'
            assert 'ingested_at' not in authored[name]
            if name == 'permit':
                assert coord['valid_from'] == clock['day2'] and coord['valid_until'] == clock['day5']

    def entries(name):
        return {entry['ref'] for entry in saved[name]['entries']}

    assert entries('occurred_start') == {refs['permit']}
    interval = {refs['permit'], refs['signature']}
    assert entries('occurred_interval') == interval
    assert saved['occurred_interval']['temporal']['interval'] == {
        'start': clock['occurred'], 'end': clock['day2']}
    assert entries('before_end') == interval
    browse, full = saved['coordinate_browse'], saved['occurred_interval']
    assert browse['selection']['fields'] == {
        'included': ['ref', 'kind', 'coordinates'], 'omitted': ['text', 'metadata']}
    assert browse['proof'] == full['proof']
    assert browse['raw_refs'] == full['raw_refs']
    assert len(browse['entries']) == len(full['entries'])
    for reduced, original in zip(browse['entries'], full['entries']):
        assert {k: reduced[k] for k in ('ref', 'kind', 'coordinates')} == {
            k: original[k] for k in ('ref', 'kind', 'coordinates')}
        assert 'text' not in reduced and 'metadata' not in reduced
    assert authored['coordinate_detail'] == browse['entries'][0]['detail_action']['arguments']
    assert saved['coordinate_detail']['entries'] == [full['entries'][0]]
    assert entries('observed_start') == {refs['permit']}
    assert entries('observed_later') == {refs['signature'], refs['receipt']}
    assert entries('ingested_day4') == set(refs.values())
    for axis, expected in [('occurred', {refs['permit']}), ('observed', set()),
                           ('ingested', set()), ('validity', {refs['permit']})]:
        name = 'day2_' + axis
        assert saved[name]['temporal']['axis'] == axis
        assert entries(name) == expected
    assert not entries('before_valid') and not entries('expired')
    assert entries('ingested_exact') == {refs['permit']}
    assert 'ingested_at' in saved['forged_ingestion']['error']['message']
    assert entries('after_refusal') == set(refs.values())
    edge = next(edge for edge in saved['signature_path']['trace'] if edge['rel'] == 'supports')
    assert (edge['from'], edge['to']) == (refs['signature'], refs['permit'])
    assert edge['why'] == authored['signature']['memories'][0]['connect_to'][0]['why']
    assert edge['evidence'] == authored['signature']['memories'][0]['connect_to'][0]['evidence']
    assert saved['occurred_after_view']['entries'] == saved['occurred_start']['entries']
    assert saved['permit_after_view']['object'] == saved['permit_read']['object']
    stages = [('overview', 'occurred', 'atlas', 'day1', 'day5'),
              ('zoom', 'occurred', 'episode', 'occurred', 'day2'),
              ('detail', 'occurred', 'moment', 'occurred', 'day2'),
              ('observed_empty', 'observed', 'moment', 'occurred', 'day2'),
              ('observed', 'observed', 'moment', 'observed', 'day4'),
              ('ingested', 'ingested', 'moment', 'day4', 'day5'),
              ('validity', 'validity', 'moment', 'day2', 'day3')]
    for name, axis, lod, start, end in stages:
        view = saved[name + '_state']['state']
        assert not saved[name + '_frame']['unhonored']
        assert view['clock'] == axis and view['projection']['semantic_zoom'] == lod
        assert view['focus']['time_range'] == {'from': clock[start], 'to': clock[end]}
    assert saved['view_state']['state']['selection'] == refs['permit']
    return ['three typed source-backed memories', 'actual kernel ingestion on day four',
            'four distinct answers at the same day-two instant', 'exclusive validity expiry',
            'direct interval includes the start and excludes the end in both directions',
            'observation and ingestion timelines retain every source', 'writer rejects forged ingestion',
            'selected coordinates expand to the complete entry with unchanged scope',
            'signature proof survives trace', 'seven explicit viewer clock and zoom stages',
            'view changes neither explicit MCP selection nor stored object']
