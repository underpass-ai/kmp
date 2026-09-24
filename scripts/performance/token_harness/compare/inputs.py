"""Load one side of a comparison and refuse anything that does not bind together.

The run is re-verified against its manifest before a number is read, and the
metrics and oracle files must name that exact manifest digest.
"""
from dataclasses import dataclass
import gzip
import hashlib
import json
from pathlib import Path

from ..capture import strict_json
from ..capture.manifest import verify_run
from ..domain.errors import HarnessError


class ComparisonRejected(HarnessError):
    code = 'COMPARISON_REJECTED'


@dataclass(frozen=True)
class SideInput:
    name: str
    verified: object
    capture: dict
    metrics: dict
    oracle: dict

    def journey_metrics(self):
        return {journey['journey']: journey for journey in self.metrics['journeys']}

    def journey_oracle(self):
        return {journey['journey']: journey for journey in self.oracle['journeys']}

    def tokenizers(self):
        return sorted(json.dumps(t, sort_keys=True) for t in self.metrics['tokenizers'])


def read_json(path):
    """Metrics and oracle files may be kept gzipped next to the capture."""
    raw = Path(path).read_bytes()
    return json.loads(gzip.decompress(raw) if str(path).endswith('.gz') else raw)


def load_side(name, run, metrics_path, oracle_path, max_file_bytes):
    verified = verify_run(run, max_file_bytes)
    metrics, oracle = read_json(metrics_path), read_json(oracle_path)
    for label, digest in (('metrics', metrics.get('capture', {}).get('manifest_sha256')),
                          ('oracle', oracle.get('manifest_sha256'))):
        if digest != verified.manifest_sha256:
            raise ComparisonRejected(f'{name} {label} was not computed from this verified run')
    traces = {}
    for row in verified.manifest.get('journeys', []):
        digest = hashlib.sha256(verified.files[row['lesson'] + '.jsonl']).hexdigest()
        if digest in traces:
            raise ComparisonRejected(f'{name}: {row["lesson"]} repeats the capture of {traces[digest]}')
        traces[digest] = row['lesson']
    capture = strict_json.loads(verified.files['capture.json'].decode('utf-8'))
    return SideInput(name, verified, capture, metrics, oracle)


def check_encoders(baseline, candidate):
    if baseline.tokenizers() != candidate.tokenizers():
        raise ComparisonRejected('baseline and candidate were measured with different encoders')


def configuration_mismatch(baseline, candidate):
    """Differences that make every row not comparable, or None."""
    checks = (('scenarios_digest', baseline.capture.get('scenarios_digest'),
               candidate.capture.get('scenarios_digest')),
              ('driver_version', baseline.capture.get('driver_version'),
               candidate.capture.get('driver_version')),
              ('representations', baseline.metrics.get('representations'),
               candidate.metrics.get('representations')))
    differing = [name for name, left, right in checks if left != right]
    return 'configuration differs: ' + ', '.join(differing) if differing else None
