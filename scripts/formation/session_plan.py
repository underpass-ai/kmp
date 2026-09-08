"""Compile paired session ingests with identical originals and explicit failures."""
import copy

from contracts import VERSION, digest, fields, string, timestamp
from planner import compile_plan
from segment import segment_session


def compile_session(session, attempts, **policy):
    """Require one terminal outcome per episode; never generate or write a store."""
    segmented = segment_session(session, **policy)
    episodes = {r['episode']['episode_id']: r['episode'] for r in segmented['episodes']}
    if not isinstance(attempts, list) or len(attempts) != len(episodes):
        raise ValueError('one completed or failed attempt is required per planned episode')
    pending, plans, results, revisions = set(episodes), [], [], set()
    clocks = []
    for attempt in attempts:
        if not isinstance(attempt, dict) or attempt.get('status') not in ('completed', 'failed'):
            raise ValueError('a terminal attempt status is required')
        expected = ('episode_id', 'status', 'observed_at', 'model_revision') + (
            ('extracted', 'verified', 'version') if attempt['status'] == 'completed' else ('error',))
        fields(attempt, expected, 'attempt')
        eid = attempt['episode_id']
        if eid not in pending:
            raise ValueError('unknown or duplicate attempted episode')
        pending.remove(eid)
        revision = string(attempt['model_revision'], 'model_revision', 256)
        revisions.add(revision)
        clock = timestamp(attempt['observed_at'])
        if any(timestamp(s['observed_at']) > clock for s in episodes[eid]['sources']):
            raise ValueError('source reported after attempt')
        clocks.append((clock, attempt['observed_at']))
        if attempt['status'] == 'failed':
            string(attempt['error'], 'attempt.error', 4000)
            results.append({'episode_id': eid, 'status': 'failed', 'error': attempt['error'],
                            'accepted_count': 0, 'rejected_count': None})
            continue
        if attempt['version'] != VERSION:
            raise ValueError('compile with the exact writer version used for the attempt')
        # Revalidate proposals and verdicts; do not trust a caller-supplied
        # ingest, relation list or original-source mapping.
        plan = compile_plan(episodes[eid], attempt['extracted'], attempt['verified'],
                            revision, attempt['observed_at'])
        plans.append((eid, plan))
        results.append({'episode_id': eid, 'status': 'completed', 'accepted_count': plan['accepted_count'],
                        'rejected_count': len(plan['rejected']), 'rejected': plan['rejected']})
    source_hash = digest(session)
    about, scope = session['about'], 'formation-session-' + source_hash[:24]
    nonblank = [s for s in session['sources'] if s['text'].strip()]
    refs = {s['id']: about + ':source:' + digest([source_hash, s['id']]) for s in nonblank}
    entries = [{'id': refs[s['id']], 'kind': 'observation', 'text': s['text'],
        'metadata': {'source_id': s['id'], 'source_role': s['role'],
            'source_kind': 'human' if s['role'] == 'user' else 'agent',
            'formation_record': 'original_source', 'source_sha256': digest(s)},
        'coordinates': [{'dimension': 'conversation', 'scope_id': scope, 'sequence': i,
            'occurred_at': s['observed_at'], 'observed_at': s['observed_at']}]} for i, s in enumerate(nonblank, 1)]
    summary = {'version': VERSION, 'session_sha256': source_hash, 'segmentation_policy': segmented['policy'],
        'results': results, 'input_sources': len(session['sources']), 'stored_original_sources': len(entries),
        'excluded_blank_source_ids': segmented['excluded_blank_source_ids'],
        'failed_episodes': sum(r['status'] == 'failed' for r in results)}
    if not nonblank:
        return {**summary, 'baseline': None, 'formed': None, 'status': 'empty_source_session'}
    baseline = {'about': about, 'idempotency_key': 'formation-source-session:' + source_hash,
        'provenance': {'source_kind': 'derived', 'source_agent': 'agent:kmp-local-formation',
                       'observed_at': nonblank[0]['observed_at']},
        'memory': {'dimensions': [{'id': scope, 'kind': 'conversation'}],
                   'entries': entries, 'relations': [], 'evidence': []}}
    formed = copy.deepcopy(baseline)
    formed['idempotency_key'] = 'formation-session:' + digest([source_hash, segmented['policy'], VERSION, sorted(revisions)])
    formed['provenance']['observed_at'] = max(clocks)[1]
    # Assemble in planned episode order, independent of job completion order.
    plan_by_id = dict(plans)
    for eid in episodes:
        if eid not in plan_by_id:
            continue
        memory = plan_by_id[eid]['ingest']['memory']
        translated = {e['id']: refs[e['metadata']['source_id']] for e in memory['entries']
                      if e['metadata']['formation_record'] == 'original_source'}
        for entry in memory['entries']:
            if entry['metadata']['formation_record'] != 'model_memory':
                continue
            entry = copy.deepcopy(entry)
            entry['metadata']['formation_episode_id'] = eid
            entry['coordinates'][0].update(scope_id=scope, sequence=len(formed['memory']['entries']) + 1)
            formed['memory']['entries'].append(entry)
        for relation in memory['relations']:
            formed['memory']['relations'].append({**relation, 'to': translated[relation['to']]})
        for proof in memory['evidence']:
            formed['memory']['evidence'].append({**proof, 'source': translated[proof['source']]})
    return {**summary, 'baseline': baseline, 'formed': formed,
        'status': 'partial' if summary['failed_episodes'] else 'completed',
        'limits': ['Use separate stores for these paired variants; replay the exact saved payload.',
                   'Repeated derived memories are preserved, not automatically consolidated.',
                   'Sources survive failed formation; acceptance is not independent proof of quality.']}
