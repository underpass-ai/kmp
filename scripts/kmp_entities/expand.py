"""Fetch complete, time-admissible identity proof along one actual graph hop."""
from formation.contracts import timestamp


def expand(plan, source_refs, as_of, call, max_calls=32, max_bytes=10000):
    """The frozen plan validates returned data; it never fills missing graph/text.

    Seeds are original refs already admitted by primary retrieval. Source->mention
    and mention->identity discovery uses returned edges. Each accepted group has
    both returned endpoints, the exact relation and every returned literal quote.
    All inspections share one budget, including pagination and source validation.
    Only the occurred/report axis is supported by this experimental integration.
    """
    if type(max_calls) is not int or max_calls < 1 or type(max_bytes) is not int or max_bytes < 1:
        raise ValueError('Positive inspection count and byte budgets required')
    cutoff = timestamp(as_of)
    payload = plan['ingest']
    about = payload['about']
    entries = {e['id']: e for e in payload['memory']['entries']}
    proofs = {e['id']: e for e in payload['memory']['evidence']}
    bindings = {frozenset((b['left'], b['right'])): b for b in plan['bindings']}
    relations = {(r['from'], r['to'], r['rel']): r for r in payload['memory']['relations']}
    cache, inspections, rejected, groups, seen = {}, [], [], [], set()

    def inspect(ref):
        if ref in cache:
            return cache[ref]
        if ref not in entries or not ref.startswith(about + ':'):
            raise ValueError('Unknown or cross-about graph reference')
        args = {'about': about, 'ref': ref, 'budget': {'max_bytes': max_bytes},
                'include': {'details': True, 'incoming': True, 'outgoing': True}}
        merged = {'object': None, 'evidence': {}, 'links': {'incoming': [], 'outgoing': []}}
        cursors = set()
        while len(inspections) < max_calls:
            page = call('kmp_inspect', args)
            inspections.append({'arguments': args, 'result': page})
            obj = page.get('object', {})
            expected = entries[ref]
            if (obj.get('ref') != ref or obj.get('text') != expected['text'] or
                    any(obj.get('metadata', {}).get(k) != v for k, v in expected.get('metadata', {}).items())):
                raise ValueError('Returned object does not match the frozen source/mention')
            if merged['object'] is not None and merged['object'] != obj:
                raise ValueError('Object changed during inspection pagination')
            merged['object'] = obj
            for evidence in page.get('evidence', []):
                eid = evidence['id']
                if eid in merged['evidence'] and merged['evidence'][eid] != evidence:
                    raise ValueError('Evidence changed during inspection pagination')
                merged['evidence'][eid] = evidence
            for direction in ('incoming', 'outgoing'):
                for link in page.get('links', {}).get(direction, []):
                    if link not in merged['links'][direction]:
                        merged['links'][direction].append(link)
            paging = page.get('page', {})
            if not paging.get('has_more'):
                cache[ref] = merged
                return merged
            cursor = paging.get('next_cursor')
            if not cursor or cursor in cursors:
                raise ValueError('Incomplete or stalled identity inspection')
            cursors.add(cursor)
            args = {**args, 'page': {'cursor': cursor}}
        raise ValueError('Identity inspection budget exhausted')

    def relation_ok(link):
        expected = relations.get((link.get('from'), link.get('to'), link.get('rel')))
        if expected is None or expected['rel'] not in ('derived_from', 'same_entity_as'):
            return False
        if any(link.get(k) != expected.get(k) for k in ('from', 'to', 'rel', 'class', 'why', 'evidence', 'method', 'confidence')):
            return False
        actual_time = link.get('coordinate', {}).get('occurred_at')
        return (actual_time == expected.get('coordinate', {}).get('occurred_at') and
                actual_time is not None and timestamp(actual_time) <= cutoff)

    def source(ref):
        result = inspect(ref)
        entry = entries[ref]
        if entry.get('metadata', {}).get('formation_record') != 'original_source':
            raise ValueError('Identity proof must refer to an original source')
        when = entry['coordinates'][0]['occurred_at']
        clocks = [r.get('coordinate', {}).get('occurred_at') for r in result['links']['incoming']
                  if r.get('rel') == 'contains_entry' and r.get('to') == ref]
        if when not in clocks or timestamp(when) > cutoff:
            raise ValueError('Original report clock missing or after cutoff')
        return result

    def mention(ref):
        result = inspect(ref)
        meta = result['object'].get('metadata', {})
        if meta.get('entity_record') != 'mention':
            raise ValueError('Identity endpoint must be a bounded mention')
        origin = source(meta['source_ref'])
        if not any(r.get('to') == meta['source_ref'] and r.get('rel') == 'derived_from' and relation_ok(r)
                   for r in result['links']['outgoing']):
            raise ValueError('Mention lacks an admissible returned source link')
        start, end = int(meta['source_start']), int(meta['source_end'])
        if not 0 <= start < end <= len(origin['object']['text']) or origin['object']['text'][start:end] != result['object']['text']:
            raise ValueError('Mention interval does not match returned original')
        return result

    for seed in dict.fromkeys(source_refs):
        try:
            origin = source(seed)
            adjacent = [r for r in origin['links']['incoming']
                        if r.get('rel') == 'derived_from' and r.get('to') == seed and relation_ok(r)]
            for edge in sorted(adjacent, key=lambda r: r['from']):
                ref = edge['from']
                if entries.get(ref, {}).get('metadata', {}).get('entity_record') != 'mention':
                    continue
                endpoint = mention(ref)
                links = endpoint['links']['outgoing'] + endpoint['links']['incoming']
                for link in links:
                    if link.get('rel') != 'same_entity_as':
                        continue
                    key = frozenset((link.get('from'), link.get('to')))
                    if key in seen:
                        continue
                    seen.add(key)
                    try:
                        binding = bindings.get(key)
                        if binding is None or ref not in key or not relation_ok(link):
                            raise ValueError('Unknown, changed or future identity relation')
                        left, right = mention(binding['left']), mention(binding['right'])
                        if left['object']['metadata']['entity_kind'] != right['object']['metadata']['entity_kind']:
                            raise ValueError('Identity cannot cross entity kinds')
                        returned = left['evidence'] | right['evidence']
                        quoted = []
                        for eid in binding['evidence_ids']:
                            proof, expected = returned.get(eid), proofs[eid]
                            if proof is None or any(proof.get(k) != expected.get(k) for k in ('id', 'source', 'text', 'time', 'supports')):
                                raise ValueError('Identity proof absent or changed in returned graph')
                            if timestamp(proof['time']) > cutoff:
                                raise ValueError('Identity proof is after cutoff')
                            original = source(proof['source'])
                            if proof['text'] not in original['object']['text']:
                                raise ValueError('Returned identity quote is not literal source text')
                            quoted.append(proof)
                        if not quoted:
                            raise ValueError('Identity has no returned proof')
                        groups.append({'binding_id': binding['id'], 'relation': link, 'quotes': quoted,
                                       'source_ref': seed, 'left': binding['left'], 'right': binding['right']})
                    except (KeyError, TypeError, ValueError) as error:
                        rejected.append({'identity_refs': sorted(x for x in key if isinstance(x, str)), 'reason': str(error)})
        except (KeyError, TypeError, ValueError) as error:
            rejected.append({'source_ref': seed, 'reason': str(error)})
    return {'groups': groups, 'inspections': inspections, 'inspection_calls': len(inspections),
            'max_calls': max_calls, 'budget_exhausted': len(inspections) >= max_calls,
            'rejected': rejected, 'axis': 'occurred', 'as_of': as_of,
            'limits': ['One identity hop from actually returned source seeds; no transitive closure.',
                       'Frozen registry validates returned data and never supplies missing text.',
                       'Declared identity and same-model verification can still be semantically wrong.']}
