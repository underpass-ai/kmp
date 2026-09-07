"""Compile verified mention equivalences while preserving every base record."""
import copy

from formation.contracts import digest, string, timestamp
from .boundary import VERSION, candidates, context, verdicts


def compile_plan(source_context, base, extracted, verified, model_revision, observed_at):
    context(source_context)
    string(model_revision, 'model_revision', 256)
    if timestamp(observed_at) < timestamp(source_context['as_of']):
        raise ValueError('Resolution precedes its context cutoff')
    if base['about'] != source_context['about']:
        raise ValueError('Entity and source about boundaries differ')
    records = base['memory']['entries']
    if any(e.get('metadata', {}).get('entity_record') for e in records):
        raise ValueError('Compile from the frozen base, not a previous entity augmentation')
    originals = [e for e in records if e.get('metadata', {}).get('formation_record') == 'original_source']
    by_source = {e['metadata']['source_id']: e for e in originals}
    sources = {s['id']: s for s in source_context['sources']}
    if len(by_source) != len(originals) or by_source.keys() != sources.keys():
        raise ValueError('Exactly the same original source registry is required')
    for sid, source in sources.items():
        entry = by_source[sid]
        if not entry['id'].startswith(base['about'] + ':') or entry['text'] != source['text']:
            raise ValueError('Original source identity/text mismatch')
        if entry['metadata']['source_role'] != source['role'] or len(entry['coordinates']) != 1:
            raise ValueError('Original source role or coordinate mismatch')
        position = entry['coordinates'][0]
        if any(timestamp(position[key]) != timestamp(source['observed_at']) for key in ('occurred_at', 'observed_at')):
            raise ValueError('Original source report clock mismatch')
    proposed, invalid = candidates(extracted, source_context)
    accepted, rejected = verdicts(verified, proposed)
    payload = copy.deepcopy(base)
    payload['idempotency_key'] = 'entity-resolution:' + digest([source_context, base, VERSION, model_revision])
    payload['provenance'] = {'source_kind': 'derived', 'source_agent': 'agent:kmp-entity-resolution',
                             'observed_at': observed_at}
    memory = payload['memory']
    scope = 'entity-mentions-' + digest([base['about'], source_context['context_id']])[:24]
    if any(d['id'] == scope for d in memory['dimensions']):
        raise ValueError('Entity dimension is already present')
    mentions, bindings = {}, []

    def coordinate(when, sequence):
        return {'dimension': 'entity_mention', 'scope_id': scope, 'sequence': sequence,
                'occurred_at': when, 'observed_at': observed_at}

    def node(endpoint, kind):
        source = sources[endpoint['source_id']]
        source_ref = by_source[source['id']]['id']
        ref = base['about'] + ':entity-mention:' + digest([source_ref, endpoint['start'], endpoint['end']])
        if ref in mentions:
            if mentions[ref]['metadata']['entity_kind'] != kind:
                raise ValueError('One source occurrence received incompatible entity kinds')
            return ref
        entry = {'id': ref, 'kind': 'observation', 'text': endpoint['text'],
            'metadata': {'source_kind': 'derived', 'entity_record': 'mention', 'entity_version': VERSION,
                'entity_kind': kind, 'source_ref': source_ref, 'source_id': source['id'],
                'source_role': source['role'], 'source_start': str(endpoint['start']), 'source_end': str(endpoint['end']),
                'source_sha256': digest(source), 'entity_model_revision': model_revision,
                'entity_verification': 'same_model_not_independent'},
            'coordinates': [coordinate(source['observed_at'], len(mentions) + 1)]}
        mentions[ref] = entry
        memory['relations'].append({'from': ref, 'to': source_ref, 'rel': 'derived_from', 'class': 'evidential',
            'why': 'This bounded mention occupies the recorded literal source character interval; this is not identity between statements.',
            'evidence': endpoint['text'], 'confidence': 'medium', 'method': VERSION,
            'coordinate': coordinate(source['observed_at'], len(mentions))})
        memory['evidence'].append({'id': 'evidence:' + ref + ':mention', 'supports': [ref],
            'source': source_ref, 'text': endpoint['text'], 'time': source['observed_at'],
            'metadata': {'entity_record': 'mention_quote', 'source_id': source['id']}})
        return ref

    for proposal in accepted:
        left = node(proposal['left'], proposal['kind'])
        right = node(proposal['right'], proposal['kind'])
        # Relation time records when all of its source proof was available;
        # an earlier source mention does not backdate a later identity claim.
        when = max((sources[c['source_id']]['observed_at'] for c in proposal['citations']), key=timestamp)
        relation = {'from': left, 'to': right, 'rel': 'same_entity_as', 'class': 'evidential',
            'why': proposal['why'], 'evidence': '\n'.join(c['quote'] for c in proposal['citations']),
            'confidence': 'medium', 'method': VERSION,
            'coordinate': coordinate(when, len(bindings) + 1)}
        memory['relations'].append(relation)
        proof_ids = []
        for index, citation in enumerate(proposal['citations']):
            eid = 'evidence:' + base['about'] + ':entity-identity:' + proposal['id'] + ':' + str(index)
            proof_ids.append(eid)
            memory['evidence'].append({'id': eid, 'supports': [left, right],
                'source': by_source[citation['source_id']]['id'], 'text': citation['quote'],
                'time': sources[citation['source_id']]['observed_at'],
                'metadata': {'entity_record': 'identity_quote', 'entity_proposal_id': proposal['id'],
                    'source_id': citation['source_id'], 'entity_basis': proposal['basis']}})
        bindings.append({'id': proposal['id'], 'left': left, 'right': right,
                         'available_at': when, 'evidence_ids': proof_ids, 'basis': proposal['basis']})
    if mentions:
        memory['dimensions'].append({'id': scope, 'kind': 'entity_mention'})
        memory['entries'].extend(mentions.values())
    assert memory['entries'][:len(records)] == records
    return {'version': VERSION, 'source_context_sha256': digest(source_context),
        'base_sha256': digest(base), 'model_revision': model_revision, 'observed_at': observed_at,
        'accepted_count': len(accepted), 'mention_count': len(mentions),
        'rejected': invalid + rejected, 'bindings': bindings, 'ingest': payload,
        'limits': ['No source or formed memory is merged, replaced, rewritten or deleted.',
                   'Equivalence endpoints are source mentions, not whole turns or claims.',
                   'Relation clocks follow the latest cited source report, not inferred real-world identity start.',
                   'Use entity_mention labels to separate mention lookup from primary claim retrieval.',
                   'Same-model verification and literal quotes do not guarantee correct identity.',
                   'No automatic transitive closure, cross-about identity or consolidation.']}
