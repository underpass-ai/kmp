"""Strict JSON Lines: one canonical object per line, duplicate keys refused.

Writers emit canonical JSON (cachekey.canonical_json), so a record's line is
also the exact text its digest is computed over. Private records never land
inside the repository: `guard_private_path` is the one check every writer of
private material calls before opening a file.
"""
import json
from pathlib import Path

from .cachekey import canonical_json
from .errors import PrivacyViolation

REPO_ROOT = Path(__file__).resolve().parents[4]


def _unique(error, origin, number):
    def hook(pairs):
        value = {}
        for key, item in pairs:
            if key in value:
                raise error(f'{origin}:{number}: duplicate JSON key {key!r}')
            value[key] = item
        return value
    return hook


def parse_lines(text, origin, error):
    """[(line_number, object)] for every non-blank line; line numbers start at 1."""
    rows = []
    for number, line in enumerate(text.split('\n'), start=1):
        if not line.strip():
            continue
        try:
            value = json.loads(line, object_pairs_hook=_unique(error, origin, number))
        except ValueError as failure:
            raise error(f'{origin}:{number}: not JSON: {failure}') from failure
        if not isinstance(value, dict):
            raise error(f'{origin}:{number}: a record is a JSON object')
        rows.append((number, value))
    return rows


def dump_lines(records):
    return ''.join(canonical_json(record) + '\n' for record in records)


def guard_private_path(path, repo_root=REPO_ROOT):
    """Refuse a destination inside the repository (git-ignored tmp/ included)."""
    path, repo_root = Path(path).resolve(), Path(repo_root).resolve()
    if path == repo_root or repo_root in path.parents:
        raise PrivacyViolation(f'private material may not be written inside the repository: {path}')
    return path


def write_text(path, text, overwrite=False):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('w' if overwrite else 'x', encoding='utf-8') as target:
        target.write(text)
    return path
