"""The kernel's own reading of a text, through `kmp_search_probe` (SCHEMAS.md, Search probe).

`run_lines` is the one adapter over the probe process: JSON Lines in, the checked
`kmp.bench.search_probe.v1` records out. Two views sit on it:

- `BinaryProbe` (here), used by the real-store corpora: the oracle for "this term
  occurs in that entry" is the tokenizer the ranker runs, never a Python imitation.
  Tests use a fake with the same `probe` signature.
- `generator.probe.SearchProbe`, used by the synth-v1 generator's oracles and
  calibration, which reads the raw records.
"""
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import subprocess

from ..domain.jsonl import REPO_ROOT
from .errors import ProbeFailed

SCHEMA = 'kmp.bench.search_probe.v1'
DEFAULT_PATH = REPO_ROOT / 'target' / 'release' / 'kmp_search_probe'
BUILD_HINT = 'build it with cargo build --release -p kmp-testkit --bin kmp_search_probe'


@dataclass(frozen=True)
class ProbeTerms:
    text: str
    language: str | None
    identifiers: frozenset
    search_keys: frozenset
    informative_terms: frozenset


def run_lines(path, requests):
    """The output records of one probe process over `requests`, in order, schema-checked.

    The probe is a pure function of its input and the kernel build, so one process per
    batch is enough. A request carries `text` or `texts`; the probe writes one record
    per text, so callers check the count against what they sent.
    """
    requests = list(requests)
    if not requests:
        return []
    payload = ''.join(json.dumps(request, ensure_ascii=False) + '\n' for request in requests)
    completed = subprocess.run([str(path), '-'], input=payload, capture_output=True, text=True,
                               encoding='utf-8', check=False)
    if completed.returncode != 0:
        raise ProbeFailed(f'exit {completed.returncode}: {completed.stderr.strip()[:500]}')
    records = []
    for line in completed.stdout.split('\n'):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except ValueError as failure:
            raise ProbeFailed(f'probe answered a non-JSON line: {failure}') from failure
        if not isinstance(record, dict) or record.get('schema') != SCHEMA:
            raise ProbeFailed('probe answered out of contract (schema)')
        records.append(record)
    return records


def _terms(record, expected_text):
    if record.get('text') != expected_text:
        raise ProbeFailed('probe answered out of contract (schema or text mismatch)')
    return ProbeTerms(record['text'], record['language'], frozenset(record['identifiers']),
                      frozenset(record['search_keys']), frozenset(record['informative_terms']))


class BinaryProbe:
    """One probe process per request; `sha256` identifies the binary in provenance."""

    def __init__(self, path):
        self.path = Path(path)
        if not self.path.is_file():
            raise ProbeFailed(f'kmp_search_probe not found at {self.path}; {BUILD_HINT}')
        self.sha256 = hashlib.sha256(self.path.read_bytes()).hexdigest()

    def probe(self, about_texts, texts):
        """ProbeTerms for each of `texts`, read with the morphology of `about_texts`."""
        texts = list(texts)
        if not texts:
            return ()
        records = run_lines(self.path, [{'about_texts': list(about_texts), 'texts': texts}])
        if len(records) != len(texts):
            raise ProbeFailed(f'asked {len(texts)} texts, got {len(records)} lines')
        return tuple(_terms(record, text) for record, text in zip(records, texts))
