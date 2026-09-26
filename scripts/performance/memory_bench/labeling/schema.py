"""A small JSON Schema (2020-12 subset) checker for `gold_schema.json`, stdlib only.

It understands exactly the keywords the schema file uses — `type`, `const`,
`enum`, `pattern`, `minLength`, `properties`, `required`,
`additionalProperties: false`, `items`, `minItems`, `uniqueItems`, `$ref` into
`$defs` — and refuses a schema that uses anything else, so the file can never
promise a rule this checker silently ignores.
"""
import json
from pathlib import Path
import re

from ..domain.errors import BenchError

SCHEMA_PATH = Path(__file__).resolve().parent / 'gold_schema.json'
KNOWN = {'$schema', '$id', 'title', 'description', '$defs', '$ref', 'type', 'const', 'enum', 'pattern',
         'minLength', 'properties', 'required', 'additionalProperties', 'items', 'minItems', 'uniqueItems'}
TYPES = {'object': dict, 'array': list, 'string': str, 'boolean': bool, 'null': type(None)}


class SchemaViolation(BenchError):
    """A label record breaks labeling/gold_schema.json."""
    code = 'BREAL_LABEL_SCHEMA'


def load_schema(path=SCHEMA_PATH):
    schema = json.loads(Path(path).read_text(encoding='utf-8'))
    _check_keywords(schema, '#')
    return schema


def _check_keywords(node, where):
    if isinstance(node, dict):
        unknown = set(node) - KNOWN if not where.endswith(('/properties', '/$defs')) else set()
        if unknown:
            raise SchemaViolation(f'{where}: unsupported schema keyword(s) {sorted(unknown)}')
        for key, value in node.items():
            _check_keywords(value, f'{where}/{key}')
    elif isinstance(node, list):
        for index, value in enumerate(node):
            _check_keywords(value, f'{where}/{index}')


def _is_type(value, name):
    if name == 'integer':
        return isinstance(value, int) and not isinstance(value, bool)
    if name == 'number':
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    return isinstance(value, TYPES[name]) and not (name != 'boolean' and isinstance(value, bool))


def validate(value, schema, root=None, where='$'):
    """Raise SchemaViolation at the first rule `value` breaks."""
    root = schema if root is None else root
    if '$ref' in schema:
        name = schema['$ref'].removeprefix('#/$defs/')
        return validate(value, root['$defs'][name], root, where)
    kinds = schema.get('type')
    if kinds is not None:
        kinds = [kinds] if isinstance(kinds, str) else kinds
        if not any(_is_type(value, kind) for kind in kinds):
            raise SchemaViolation(f'{where}: expected {" or ".join(kinds)}')
    if 'const' in schema and value != schema['const']:
        raise SchemaViolation(f'{where}: must be {schema["const"]!r}')
    if 'enum' in schema and value not in schema['enum']:
        raise SchemaViolation(f'{where}: {value!r} is not one of {schema["enum"]}')
    if isinstance(value, str):
        if len(value) < schema.get('minLength', 0):
            raise SchemaViolation(f'{where}: shorter than {schema["minLength"]}')
        if 'pattern' in schema and not re.search(schema['pattern'], value):
            raise SchemaViolation(f'{where}: {value!r} does not match {schema["pattern"]}')
    if isinstance(value, dict):
        for key in schema.get('required', ()):
            if key not in value:
                raise SchemaViolation(f'{where}: missing {key}')
        properties = schema.get('properties', {})
        if schema.get('additionalProperties') is False:
            extra = sorted(set(value) - set(properties))
            if extra:
                raise SchemaViolation(f'{where}: unknown field(s) {", ".join(extra)}')
        for key, sub in properties.items():
            if key in value:
                validate(value[key], sub, root, f'{where}.{key}')
    if isinstance(value, list):
        if len(value) < schema.get('minItems', 0):
            raise SchemaViolation(f'{where}: fewer than {schema["minItems"]} items')
        if schema.get('uniqueItems') and len({json.dumps(v, sort_keys=True) for v in value}) != len(value):
            raise SchemaViolation(f'{where}: duplicate items')
        if 'items' in schema:
            for index, item in enumerate(value):
                validate(item, schema['items'], root, f'{where}[{index}]')
    return value
