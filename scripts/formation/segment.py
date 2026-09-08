#!/usr/bin/env python3
"""Plan whole-turn formation episodes from one source-only reporting session."""
import argparse
import copy
import json
from pathlib import Path

from contracts import digest, episode, fields, string, timestamp

SEGMENT_VERSION = 'whole-turn-session-v1'


def segment_session(session, target_chars=8000, max_sources=32, overlap=True):
    """Preserve every nonblank turn; record context reuse and blank exclusions.

    The target is soft for a single turn, but the writer's 24,000-character
    limit is hard. Prompt token admission remains the model client's job.
    No future session, query, score, or model-generated text enters this plan.
    """
    fields(session, ('about', 'session_id', 'sources'), 'session')
    string(session['session_id'], 'session_id', 256)
    if type(target_chars) is not int or not 1 <= target_chars <= 24000:
        raise ValueError('target_chars must be an integer in 1..24000')
    if type(max_sources) is not int or not 1 <= max_sources <= 128:
        raise ValueError('max_sources must be an integer in 1..128')
    if type(overlap) is not bool:
        raise ValueError('overlap must be boolean')
    if not isinstance(session['sources'], list):
        raise ValueError('sources must be an array')
    # Validate the complete input before producing any episodes. Blank records
    # are retained by the caller and explicitly excluded only from generation.
    seen, clock, usable, blank = set(), None, [], []
    for source in session['sources']:
        fields(source, ('id', 'role', 'text', 'observed_at'), 'source')
        sid = string(source['id'], 'source.id', 256)
        if sid in seen:
            raise ValueError('duplicate source id')
        seen.add(sid)
        if not isinstance(source['text'], str) or len(source['text']) > 24000:
            raise ValueError('whole source exceeds 24000 characters or is not text')
        # Reuse the writer boundary, including scope, role and clock checks.
        checked = dict(source, text=source['text'] if source['text'].strip() else '[blank]')
        episode({'about': session['about'], 'episode_id': session['session_id'], 'sources': [checked]})
        observed = timestamp(source['observed_at'])
        if clock is not None and observed != clock:
            raise ValueError('one session must have one reporting timestamp; separate sessions explicitly')
        clock = observed
        if source['text'].strip():
            usable.append(copy.deepcopy(source))
        else:
            blank.append(sid)
    if not session['sources']:
        # Validate about even when a session contains no turns.
        episode({'about': session['about'], 'episode_id': session['session_id'], 'sources': [
            {'id': 'validation', 'role': 'system', 'text': '[empty]', 'observed_at': '1970-01-01T00:00:00Z'}]})
    policy = {'target_chars': target_chars, 'max_sources': max_sources, 'overlap_previous_turn': overlap}
    fingerprint = digest([SEGMENT_VERSION, session, policy])
    rows, index = [], 0
    while index < len(usable):
        start, chosen, reused = index, [], []
        prior = usable[index - 1] if index and overlap else None
        # Include context only if the next new turn still fits. Always advance
        # with at least one new turn, even when a whole turn exceeds the target.
        if prior and max_sources > 1 and len(prior['text']) + len(usable[index]['text']) <= target_chars:
            chosen.append(prior)
            reused.append(prior['id'])
        chosen.append(usable[index])
        size = sum(len(s['text']) for s in chosen)
        index += 1
        while index < len(usable) and len(chosen) < max_sources and size + len(usable[index]['text']) <= target_chars:
            chosen.append(usable[index])
            size += len(usable[index]['text'])
            index += 1
        source_episode = episode({'about': session['about'],
            'episode_id': 'segment-' + digest([fingerprint, len(rows)])[:32], 'sources': chosen})
        rows.append({'episode': source_episode, 'new_source_ids': [s['id'] for s in usable[start:index]],
            'context_source_ids': reused, 'context_omitted': prior['id'] if prior and not reused else None,
            'source_chars': size, 'whole_turn_exceeds_target': size > target_chars})
    return {'version': SEGMENT_VERSION, 'session_sha256': digest(session), 'policy': policy,
        'episodes': rows, 'input_source_count': len(session['sources']),
        'excluded_blank_source_ids': blank,
        'limits': ['Character packing does not guarantee model token admission.',
                   'Only the previous turn can overlap; longer antecedents may cross a boundary.',
                   'Do not concatenate episode ingests as a deduplicated session store.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--session', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='New JSON plan; an existing file is refused')
    parser.add_argument('--target-chars', type=int, default=8000)
    parser.add_argument('--max-sources', type=int, default=32)
    parser.add_argument('--no-overlap', action='store_true')
    args = parser.parse_args()
    plan = segment_session(json.loads(args.session.read_text()), args.target_chars, args.max_sources, not args.no_overlap)
    with args.output.open('x') as handle:
        json.dump(plan, handle, ensure_ascii=False, indent=2)
        handle.write('\n')


if __name__ == '__main__':
    main()
