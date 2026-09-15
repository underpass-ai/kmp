#!/usr/bin/env python3
"""Capture authored surface journeys; keep failed attempts and whole RPC envelopes."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
LESSONS = ('semantic-batch', 'decision-history', 'four-clocks', 'budget-proof',
           'dimensional-memberships', 'distributed-incident', 'workflow-proof',
           'shared-resumption')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    rows = []
    for lesson in LESSONS:
        trace = args.output / (lesson + '.jsonl')
        result = args.output / (lesson + '-result.json')
        script = 'replay_shared.py' if lesson == 'shared-resumption' else 'replay.py'
        command = [sys.executable, str(ROOT / 'scripts/guide_examples' / script),
                   '--binary', str(args.binary.resolve()), '--trace', str(trace.resolve()),
                   '--result', str(result.resolve())]
        if lesson != 'shared-resumption':
            command += ['--lesson', lesson]
        start = time.perf_counter()
        run = subprocess.run(command, cwd=ROOT, capture_output=True)
        elapsed = time.perf_counter() - start
        (args.output / (lesson + '.stdout')).write_bytes(run.stdout)
        (args.output / (lesson + '.stderr')).write_bytes(run.stderr)
        rows.append({'lesson': lesson, 'command': command, 'exit_code': run.returncode,
                     'client_total_seconds': elapsed})
        print(json.dumps(rows[-1]), flush=True)
    files = {}
    for path in sorted(args.output.iterdir()):
        raw = path.read_bytes()
        files[path.name] = {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
        if path.suffix in ('.jsonl', '.json'):
            path.with_suffix(path.suffix + '.gz').write_bytes(gzip.compress(raw, mtime=0))
            path.unlink()  # lossless originals retained as gzip, hash recorded above
    manifest = {'binary': str(args.binary.resolve()),
                'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                'main_base': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                'model_calls': 0, 'human_review': False,
                'kind': 'authored deterministic replay; not agent learning or billing',
                'journeys': rows, 'uncompressed_files': files}
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return int(any(row['exit_code'] for row in rows))


if __name__ == '__main__':
    sys.exit(main())
