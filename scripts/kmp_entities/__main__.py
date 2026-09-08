"""Produce a cited entity ingest with a loopback model; never write a store."""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import sys

from .boundary import VERSION, context
from .planner import compile_plan
from .resolve import resolve

# The existing local client is a standalone formation script. Reuse its
# loopback-only transport without changing its frozen generation profile.
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'formation'))
from local_client import GENERATION_PROFILE, LocalClient


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--context', type=Path, required=True)
    parser.add_argument('--base', type=Path, required=True, help='Frozen canonical source/formed ingest')
    parser.add_argument('--output', type=Path, required=True, help='New directory')
    parser.add_argument('--base-url', default='http://127.0.0.1:8000/v1')
    parser.add_argument('--model', required=True)
    parser.add_argument('--model-revision', required=True)
    args = parser.parse_args()
    source = context(json.loads(args.context.read_text()))
    base = json.loads(args.base.read_text())
    observed_at = datetime.now(timezone.utc).isoformat()
    # Validate base/source alignment before any generation, even if the model
    # later proposes no identities. The actual plan revalidates it again.
    compile_plan(source, base, {'proposals': []}, {'verdicts': {}}, args.model_revision, observed_at)
    args.output.mkdir(parents=True, exist_ok=False)
    save(args.output / 'context.json', source)
    save(args.output / 'base.json', base)
    manifest = {'version': VERSION, 'status': 'running', 'model': args.model,
        'model_revision': args.model_revision, 'observed_at': observed_at,
        'generation': GENERATION_PROFILE, 'maximum_generation_calls': 2,
        'api_cost_usd': 0, 'store_modified': False}
    save(args.output / 'manifest.json', manifest)
    try:
        with (args.output / 'model-calls.jsonl').open('w') as log:
            def trace(event):
                log.write(json.dumps(event, ensure_ascii=False) + '\n')
                log.flush()
            model = LocalClient(args.base_url, args.model, trace)
            extracted, verified = resolve(source, model, lambda name, value: save(args.output / (name + '.json'), value))
        plan = compile_plan(source, base, extracted, verified, args.model_revision, observed_at)
        save(args.output / 'plan.json', plan)
        save(args.output / 'ingest.json', plan['ingest'])
        manifest.update(status='completed', accepted_count=plan['accepted_count'], mention_count=plan['mention_count'])
    except Exception as error:
        manifest.update(status='failed', error=str(error))
        raise
    finally:
        save(args.output / 'manifest.json', manifest)


if __name__ == '__main__':
    main()
