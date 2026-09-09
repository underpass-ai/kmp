"""Check retrieved operands and their proof; this is not an automatic writer."""
from datetime import datetime, timedelta, timezone
from decimal import Decimal
import re


def clock_bindings():
    now = datetime.now(timezone.utc)
    if now < datetime(2026, 9, 4, 9, tzinfo=timezone.utc):
        raise ValueError('The fictional sources are not yet in the past')
    end = now.replace(hour=0, minute=0, second=0, microsecond=0) + timedelta(days=1)
    return {'calculated': now.isoformat().replace('+00:00', 'Z'),
            'until': end.isoformat().replace('+00:00', 'Z')}


def check(saved, client, authored):
    kinds = {'policy': 'constraint', 'train': 'observation', 'hotel_old': 'observation',
             'meal': 'observation', 'usd': 'observation', 'hotel_final': 'observation',
             'hotel_copy': 'observation', 'void': 'feedback', 'statement': 'observation',
             'total': 'derived_value', 'exclusions': 'decision'}
    refs = {name: saved[name]['generated_refs'][0] for name in kinds}
    assert len(set(refs.values())) == 11
    for name, kind in kinds.items():
        read = saved[name + '_read']
        assert saved[name]['accepted']
        assert read['object']['kind'] == kind and read['object']['ref'] == refs[name]
        assert read['object']['text'] == authored[name]['memories'][0]['summary']
        assert any(e['text'] == authored[name]['memories'][0]['evidence'] for e in read['evidence'])
        for link in read['links']['incoming']:
            if 'coordinate' in link:
                assert link['coordinate']['observed_at'] == authored[name]['observed_at']

    def entries(name):
        return {entry['ref'] for entry in saved[name]['entries']}

    early = {refs[n] for n in ('train', 'hotel_old', 'meal')}
    assert entries('before_corrections') == entries('before_corrections_again') == early
    assert entries('eur_candidates') == {refs[n] for n in
                                        ('train', 'hotel_old', 'meal', 'hotel_final', 'hotel_copy', 'void')}
    assert entries('usd_candidates') == {refs['usd']}
    for name in ('eur_candidates', 'usd_candidates', 'before_corrections_again'):
        for entry in saved[name]['entries']:
            assert all(c['dimension'] == 'trip' for c in entry['coordinates'])

    # Extract the two chosen operands from native inspection, not fixture totals.
    train = re.search(r'settled transport charge ([0-9.]+) (EUR)', saved['train_operand']['object']['text'])
    hotel = re.search(r'settled hotel amount is ([0-9.]+) (EUR)', saved['hotel_final_operand']['object']['text'])
    result = re.search(r'subtotal is ([0-9.]+) (EUR)', saved['total_read']['object']['text'])
    assert train and hotel and result
    assert train[2] == hotel[2] == result[2]
    assert Decimal(train[1]) + Decimal(hotel[1]) == Decimal(result[1]) == Decimal('55.00')
    assert '7.00 USD' in saved['usd_read']['object']['text']
    assert 'No currency conversion rate' in saved['statement_operand']['object']['text']
    total_of = [e for e in saved['total_read']['links']['outgoing'] if e['rel'] == 'total_of']
    assert {e['to'] for e in total_of} == {refs['train'], refs['hotel_final']}
    assert all(e['class'] == 'evidential' and e['why'] and e['evidence'] for e in total_of)
    assert saved['hotel_old_after']['object'] == saved['hotel_old_read']['object']
    for link in saved['hotel_old_read']['links']['incoming']:
        if 'coordinate' in link:
            current = next(x for x in saved['hotel_old_after']['links']['incoming'] if x['from'] == link['from'])
            assert current['coordinate'] == link['coordinate']

    for path, source, target, rel in [
            ('correction_path', 'hotel_final', 'hotel_old', 'corrects'),
            ('duplicate_path', 'hotel_copy', 'hotel_final', 'same_event_as'),
            ('void_path', 'void', 'meal', 'updates_state'),
            ('total_train_path', 'total', 'train', 'total_of'),
            ('total_hotel_path', 'total', 'hotel_final', 'total_of'),
            ('exclusion_path', 'exclusions', 'total', 'excluded_from'),
            ('exclusion_void_path', 'exclusions', 'void', 'chosen_because')]:
        expected = next(e for e in authored[source]['memories'][0]['connect_to'] if e['ref'] == refs[target] and e['rel'] == rel)
        actual = next(e for e in saved[path]['trace'] if e['from'] == refs[source] and e['to'] == refs[target] and e['rel'] == rel)
        for field in ('class', 'why', 'evidence'):
            assert actual[field] == expected[field]
    assert authored['exclusions']['memories'][0]['connect_to'][0]['class'] == 'constraint'
    for name in ('total', 'exclusions'):
        assert authored[name]['observed_at'] == saved['clock']['calculated']
        assert authored[name]['source_kind'] == 'derived'
    for name in ('early', 'sources', 'settlement'):
        assert not saved[name + '_frame']['unhonored']
        state = saved[name + '_state']['state']
        assert state['clock'] == 'observed' and state['projection']['semantic_zoom'] == 'moment'
    assert saved['early_state']['state']['focus']['time_range']['to'] == '2026-09-02T00:00:00Z'
    assert saved['view_state']['state']['selection'] == refs['total']
    return ['eleven source-backed memories with five justified kinds',
            'earlier observed slice excludes later knowledge',
            'six EUR documents are candidates, not six expenses',
            'currency selectors retain only the requested trip lane',
            '55.00 EUR recomputed from retrieved decimal operands',
            'two included operands; USD, old invoice, duplicate and void are not added',
            'original invoice and clocks remain unchanged',
            'seven directed proof paths preserve class, rationale and evidence',
            'calculation and exclusion audit use the actual current clock',
            'three explicit shared-view stages']
