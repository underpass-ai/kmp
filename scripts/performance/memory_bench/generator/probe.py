"""Adapter over `kmp_search_probe` (kmp.bench.search_probe.v1, SCHEMAS.md "Search probe").

The bench's lexical oracles never reimplement the kernel's tokenizer: they ask the probe
binary built from the same tree. The probe is a pure function of its input and the kernel
build, so one process per batch is enough.
"""
from dataclasses import dataclass
import json
from pathlib import Path
import subprocess

from ..domain.errors import BenchError
from ..domain.jsonl import REPO_ROOT

DEFAULT_PATH = REPO_ROOT / 'target' / 'release' / 'kmp_search_probe'
SCHEMA = 'kmp.bench.search_probe.v1'
# Texts that make the probe read one morphology. The ranker reads the about's language
# from its own texts; the oracles check under every morphology an about could get.
ENGLISH_ABOUT = ('The deployment of the gateway was frozen during the audit and the team '
                 'wrote a short report about the incident for the weekly review.',) * 3
SPANISH_ABOUT = ('El despliegue de la pasarela se congeló durante la auditoría y el equipo '
                 'escribió un informe breve sobre el incidente para la revisión semanal.',) * 3
MORPHOLOGIES = {'english': ENGLISH_ABOUT, 'spanish': SPANISH_ABOUT, 'none': ()}


class ProbeUnavailable(BenchError):
    code = 'SEARCH_PROBE_UNAVAILABLE'


@dataclass(frozen=True)
class SearchProbe:
    path: Path

    @classmethod
    def locate(cls, path=None):
        path = Path(path) if path else DEFAULT_PATH
        if not path.is_file():
            raise ProbeUnavailable(
                f'{path} not found: cargo build --release -p kmp-testkit --bin kmp_search_probe')
        return cls(path)

    def run(self, requests):
        """One output line (dict) per request, in order; each request carries one `text`."""
        if not requests:
            return []
        payload = ''.join(json.dumps(request, ensure_ascii=False) + '\n' for request in requests)
        done = subprocess.run([str(self.path), '-'], input=payload.encode('utf-8'),
                              capture_output=True, check=False)
        if done.returncode != 0:
            raise ProbeUnavailable(f'kmp_search_probe failed: {done.stderr.decode()[:400]}')
        lines = [json.loads(line) for line in done.stdout.decode('utf-8').splitlines() if line]
        if len(lines) != len(requests) or any(line['schema'] != SCHEMA for line in lines):
            raise ProbeUnavailable('kmp_search_probe answered an unexpected shape')
        return lines

    def search_keys(self, texts, morphology):
        about = list(MORPHOLOGIES[morphology])
        return [frozenset(line['search_keys']) for line in
                self.run([{'about_texts': about, 'text': text} for text in texts])]

    def overlap(self, pairs):
        """For (question, entry) pairs: the search keys they share under each morphology."""
        texts = [text for pair in pairs for text in pair]
        result = [dict() for _ in pairs]
        for morphology in MORPHOLOGIES:
            keys = self.search_keys(texts, morphology)
            for index in range(len(pairs)):
                shared = keys[2 * index] & keys[2 * index + 1]
                result[index][morphology] = sorted(shared)
        return result

    def informative_terms(self, texts):
        return [line['informative_terms'] for line in
                self.run([{'about_texts': [], 'text': text} for text in texts])]
