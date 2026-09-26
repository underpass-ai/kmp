"""Strict reading of one JSON/TOML object: typed getters, unknown keys refused.

Absent and null are the same for an optional field; a required field refuses
both. Booleans are never accepted where an integer or a number is expected.
"""
import math
import re


class Fields:
    def __init__(self, data, where, error):
        if not isinstance(data, dict):
            raise error(f'{where}: expected an object, got {type(data).__name__}')
        self.data, self.where, self.error, self.seen = data, where, error, set()

    def fail(self, key, message):
        raise self.error(f'{self.where}.{key}: {message}')

    def raw(self, key, required=False):
        self.seen.add(key)
        value = self.data.get(key)
        if value is None and required:
            self.fail(key, 'required')
        return value

    def text(self, key, required=True, pattern=None, choices=None):
        value = self.raw(key, required)
        if value is None:
            return None
        if not isinstance(value, str) or not value:
            self.fail(key, 'expected a non-empty string')
        if pattern is not None and not re.fullmatch(pattern, value):
            self.fail(key, f'{value!r} does not match {pattern}')
        if choices is not None and value not in choices:
            self.fail(key, f'{value!r} is not one of {", ".join(choices)}')
        return value

    def integer(self, key, required=True, minimum=None, maximum=None):
        value = self.raw(key, required)
        if value is None:
            return None
        if isinstance(value, bool) or not isinstance(value, int):
            self.fail(key, 'expected an integer')
        if minimum is not None and value < minimum or maximum is not None and value > maximum:
            self.fail(key, f'{value} outside [{minimum}, {maximum}]')
        return value

    def number(self, key, required=True, minimum=None):
        value = self.raw(key, required)
        if value is None:
            return None
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
            self.fail(key, 'expected a finite number')
        if minimum is not None and value < minimum:
            self.fail(key, f'{value} below {minimum}')
        return float(value)

    def flag(self, key, default):
        value = self.raw(key)
        if value is None:
            return default
        if not isinstance(value, bool):
            self.fail(key, 'expected a boolean')
        return value

    def texts(self, key, pattern=None, unique=True):
        """A list of non-empty strings, kept in the given order; () when absent."""
        value = self.raw(key)
        if value is None:
            return ()
        if not isinstance(value, list) or not all(isinstance(item, str) and item for item in value):
            self.fail(key, 'expected a list of non-empty strings')
        if pattern is not None:
            for item in value:
                if not re.fullmatch(pattern, item):
                    self.fail(key, f'{item!r} does not match {pattern}')
        if unique and len(set(value)) != len(value):
            self.fail(key, 'duplicate items')
        return tuple(value)

    def mapping(self, key, required=False):
        value = self.raw(key, required)
        if value is None:
            return None
        if not isinstance(value, dict):
            self.fail(key, 'expected an object')
        return value

    def objects(self, key):
        value = self.raw(key)
        if value is None:
            return ()
        if not isinstance(value, list):
            self.fail(key, 'expected a list')
        return tuple(value)

    def done(self):
        unknown = sorted(set(self.data) - self.seen)
        if unknown:
            raise self.error(f'{self.where}: unknown field(s) {", ".join(unknown)}')
