"""Bind verifier output to candidate identities before normalizing the boundary."""
from contracts import fields, string, verdicts
from prompts import TEXT, obj


def candidate_ids(proposed):
    ids = [string(memory['id'], 'candidate.id', 64) for memory in proposed]
    if not ids or len(set(ids)) != len(ids):
        raise ValueError('nonempty unique candidate identities required')
    return ids


def verification_schema(proposed):
    # Required object keys avoid asking the model to regenerate arbitrary hashes
    # or choose repeated IDs from an enum. The decoder must emit each key once.
    return obj({'verdicts': obj({identity: obj({'supported': {'type': 'boolean'}, 'reason': TEXT})
                                for identity in candidate_ids(proposed)})})


def normalize_verification(value, proposed):
    fields(value, ('verdicts',), 'bound verification')
    ids = candidate_ids(proposed)
    fields(value['verdicts'], ids, 'bound verdict IDs')
    rows = []
    for identity in ids:
        row = value['verdicts'][identity]
        fields(row, ('supported', 'reason'), 'bound verdict')
        rows.append({'id': identity, **row})
    normalized = {'verdicts': rows}
    # Keep the application boundary authoritative even if a server ignores its
    # schema, returns malformed output, or the verifier misjudges the content.
    verdicts(normalized, proposed)
    return normalized
