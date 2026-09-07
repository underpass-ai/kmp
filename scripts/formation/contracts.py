"""Source-only boundary for the optional local memory writer."""
from datetime import datetime, timezone
import hashlib
import json

VERSION = 'formation-v6-bound-verdict-ids'
CATEGORIES = ('fact', 'preference', 'decision', 'constraint', 'event')


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False,
                                     separators=(',', ':')).encode()).hexdigest()


def fields(value, expected, name):
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValueError(f'{name}: expected exactly {sorted(expected)}')


def string(value, name, limit):
    if not isinstance(value, str) or not value.strip() or len(value) > limit:
        raise ValueError(f'{name}: nonempty string of at most {limit} characters required')
    return value


def timestamp(value):
    string(value, 'timestamp', 64)
    stamp = datetime.fromisoformat(value.replace('Z', '+00:00'))
    if stamp.tzinfo is None:
        raise ValueError('timestamp must include a timezone')
    return stamp.astimezone(timezone.utc)


def episode(value):
    # In particular, benchmark questions, labels and answers cannot cross here.
    fields(value, ('about', 'episode_id', 'sources'), 'episode')
    about = string(value['about'], 'about', 256)
    if any(c.isspace() for c in about) or about.endswith(':'):
        raise ValueError('about must be an exact non-whitespace KMP scope')
    string(value['episode_id'], 'episode_id', 256)
    sources = value['sources']
    if not isinstance(sources, list) or not 1 <= len(sources) <= 128:
        raise ValueError('episode requires 1..128 sources')
    seen, size = set(), 0
    for source in sources:
        fields(source, ('id', 'role', 'text', 'observed_at'), 'source')
        sid = string(source['id'], 'source.id', 256)
        if sid in seen:
            raise ValueError('duplicate source id')
        seen.add(sid)
        if source['role'] not in ('user', 'assistant', 'system'):
            raise ValueError('unsupported source role')
        size += len(string(source['text'], 'source.text', 24000))
        timestamp(source['observed_at'])
    if size > 24000:
        raise ValueError('episode exceeds 24000 characters; split at source boundaries explicitly')
    return value


def candidates(value, source_episode):
    fields(value, ('memories',), 'extraction')
    memories = value['memories']
    if not isinstance(memories, list) or len(memories) > 24:
        raise ValueError('extraction requires an array of at most 24 memories')
    sources = {s['id']: s for s in episode(source_episode)['sources']}
    admitted, rejected, seen = [], [], set()
    for index, memory in enumerate(memories):
        try:
            fields(memory, ('text', 'category', 'citations'), 'memory')
            string(memory['text'], 'memory.text', 1600)
            if memory['category'] not in CATEGORIES:
                raise ValueError('unsupported memory category')
            citations = memory['citations']
            if not isinstance(citations, list) or not 1 <= len(citations) <= 8:
                raise ValueError('memory requires 1..8 citations')
            citation_keys = set()
            for citation in citations:
                fields(citation, ('source_id', 'quote', 'why'), 'citation')
                sid = string(citation['source_id'], 'citation.source_id', 256)
                quote = string(citation['quote'], 'citation.quote', 24000)
                string(citation['why'], 'citation.why', 1200)
                if sid not in sources or quote not in sources[sid]['text']:
                    raise ValueError('citation is not a literal span of an admitted source')
                key = (sid, quote)
                if key in citation_keys:
                    raise ValueError('duplicate citation')
                citation_keys.add(key)
            canonical = {**memory, 'citations': sorted(citations, key=lambda c: (c['source_id'], c['quote']))}
            fingerprint = digest(canonical)
            if fingerprint in seen:
                raise ValueError('duplicate memory')
            seen.add(fingerprint)
            admitted.append({'id': fingerprint, **canonical})
        except (TypeError, ValueError) as error:
            rejected.append({'index': index, 'memory': memory, 'reason': str(error)})
    return admitted, rejected


def verdicts(value, proposed):
    fields(value, ('verdicts',), 'verification')
    rows = value['verdicts']
    if not isinstance(rows, list) or len(rows) != len(proposed):
        raise ValueError('verification must cover every candidate exactly once')
    pending = {m['id']: m for m in proposed}
    accepted, rejected = [], []
    for row in rows:
        fields(row, ('id', 'supported', 'reason'), 'verdict')
        string(row['id'], 'verdict.id', 64)
        if row['id'] not in pending or type(row['supported']) is not bool:
            raise ValueError('unknown/duplicate verdict or non-boolean support')
        string(row['reason'], 'verdict.reason', 1600)
        memory = pending.pop(row['id'])
        if row['supported']:
            accepted.append(memory)
        else:
            rejected.append({'memory': memory, 'reason': row['reason']})
    return sorted(accepted, key=lambda m: m['id']), rejected
