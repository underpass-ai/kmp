"""Validate source-only contexts and literal, scoped entity proposals."""
from formation.contracts import digest, episode, fields, string, timestamp

VERSION = 'entities-v1-cited-mentions'
KINDS = ('person', 'organization', 'project', 'place', 'device', 'account', 'other')
BASES = ('explicit_alias', 'explicit_rename', 'stable_identifier', 'unambiguous_reference')


def context(value):
    fields(value, ('about', 'context_id', 'as_of', 'sources'), 'entity context')
    episode({'about': value['about'], 'episode_id': value['context_id'], 'sources': value['sources']})
    cutoff = timestamp(value['as_of'])
    if any(timestamp(s['observed_at']) > cutoff for s in value['sources']):
        raise ValueError('Entity context contains a source after its explicit cutoff')
    return value


def positions(text, needle):
    start = 0
    while (start := text.find(needle, start)) >= 0:
        yield start
        start += 1


def mention(value, sources):
    fields(value, ('source_id', 'text', 'occurrence'), 'mention')
    sid = string(value['source_id'], 'mention.source_id', 256)
    text = string(value['text'], 'mention.text', 256)
    occurrence = value['occurrence']
    if sid not in sources or type(occurrence) is not int or occurrence < 0:
        raise ValueError('Mention requires an admitted source and nonnegative occurrence')
    offsets = list(positions(sources[sid]['text'], text))
    if occurrence >= len(offsets):
        raise ValueError('Mention is not the requested literal source occurrence')
    start = offsets[occurrence]
    return {**value, 'start': start, 'end': start + len(text)}


def candidates(extracted, source_context):
    sources = {s['id']: s for s in context(source_context)['sources']}
    fields(extracted, ('proposals',), 'entity extraction')
    if not isinstance(extracted['proposals'], list) or len(extracted['proposals']) > 32:
        raise ValueError('At most 32 identity proposals per bounded context')
    accepted, rejected, seen = [], [], set()
    for index, proposal in enumerate(extracted['proposals']):
        try:
            fields(proposal, ('left', 'right', 'kind', 'basis', 'why', 'citations'), 'identity proposal')
            left, right = [mention(proposal[key], sources) for key in ('left', 'right')]
            key = lambda m: (m['source_id'], m['start'], m['end'])
            left, right = sorted((left, right), key=key)
            pair = (key(left), key(right))
            if pair[0] == pair[1] or pair in seen:
                raise ValueError('Self-equivalence or duplicate mention pair')
            if proposal['kind'] not in KINDS or proposal['basis'] not in BASES:
                raise ValueError('Unsupported entity kind or identity basis; name equality alone is not proof')
            string(proposal['why'], 'identity.why', 1600)
            citations = proposal['citations']
            if not isinstance(citations, list) or not 1 <= len(citations) <= 8:
                raise ValueError('Identity requires 1..8 literal proof quotes')
            seen_quotes = set()
            for citation in citations:
                fields(citation, ('source_id', 'quote', 'why'), 'identity citation')
                sid = string(citation['source_id'], 'citation.source_id', 256)
                quote = string(citation['quote'], 'citation.quote', 24000)
                string(citation['why'], 'citation.why', 1600)
                if sid not in sources or quote not in sources[sid]['text']:
                    raise ValueError('Identity proof must quote an admitted source verbatim')
                if (sid, quote) in seen_quotes:
                    raise ValueError('Duplicate identity quote')
                seen_quotes.add((sid, quote))
            for endpoint in (left, right):
                if not any(c['source_id'] == endpoint['source_id'] and any(
                        start <= endpoint['start'] and endpoint['end'] <= start + len(c['quote'])
                        for start in positions(sources[c['source_id']]['text'], c['quote'])) for c in citations):
                    raise ValueError('Proof must cover each exact mention occurrence')
            normalized = {**proposal, 'left': left, 'right': right,
                          'citations': sorted(citations, key=lambda c: (c['source_id'], c['quote']))}
            accepted.append({'id': digest(normalized), **normalized})
            seen.add(pair)
        except (TypeError, ValueError) as error:
            rejected.append({'index': index, 'proposal': proposal, 'reason': str(error)})
    return accepted, rejected


def verdicts(value, proposed):
    fields(value, ('verdicts',), 'identity verification')
    by_id = value['verdicts']
    fields(by_id, [p['id'] for p in proposed], 'identity verdict IDs')
    accepted, rejected = [], []
    for proposal in proposed:
        verdict = by_id[proposal['id']]
        fields(verdict, ('same_entity', 'reason'), 'identity verdict')
        if type(verdict['same_entity']) is not bool:
            raise ValueError('Identity verdict must be boolean')
        string(verdict['reason'], 'identity verdict reason', 2400)
        if verdict['same_entity']:
            accepted.append(proposal)
        else:
            rejected.append({'proposal': proposal, 'reason': verdict['reason']})
    return accepted, rejected
