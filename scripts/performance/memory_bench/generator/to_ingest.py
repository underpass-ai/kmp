"""From a synth-v1 world to the KMP calls that load it (SCHEMAS.md section 1.3).

For each about, in `abouts.jsonl` order, the entries at the level in ordinal order are
split into batches of `batch_size`; each batch is one `kmp_ingest` whose relations are the
ones whose later endpoint the batch holds. Afterwards every write at the level goes through
`kmp_write_memory` in id order. `ordinal`, `block` and `truth` never leave the bench.

This module only builds the calls; sending them, resolving a `needs_review` and checking
receipts is the store builder's job (BT14).
"""
import json


def idempotency_key(world_key, about, batch_size, index):
    return f'synth-v1:{world_key[:16]}:{about}:B{batch_size}:{index}'


def entry_payload(entry):
    payload = {'id': entry['ref'], 'kind': entry['kind'], 'text': entry['text'],
               'coordinates': entry['coordinates']}
    if entry['metadata']:
        payload['metadata'] = entry['metadata']
    return payload


def relation_payload(relation):
    payload = {key: relation[key] for key in ('from', 'to', 'rel', 'class', 'why', 'evidence',
                                              'confidence')}
    if relation['clocks'] is not None:
        payload['clocks'] = relation['clocks']
    return payload


def ingest_calls(world, level, batch_size):
    """[(about, kmp_ingest arguments)] loading `world` up to `level` in batches."""
    if batch_size < 1:
        raise ValueError('batch_size must be positive')
    bound = level // world.params.block_size
    abouts = [json.loads(line) for line in world.lines['abouts']]
    entries = {record['about']: [] for record in abouts}
    for line, block in zip(world.lines['entries'], world.blocks_of['entries']):
        if block < bound:
            entry = json.loads(line)
            entries[entry['about']].append(entry)
    later = {}
    for line, block in zip(world.lines['relations'], world.blocks_of['relations']):
        if block < bound:
            relation = json.loads(line)
            ordinal = max(int(relation['from'].rsplit(':e', 1)[1]),
                          int(relation['to'].rsplit(':e', 1)[1]))
            later.setdefault(ordinal, []).append(relation)
    calls = []
    for record in abouts:
        about = record['about']
        rows = entries[about]
        for index, start in enumerate(range(0, len(rows), batch_size)):
            batch = rows[start:start + batch_size]
            relations = [relation_payload(relation) for entry in batch
                         for relation in later.get(entry['ordinal'], ())]
            calls.append((about, {
                'about': about,
                'idempotency_key': idempotency_key(world.world_key, about, batch_size, index),
                'memory': {'dimensions': record['dimensions'] if index == 0 else [],
                           'entries': [entry_payload(entry) for entry in batch],
                           'relations': relations}}))
    return calls


def write_calls(world, level):
    """[(about, kmp_write_memory arguments)] for the writes at `level`, in id order."""
    bound = level // world.params.block_size
    return [(write['about'], write['arguments'])
            for write, block in ((json.loads(line), block) for line, block in
                                 zip(world.lines['writes'], world.blocks_of['writes']))
            if block < bound]
