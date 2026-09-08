"""Bound source IDs and verifier IDs to the admitted entity context."""
from .boundary import BASES, KINDS


def obj(properties):
    return {'type': 'object', 'properties': properties, 'required': list(properties), 'additionalProperties': False}


def extraction_schema(source_ids):
    text = {'type': 'string'}
    source = {'type': 'string', 'enum': list(source_ids)}
    endpoint = obj({'source_id': source, 'text': text, 'occurrence': {'type': 'integer', 'minimum': 0}})
    citation = obj({'source_id': source, 'quote': text, 'why': text})
    proposal = obj({'left': endpoint, 'right': endpoint, 'kind': {'type': 'string', 'enum': list(KINDS)},
        'basis': {'type': 'string', 'enum': list(BASES)}, 'why': text,
        'citations': {'type': 'array', 'items': citation, 'minItems': 1, 'maxItems': 8}})
    return obj({'proposals': {'type': 'array', 'items': proposal, 'maxItems': 32}})


def verification_schema(proposed):
    verdict = obj({'same_entity': {'type': 'boolean'}, 'reason': {'type': 'string'}})
    return obj({'verdicts': obj({p['id']: verdict for p in proposed})})
