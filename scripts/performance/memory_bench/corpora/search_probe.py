"""The kernel's own reading of a text, through `kmp_search_probe` (SCHEMAS.md, Search probe).

`BinaryProbe` is the adapter every real generation uses: the oracle for "this
term occurs in that entry" is the tokenizer the ranker runs, never a Python
imitation. Tests use a fake with the same `probe` signature.
"""
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import subprocess

from .errors import ProbeFailed

SCHEMA = 'kmp.bench.search_probe.v1'


@dataclass(frozen=True)
class ProbeTerms:
    text: str
    language: str | None
    identifiers: frozenset
    search_keys: frozenset
    informative_terms: frozenset


def _terms(line, expected_text):
    try:
        record = json.loads(line)
    except ValueError as failure:
        raise ProbeFailed(f'probe answered a non-JSON line: {failure}') from failure
    if record.get('schema') != SCHEMA or record.get('text') != expected_text:
        raise ProbeFailed('probe answered out of contract (schema or text mismatch)')
    return ProbeTerms(record['text'], record['language'], frozenset(record['identifiers']),
                      frozenset(record['search_keys']), frozenset(record['informative_terms']))


class BinaryProbe:
    """Runs one probe process per request; `sha256` identifies the binary in provenance."""

    def __init__(self, path):
        self.path = Path(path)
        if not self.path.is_file():
            raise ProbeFailed(f'kmp_search_probe not found at {self.path}; '
                              'build it with cargo build --release -p kmp-testkit --bin kmp_search_probe')
        self.sha256 = hashlib.sha256(self.path.read_bytes()).hexdigest()

    def probe(self, about_texts, texts):
        """ProbeTerms for each of `texts`, read with the morphology of `about_texts`."""
        texts = list(texts)
        if not texts:
            return ()
        request = json.dumps({'about_texts': list(about_texts), 'texts': texts}, ensure_ascii=False)
        completed = subprocess.run([str(self.path), '-'], input=request + '\n', capture_output=True,
                                   text=True, encoding='utf-8', check=False)
        if completed.returncode != 0:
            raise ProbeFailed(f'exit {completed.returncode}: {completed.stderr.strip()[:500]}')
        lines = [line for line in completed.stdout.split('\n') if line.strip()]
        if len(lines) != len(texts):
            raise ProbeFailed(f'asked {len(texts)} texts, got {len(lines)} lines')
        return tuple(_terms(line, text) for line, text in zip(lines, texts))
