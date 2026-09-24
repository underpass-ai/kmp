"""Report header: what produced these numbers, so they can be recomputed."""
import hashlib
from pathlib import Path
import platform
import sys

PACKAGE = Path(__file__).resolve().parents[1]
SCHEMA = 'kmp.token.measure.v1'
UNIT_NOTE = ('whole-unit reference counts per declared encoder and representation; '
             'not billing or host context; representations overlap and are never summed')


def harness_code_sha256():
    """Digest of the harness source (tests excluded), path-qualified and ordered."""
    digest = hashlib.sha256()
    for path in sorted(PACKAGE.rglob('*.py')):
        relative = path.relative_to(PACKAGE).as_posix()
        if relative.startswith('tests/'):
            continue
        digest.update(relative.encode() + b'\0' + path.read_bytes() + b'\0')
    return digest.hexdigest()


def header(counters, representations, verification):
    return {'schema_version': SCHEMA, 'evidence_scope': 'historical_import', 'unit': UNIT_NOTE,
            'environment': {'python': sys.version, 'implementation': platform.python_implementation(),
                            'platform': platform.platform()},
            'tokenizers': [c.identity.as_dict() for c in counters],
            'representations': [r.value for r in representations],
            'harness': {'code_sha256': harness_code_sha256()},
            'capture': {'run': verification['run'],
                        'manifest_sha256': verification['manifest_sha256'],
                        'binary_sha256': verification['binary_sha256'],
                        'verified_files': verification['verified_files']}}
