#!/usr/bin/env python3
"""Compare complete MCP catalogues and conditional-call acceptance, without a model.

Run with jsonschema and tiktoken installed; see --help. Counts describe separately
serialized representations, never billed context or an unobserved host refresh.
"""
import argparse
import copy
import gzip
import hashlib
import importlib.metadata
import itertools
import json
from pathlib import Path

import jsonschema
import tiktoken


def read(path):
    raw = path.read_bytes()
    if path.suffix == '.gz':
        raw = gzip.decompress(raw)
    return json.loads(raw)


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode()


def measure(value, encoder):
    raw = encoded(value)
    return {'bytes': len(raw), 'tokens': len(encoder.encode(raw.decode())),
            'sha256': hashlib.sha256(raw).hexdigest()}


def initial_calls():
    about = 'project:surface-control'
    ref = about + ':source'
    clock = {'time': '2026-09-01T12:00:00Z'}
    calls = {
        'kmp_ingest': [{'about': about, 'idempotency_key': 'surface-ingest',
            'memory': {'dimensions': [], 'entries': [{'id': ref, 'kind': 'claim',
                'text': 'The cache failed.', 'coordinates': [
                    {'dimension': 'task', 'scope_id': 'surface-control'}]}]}}],
        'kmp_wake': [{'about': about}],
        'kmp_ask': [{'about': about, 'question': 'When did the cache fail?'}],
        'kmp_relate': [{'about': about}],
        'kmp_goto': [{'about': about, 'at': clock}],
        'kmp_near': [{'about': about, 'around': clock}],
        'kmp_forward': [{'about': about, 'from': clock}],
        'kmp_rewind': [{'about': about, 'from': clock}],
        'kmp_trace': [{'about': about, 'from': ref, 'to': about + ':check'}],
        'kmp_inspect': [{'about': about, 'ref': ref}],
        'kmp_relabel': [{'about': about, 'ref': ref, 'actor': 'surface-control',
            'observed_at': '2026-09-01T12:00:00Z', 'why': 'Index the source.',
            'add': {'task': ['surface-control']}}],
        'kmp_condense': [{'about': about, 'ref': ref, 'actor': 'surface-control',
            'scope': 'node_body', 'language': 'en', 'card': 'The cache failed.',
            'source': {'revision': 1, 'record_digest': 'synthetic-schema-control'},
            'expect': {'absent': True}}],
        'kmp_write_memory': [
            {'about': about, 'actor': 'surface-control', 'memories': [
                {'id': 'failure', 'kind': 'observation', 'summary': 'The cache failed.',
                 'evidence': 'S1 reports the cache failure.'}],
             'labels': {'task': ['surface-control']}},
            {'about': about, 'context_id': 'context_control', 'search_summaries': [
                {'ref': ref, 'summary_en': 'The cache failed.'}]},
            {'about': about, 'actor': 'surface-control', 'relations': [
                {'from': ref, 'to': about + ':check', 'rel': 'verified_by',
                 'why': 'The check verifies the failure.', 'evidence': 'S2 checks S1.'}]},
        ],
    }
    return calls


def specimens(schema, bases):
    handle = {'continuation': 'read_' + 'a' * 32}
    yield None
    yield []
    yield 'not an object'
    yield {}
    yield handle
    for invalid in (None, '', 'read_bad', 4, [], {}):
        yield {'continuation': invalid}
    for name in schema['properties']:
        for value in (None, False, 0, '', 'x', [], {}):
            yield {**handle, name: value}
            yield {name: value}
            for base in bases:
                yield {**base, name: value}
    for base in bases:
        yield base
        yield {**base, 'unrecognized': True}
        yield {**base, **handle}
        # Cover all combinations of the initial required-field choices, including
        # each writer variant. This checks actor/context and exclusive payloads.
        keys = list(base)
        for present in itertools.product((False, True), repeat=len(keys)):
            yield {key: base[key] for key, keep in zip(keys, present) if keep}
        for actor, context in itertools.product((None, 'writer'), (None, 'context_control')):
            value = {key: val for key, val in base.items() if key not in ('actor', 'context_id')}
            if actor is not None:
                value['actor'] = actor
            if context is not None:
                value['context_id'] = context
            yield value
    if 'memory' in schema['properties']:
        for entries, relations, provenance, default in itertools.product(
                ([], bases[0]['memory']['entries']), (None, [], [{
                    'from': 'project:surface-control:source', 'to': 'project:surface-control:check',
                    'rel': 'supports', 'class': 'evidential'}]),
                (None, {}, {'observed_at': '2026-09-01T12:00:00Z'}), (None, False, True)):
            value = copy.deepcopy(bases[0])
            value['memory']['entries'] = entries
            if relations is not None:
                value['memory']['relations'] = relations
            if provenance is not None:
                value['provenance'] = provenance
            if default is not None:
                value['default_observation_to_ingestion'] = default
            yield value
    if 'expect' in schema['properties']:
        for expect in ({}, {'absent': True}, {'absent': False}, {'card_revision': 1},
                       {'card_revision': 0}, {'absent': True, 'card_revision': 1}):
            yield {**bases[0], 'expect': expect}
    if 'memories' in schema['properties']:
        common = {'about': 'project:surface-control', 'actor': 'surface-control'}
        payloads = {name: base[name] for base in bases for name in
                    ('memories', 'relations', 'search_summaries') if name in base}
        for keys in itertools.chain.from_iterable(
                itertools.combinations(payloads, n) for n in range(4)):
            yield {**common, **{key: payloads[key] for key in keys}}
        memory = copy.deepcopy(bases[0])
        del memory['memories'][0]['evidence']
        yield memory
        yield {**memory, 'options': {'strict': False}}
        for name in ('labels', 'occurred_at', 'read_context', 'review_token'):
            yield {**bases[1], name: bases[0].get(name, '2026-09-01T12:00:00Z')}


def property_shapes(value):
    """Properties/types/descriptions must survive; assertion placement may move."""
    if isinstance(value, dict):
        return {key: property_shapes(item) for key, item in value.items()
                if key not in ('oneOf', 'anyOf', 'allOf', 'if', 'then', 'else', 'not')}
    if isinstance(value, list):
        return [property_shapes(item) for item in value]
    return value


def compare(before, after):
    old = {tool['name']: tool for tool in before['tools']}
    new = {tool['name']: tool for tool in after['tools']}
    if old.keys() != new.keys():
        raise AssertionError('Tool inventory changed')
    results = []
    for name, bases in initial_calls().items():
        lhs, rhs = old[name]['inputSchema'], new[name]['inputSchema']
        if property_shapes(lhs['properties']) != property_shapes(rhs['properties']):
            raise AssertionError(f'{name}: argument descriptions or shapes changed')
        left = jsonschema.Draft202012Validator(lhs)
        right = jsonschema.Draft202012Validator(rhs)
        left.check_schema(lhs)
        right.check_schema(rhs)
        for base in bases:
            if not left.is_valid(base) or not right.is_valid(base):
                raise AssertionError(f'{name}: invalid control {base!r}')
        accepted = rejected = 0
        for value in specimens(lhs, bases):
            was, now = left.is_valid(value), right.is_valid(value)
            if was != now:
                raise AssertionError(f'{name}: acceptance differs for {value!r}')
            accepted += was
            rejected += not was
        # Every field outside inputSchema, especially outputSchema, stays exact.
        if {k: v for k, v in old[name].items() if k != 'inputSchema'} != {
                k: v for k, v in new[name].items() if k != 'inputSchema'}:
            raise AssertionError(f'{name}: non-input contract changed')
        results.append({'tool': name, 'accepted': accepted, 'rejected': rejected})
    unchanged = set(old) - set(initial_calls())
    if any(old[name] != new[name] for name in unchanged):
        raise AssertionError('An unrelated tool changed')
    return results


def inventory(catalogue, encoder):
    return {'catalogue': measure(catalogue, encoder), 'tools': [
        {'name': tool['name'], 'input': measure(tool['inputSchema'], encoder),
         'output': measure(tool.get('outputSchema'), encoder),
         'description': measure(tool['description'], encoder)}
        for tool in catalogue['tools']]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', required=True, type=Path)
    parser.add_argument('--after', required=True, type=Path)
    parser.add_argument('--host', type=Path)
    parser.add_argument('--before-native', type=Path)
    parser.add_argument('--after-native', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    before, after = read(args.before), read(args.after)
    encoder = tiktoken.get_encoding('o200k_base')
    result = {'encoder': 'o200k_base',
              'packages': {name: importlib.metadata.version(name) for name in ('tiktoken', 'jsonschema')},
              'unit': 'separately serialized compact UTF-8 JSON; no billing inference',
              'before': inventory(before, encoder), 'after': inventory(after, encoder),
              'schema_cases': compare(before, after), 'model_calls': 0}
    if bool(args.before_native) != bool(args.after_native):
        parser.error('Supply both native captures together')
    if args.before_native:
        result['native'] = {}
        for side, path, expected in (('before', args.before_native, before),
                                      ('after', args.after_native, after)):
            capture = read(path)
            model_catalogue = capture['profiles']['model'][1]['response']['result']
            if model_catalogue != expected:
                raise AssertionError(f'{side}: native catalogue differs from reviewed fixture')
            result['native'][side] = {'binary_sha256': capture['binary_sha256'],
                'profiles': {name: [{'method': event['request']['method'],
                    'request': measure(event['request'], encoder),
                    'response': measure(event['response'], encoder)} for event in events]
                    for name, events in capture['profiles'].items()}}
    if args.host:
        host = read(args.host)
        declarations = [tool for tool in host['tools'] if 'exec tool declaration:' in tool['description']]
        result['host_before'] = {'metadata': measure(host, encoder),
            'opaque_argument_tools': [tool['name'] for tool in declarations
                if 'args: { [key: string]: unknown; }' in tool['description']
                or 'args: unknown' in tool['description']],
            'source': host['source'], 'candidate_host_refresh_observed': False}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'before': result['before']['catalogue'],
                      'after': result['after']['catalogue'],
                      'schema_cases': sum(row['accepted'] + row['rejected'] for row in result['schema_cases'])}))


if __name__ == '__main__':
    main()
