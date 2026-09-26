"""Pre-registered negative rules: negatives.toml, its hash and its registration.

The rules are hashed as bytes (SHA-256). `negatives.sha256` beside the TOML
is the reviewed record of that hash; `load_rules` refuses a TOML whose hash
differs, so an edited rule cannot generate questions until it is registered
again. `register` also appends the hash to the private registry, and
`require_registered` is what a runner calls before the first run of a
candidate on generated questions (BENCH_SPEC section 2, principle 2).
"""
from dataclasses import dataclass
import datetime
import hashlib
import json
from pathlib import Path
import tomllib

from ..domain import jsonl
from ..domain.cachekey import canonical_json
from .errors import NegativeRulesInvalid

SCHEMA = 'kmp.bench.negatives.v1'
CONFIG_DIR = Path(__file__).resolve().parents[1] / 'config'
RULES_PATH = CONFIG_DIR / 'negatives.toml'
HASH_PATH = CONFIG_DIR / 'negatives.sha256'
REGISTRY_NAME = 'rules-registry.jsonl'
REQUIRED = {'schema': str, 'rules_version': str, 'seed': int, 'answer_policy': str,
            'per_form': int, 'min_per_form': int, 'audit_fraction': float, 'store': dict,
            'anchors': dict, 'attributes': dict, 'anchor_absent': dict,
            'anchor_neighbor_existing': dict, 'negated_anchor': dict,
            'singular_anchored_twin': dict, 'templates': dict, 'identifier_guards': dict}
LANGUAGES = ('en', 'es')
TEMPLATE_KEYS = ('short', 'long', 'twin_long', 'negated_short', 'negated_long', 'and')


@dataclass(frozen=True)
class NegativeRules:
    raw: dict
    sha256: str

    def __getitem__(self, key):
        return self.raw[key]

    def templates(self, language):
        return self.raw['templates'][language]


def rules_sha256(data):
    return hashlib.sha256(data).hexdigest()


def _check(raw):
    for key, kind in REQUIRED.items():
        if not isinstance(raw.get(key), kind) or isinstance(raw.get(key), bool):
            raise NegativeRulesInvalid(f'{key} must be a {kind.__name__}')
    if raw['schema'] != SCHEMA:
        raise NegativeRulesInvalid(f'schema must be {SCHEMA}')
    if not 0 < raw['audit_fraction'] <= 1 or raw['min_per_form'] > raw['per_form']:
        raise NegativeRulesInvalid('audit_fraction in (0, 1] and min_per_form <= per_form')
    for language in LANGUAGES:
        templates = raw['templates'].get(language) or {}
        missing = [key for key in TEMPLATE_KEYS if not templates.get(key)]
        if missing:
            raise NegativeRulesInvalid(f'templates.{language} lacks {", ".join(missing)}')
        for group in raw['singular_anchored_twin'].get(language) or ():
            if not group or any(' ' in word for word in group):
                raise NegativeRulesInvalid('twin attribute groups hold single words')


def parse_rules(data, expected_sha256=None):
    """NegativeRules from TOML bytes; with `expected_sha256`, a different hash is refused."""
    digest = rules_sha256(data)
    if expected_sha256 is not None and digest != expected_sha256:
        raise NegativeRulesInvalid(f'negatives.toml hashes to {digest}, registered {expected_sha256}; '
                                   'run the register command and review the change')
    try:
        raw = tomllib.loads(data.decode('utf-8'))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as failure:
        raise NegativeRulesInvalid(f'unreadable TOML: {failure}') from failure
    _check(raw)
    return NegativeRules(raw, digest)


def registered_sha256(hash_path=HASH_PATH):
    try:
        return Path(hash_path).read_text(encoding='utf-8').split()[0]
    except (OSError, IndexError) as failure:
        raise NegativeRulesInvalid(f'no registered hash at {hash_path}') from failure


def load_rules(path=RULES_PATH, hash_path=HASH_PATH):
    return parse_rules(Path(path).read_bytes(), registered_sha256(hash_path))


def register(private_root, path=RULES_PATH, hash_path=HASH_PATH, now=None):
    """Record the TOML's hash beside it and in the private registry; returns the hash."""
    registry = jsonl.guard_private_path(Path(private_root) / REGISTRY_NAME)
    rules = parse_rules(Path(path).read_bytes())
    Path(hash_path).write_text(f'{rules.sha256}  negatives.toml\n', encoding='utf-8')
    registry.parent.mkdir(parents=True, exist_ok=True)
    stamp = (now or datetime.datetime.now(datetime.timezone.utc)).strftime('%Y-%m-%dT%H:%M:%SZ')
    with registry.open('a', encoding='utf-8') as target:
        target.write(canonical_json({'rules_sha256': rules.sha256, 'rules_version': rules['rules_version'],
                                     'registered_at': stamp}) + '\n')
    return rules.sha256


def require_registered(rules_sha256_value, private_root):
    """Refuse questions whose rules were never registered in the private registry."""
    registry = Path(private_root) / REGISTRY_NAME
    try:
        lines = registry.read_text(encoding='utf-8').splitlines()
    except OSError as failure:
        raise NegativeRulesInvalid(f'no rules registry at {registry}') from failure
    if not any(json.loads(line).get('rules_sha256') == rules_sha256_value for line in lines if line.strip()):
        raise NegativeRulesInvalid(f'rules {rules_sha256_value} are not registered in {registry}')
    return True
