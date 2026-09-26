"""run.json (kmp.bench.run.v1): what one run measured, and the key it is cached under.

The run id is the section 8 result key, recomputed here from the key material
the manifest carries, so a renamed or edited run directory cannot pass for
another. `comparability` is the anti-drift view BT10 compares before scoring.
"""
from dataclasses import dataclass

from . import cachekey
from .errors import CacheKeyInvalid, RunRecordInvalid
from .fields import Fields
from .run_record import MAX_CALLS_LIMIT, RUN_SCHEMA

KEY_FIELDS = ('binary_sha256', 'config_digest', 'store_key', 'questions_digest', 'mode',
              'bench_version', 'nonce')
OPTIONAL = ('machine', 'order', 'isolation', 'started_at', 'ended_at', 'failures', 'max_bytes', 'notes')


@dataclass(frozen=True)
class RunManifest:
    data: dict

    @classmethod
    def from_dict(cls, data, where='run.json'):
        f = Fields(data, where, RunRecordInvalid)
        f.text('schema', choices=(RUN_SCHEMA,))
        run_id = f.text('run_id', pattern=cachekey.HEX64.pattern)
        key = _table(f, 'key', KEY_FIELDS)
        variant = _table(f, 'variant', ('name', 'path', 'preregistration_digest', 'config_digest'))
        binary = _table(f, 'binary', ('sha256', 'version', 'provenance'))
        store = _table(f, 'store', ('key', 'content_digest', 'label'))
        questions = _table(f, 'questions', ('digest', 'count', 'by_corpus'))
        f.text('driver_version')
        encoders = f.objects('encoders')
        for name in ('samples', 'repeats'):
            f.integer(name, minimum=1)
        f.integer('max_calls', minimum=1, maximum=MAX_CALLS_LIMIT)
        f.number('timeout_s', minimum=0.0)
        if not isinstance(f.raw('private', required=True), bool):
            f.fail('private', 'a boolean')
        for name in OPTIONAL:
            f.raw(name)
        f.done()
        if not isinstance(key.get('bench_version'), str) or not key['bench_version']:
            f.fail('key', 'bench_version is required')
        try:
            expected = cachekey.result_key(**{name: key.get(name) for name in KEY_FIELDS})
        except CacheKeyInvalid as error:
            raise RunRecordInvalid(f'{where}.key: {error.detail}') from error
        checks = ((run_id, expected, 'run_id is not the result key of `key`'),
                  (binary.get('sha256'), key.get('binary_sha256'), 'binary.sha256 differs from key'),
                  (variant.get('config_digest'), key.get('config_digest'), 'variant.config_digest differs from key'),
                  (store.get('key'), key.get('store_key'), 'store.key differs from key'),
                  (questions.get('digest'), key.get('questions_digest'), 'questions.digest differs from key'))
        for found, wanted, message in checks:
            if found != wanted:
                raise RunRecordInvalid(f'{where}: {message}')
        if not all(isinstance(item, dict) and isinstance(item.get('encoding'), str) for item in encoders):
            f.fail('encoders', 'a list of {encoding, asset_sha256}')
        return cls(data)

    @property
    def run_id(self):
        return self.data['run_id']

    def comparability(self):
        """Section 8 anti-drift: runs differing here are `no_comparable`."""
        return {'questions_digest': self.data['key']['questions_digest'],
                'driver_version': self.data['driver_version'],
                'encoders': sorted((e['encoding'], e.get('asset_sha256')) for e in self.data['encoders']),
                'bench_version': self.data['key']['bench_version']}


def _table(fields, key, allowed):
    value = fields.mapping(key, required=True)
    unknown = sorted(set(value) - set(allowed))
    if unknown:
        fields.fail(key, f'unknown field(s) {", ".join(unknown)}')
    return value
