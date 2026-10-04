"""Check that an open-ended, a replaced and an expired decision stay distinct."""


def refs(items):
    return {item['ref'] for item in items}


def check(saved, client, authored):
    local = saved['recorded']['local_refs']
    d1, r1, l1 = local['d1'], local['r1'], local['l1']
    r2 = saved['replacement']['local_refs']['r2']
    assert saved['replacement_review']['status'] == 'needs_review'
    assert not saved['replacement_review']['accepted']

    present = saved['present']['proof']
    assert [(s['ref'], s['superseded_by']) for s in present['superseded']] == [(r1, r2)]
    supersedes = authored['replacement']['memories'][0]['connect_to'][0]
    assert present['superseded'][0]['why'] == supersedes['why']
    assert refs(present['expired']) == {l1}
    assert present['expired'][0]['valid_until'] == '2026-09-30T00:00:00Z'
    assert not present['conflicts']
    assert d1 not in refs(present['superseded']) | refs(present['expired'])

    latest = {entry['ref']: entry for entry in saved['latest']['entries']}
    assert set(latest) == {d1, r1, l1, r2}
    observed = {c['observed_at'] for c in latest[d1]['coordinates']}
    ingested = {c['ingested_at'] for c in latest[d1]['coordinates']}
    assert observed == {'2026-08-23T10:00:00Z'}
    assert ingested and observed.isdisjoint(ingested)

    before = saved['before_replacement']
    assert r2 not in refs(before['entries']) and r1 in refs(before['entries'])
    assert not before['proof']['superseded']

    after = saved['after_end']
    assert refs(after['proof']['expired']) == {l1}
    assert l1 not in refs(after['entries'])
    assert not after['proof']['superseded'] or d1 not in refs(after['proof']['superseded'])

    answer = saved['answer']
    assert answer['answer_status'] == 'answered'
    cited = {item['claim'] for item in answer['because']}
    assert d1 in cited
    assert d1 not in refs(answer['proof']['superseded']) | refs(answer['proof']['expired'])
    return ['the replaced decision is marked superseded with its own why and its replacement',
            'the ended decision is reported expired at its exclusive valid_until, not superseded',
            'the open-ended decision carries no lifecycle marker and keeps its own observation time',
            'ingestion time is distinct from observation time for the open-ended decision',
            'a read before the replacement neither shows the replacement nor marks the old decision',
            'Ask cites the open-ended decision without a lifecycle marker']
