"""Compile verified proposals and unchanged source records to one canonical ingest."""
from contracts import VERSION, candidates, digest, episode, string, timestamp, verdicts


def compile_plan(source_episode, extracted, verified, model_revision, observed_at):
    episode(source_episode)
    string(model_revision, 'model_revision', 256)
    written = timestamp(observed_at)
    if any(timestamp(s['observed_at']) > written for s in source_episode['sources']):
        raise ValueError('a source was observed after this formation attempt')
    proposed, invalid = candidates(extracted, source_episode)
    accepted, unsupported = verdicts(verified, proposed)
    about = source_episode['about']
    source_hash = digest(source_episode)
    scope = 'formation-' + source_hash[:24]
    sources = {s['id']: s for s in source_episode['sources']}
    refs = {sid: about + ':source:' + digest([source_hash, sid]) for sid in sources}
    entries, relations, evidence = [], [], []
    for index, source in enumerate(sources.values(), 1):
        entries.append({'id': refs[source['id']], 'kind': 'observation', 'text': source['text'],
            'metadata': {'source_id': source['id'], 'source_role': source['role'],
                         'source_kind': 'human' if source['role'] == 'user' else 'agent',
                         'formation_record': 'original_source', 'source_sha256': digest(source)},
            'coordinates': [{'dimension': 'conversation', 'scope_id': scope, 'sequence': index,
                'occurred_at': source['observed_at'], 'observed_at': source['observed_at']}]})
    for index, memory in enumerate(accepted, len(entries) + 1):
        ref = about + ':memory:' + digest([source_hash, VERSION, model_revision, memory['id']])
        reported_at = max((sources[c['source_id']]['observed_at'] for c in memory['citations']), key=timestamp)
        entries.append({'id': ref, 'kind': {'decision': 'decision', 'constraint': 'constraint'}.get(
                memory['category'], 'observation'), 'text': memory['text'],
            'metadata': {'source_kind': 'derived', 'formation_record': 'model_memory',
                         'formation_category': memory['category'], 'formation_version': VERSION,
                         'formation_model_revision': model_revision,
                         'formation_verification': 'same_model_supported_not_independent',
                         'formation_source_sha256': source_hash,
                         'occurred_at_basis': 'latest_cited_source_report_not_inferred_event'},
            'coordinates': [{'dimension': 'conversation', 'scope_id': scope, 'sequence': index,
                             'occurred_at': reported_at, 'observed_at': observed_at}]})
        derivations = {}
        for ci, citation in enumerate(memory['citations']):
            source_ref = refs[citation['source_id']]
            derivations.setdefault(source_ref, []).append(citation)
            evidence.append({'id': 'evidence:' + ref + ':' + str(ci), 'supports': [ref],
                'text': citation['quote'], 'source': source_ref,
                'metadata': {'formation_record': 'literal_quote',
                             'source_id': citation['source_id']}})
        for source_ref, citations in derivations.items():
            relations.append({'from': ref, 'to': source_ref, 'rel': 'derived_from', 'class': 'evidential',
                'why': '\n'.join(c['why'] for c in citations),
                'evidence': '\n'.join(c['quote'] for c in citations), 'confidence': 'medium'})
    payload = {'about': about, 'idempotency_key': 'formation:' + digest([source_hash, VERSION, model_revision]),
        'provenance': {'source_kind': 'derived', 'source_agent': 'agent:kmp-local-formation',
                       'observed_at': observed_at},
        'memory': {'dimensions': [{'id': scope, 'kind': 'conversation'}], 'entries': entries,
                   'relations': relations, 'evidence': evidence}}
    return {'version': VERSION, 'source_sha256': source_hash, 'model_revision': model_revision,
            'observed_at': observed_at, 'accepted_count': len(accepted),
            'rejected': invalid + unsupported, 'ingest': payload,
            'limits': ['Model verification is not independent or a guarantee of entailment.',
                       'Replay this exact saved ingest; do not regenerate a retry.',
                       'No entity merging, inferred event timestamps or supersession.']}
