#!/usr/bin/env python3
"""Form a reviewable, source-linked KMP ingest plan with an optional local model."""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
from contracts import VERSION, candidates, digest, episode
from local_client import GENERATION_PROFILE, LocalClient
from coverage_review import REVIEW_SCHEMA, review_messages, reviewed_extraction
from planner import compile_plan
from prompts import EXTRACTION_SCHEMA, VERIFICATION_SCHEMA, extract_messages, verify_messages


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def form(source_episode, model, record_stage=lambda name, value: None):
    episode(source_episode)
    extracted = model.generate(extract_messages(source_episode), EXTRACTION_SCHEMA, 'formation_extract')
    record_stage('initial-extracted', extracted)
    draft, rejected = candidates(extracted, source_episode)
    # Always review coverage, including an empty draft. A missing fact creates
    # no rejected candidate and would be invisible to a rejection-only repair.
    reviewed = model.generate(review_messages(source_episode, draft, rejected), REVIEW_SCHEMA,
                              'formation_review')
    record_stage('coverage-review', reviewed)
    extracted = reviewed_extraction(reviewed)
    proposed, _ = candidates(extracted, source_episode)
    verified = (model.generate(verify_messages(source_episode, proposed), VERIFICATION_SCHEMA,
                              'formation_verify') if proposed else {'verdicts': []})
    return extracted, verified


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--episode', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='New directory; existing output is refused')
    parser.add_argument('--base-url', default='http://127.0.0.1:8000/v1')
    parser.add_argument('--model', required=True, help='Local served model name')
    parser.add_argument('--model-revision', required=True, help='Pinned weights identity, recorded without inference')
    args = parser.parse_args()
    source = episode(json.loads(args.episode.read_text()))
    observed_at = datetime.now(timezone.utc).isoformat()
    args.output.mkdir(parents=True, exist_ok=False)
    save(args.output / 'episode.json', source)
    manifest = {'version': VERSION, 'status': 'running', 'observed_at': observed_at,
                'source_sha256': digest(source), 'model_revision': args.model_revision,
                'model': args.model, 'generation': GENERATION_PROFILE, 'api_cost_usd': 0, 'store_modified': False}
    save(args.output / 'manifest.json', manifest)
    try:
        with (args.output / 'model-calls.jsonl').open('w') as trace:
            def record(event):
                trace.write(json.dumps(event, ensure_ascii=False) + '\n')
                trace.flush()
            client = LocalClient(args.base_url, args.model, record)
            extracted, verified = form(source, client,
                                      lambda name, value: save(args.output / (name + '.json'), value))
        save(args.output / 'extracted.json', extracted)
        save(args.output / 'verified.json', verified)
        plan = compile_plan(source, extracted, verified, args.model_revision, observed_at)
        save(args.output / 'plan.json', plan)
        save(args.output / 'ingest.json', plan['ingest'])
        manifest.update(status='completed', accepted_count=plan['accepted_count'],
                        rejected_count=len(plan['rejected']))
    except Exception as error:
        manifest.update(status='failed', error=str(error))
        raise
    finally:
        save(args.output / 'manifest.json', manifest)
    print(args.output / 'plan.json')


if __name__ == '__main__':
    main()
