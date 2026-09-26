"""Synthetic run directories for the scoring tests: exactly the layout BT15 seals.

`make_run` writes, for each question (and sample, and repeat), a token_harness trace
whose `tools/call` lines carry the structured answers `pages(question)` returns, plus
calls.jsonl, journeys.jsonl, run.json and a handshake trace, and seals the directory
with token_harness' own `_seal`. The answers are chosen by the test, so every metric
has a known expected value.
"""
import json
from pathlib import Path

from ...token_harness.adapters.tiktoken_counter import PINNED_ASSETS
from ...token_harness.native.capture import _seal
from ...token_harness.native.driver import DRIVER_VERSION
from .. import BENCH_VERSION
from ..domain import cachekey, jsonl
from ..domain.question import Question, questions_digest
from ..domain.run_record import journey_name

A = 'synth:mono-a000'


def ref(n):
    return f'{A}:e{n:07d}'


def question(identifier, kind, gold, tool='kmp_ask', **extra):
    record = {'schema': 'kmp.bench.question.v1', 'id': identifier, 'corpus': 'synth-v1', 'type': kind,
              'about': A, 'tool': tool, 'gold': gold, **extra}
    if tool == 'kmp_ask':
        record.setdefault('question', f'What about {identifier}?')
        record.setdefault('answer_policy', 'evidence_or_unknown')
    return Question.from_dict(record)


def facet(name, answers=(), related=(), words=None):
    return {'name': name, 'words': words, 'answers': [ref(n) for n in answers],
            'related': [ref(n) for n in related], 'answerable_facet': bool(answers)}


def answer(cited=(), evidence=None, confidence='high', missing=(), unknown=False, nearest=None, **extra):
    """A kmp_ask structured answer as v0.23.0 shapes it."""
    evidence = cited if evidence is None else evidence
    proof = {'evidence': [{'id': f'entry:{ref(n)}', 'text': f'text {n}'} for n in evidence],
             'confidence': confidence, 'missing': list(missing)}
    if nearest is not None:
        proof['nearest_outside'] = {'ref': ref(nearest)}
    return {'answer': 'UNKNOWN' if unknown else 'retrieved',
            'because': [] if unknown else [{'ref': f'entry:{ref(n)}', 'claim': str(n)} for n in cited],
            'projection': {'budget': {'used_bytes': 100}, 'next_action': None}, 'proof': proof, **extra}


def _call(record, **overrides):
    return {'schema': 'kmp.bench.call.v1', 'rpc_id': None, 'response_path': None,
            'response_sha256': None, 'response_bytes': None, 'wall_ns': None, 'server_ms': None,
            'server_us': None, 'cpu_ns': None, 'rss_peak_kb': None, 'io': None, 'jev': None,
            'censored': False, 'censor_reason': None, **record, **overrides}


def make_run(root, questions, pages, *, name='run', variant='baseline', binary='1' * 64, config='2' * 64,
             samples=1, repeats=1, jev=None, failures=(), mode='test', max_bytes=None, status=None,
             wall_ns=10_000_000, driver=DRIVER_VERSION, encoders=None):
    """Seal a run directory under `root/name`; returns its path.

    `pages(question, sample)` -> list of structured answers, one per call; `jev(question)`
    -> list of evaluation dicts (None: the binary has no telemetry); `status(question)` ->
    journey status override (e.g. 'transport_error')."""
    out = Path(root) / name
    out.mkdir(parents=True)
    digest = questions_digest(questions)
    key = {'binary_sha256': binary, 'config_digest': config, 'store_key': '3' * 64,
           'questions_digest': digest, 'mode': mode, 'bench_version': BENCH_VERSION, 'nonce': None}
    run_id = cachekey.result_key(**key)
    calls, journeys, rows, rpc = [], [], [], 10
    (out / 'process-0.jsonl').write_text(
        '{"session":"process-0","request":{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}},'
        '"response":{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"kmp-mcp"},"tools":[]}}}\n')
    for q in questions:
        for sample in range(samples):
            for repeat in range(repeats):
                journey = journey_name(q.id, sample, repeat)
                lines = [json.dumps({'session_start': journey, 'process': 'process-0'})]
                answers = pages(q, sample)
                forced = status(q) if status else None
                for index, structured in enumerate(answers):
                    rpc += 1
                    tool, arguments = q.first_call(max_bytes) if index == 0 else (q.tool, {'continuation': f'c{index}'})
                    request = json.dumps({'jsonrpc': '2.0', 'id': rpc, 'method': 'tools/call',
                                          'params': {'name': tool, 'arguments': arguments}})
                    response = json.dumps({'jsonrpc': '2.0', 'id': rpc, 'result': {
                        'content': [{'type': 'text', 'text': 'answer'}], 'isError': False,
                        'structuredContent': structured}})
                    lines.append('{"session":"process-0","request":' + request + ',"response":' + response + '}')
                    evaluations = jev(q) if jev else None
                    record = {'run_id': run_id, 'question_id': q.id, 'variant': variant, 'sample': sample,
                              'repeat': repeat, 'journey': journey, 'call_index': index,
                              'process_call_index': rpc, 'phase': 'repeat' if repeat else 'warm',
                              'binary_sha256': binary, 'tool': tool, 'arguments': arguments}
                    absent = {'server_us': 'fixture', 'cpu_ns': 'fixture', 'rss_peak_kb': 'fixture',
                              'io': 'fixture'}
                    if evaluations is None:
                        absent['jev'] = 'binary emits no kmp_mcp::judgement telemetry (predates BT03)'
                    failed = forced is not None and index == len(answers) - 1
                    calls.append(_call(record, status='transport_error' if failed else 'ok', rpc_id=rpc,
                                       response_path=None if failed else f'{journey}.jsonl#{len(lines) - 1}',
                                       response_sha256=None if failed else cachekey.sha256_hex(response.encode()),
                                       response_bytes=None if failed else len(response.encode()),
                                       wall_ns=wall_ns + rpc, server_ms=5,
                                       jev=None if evaluations is None else {'evaluations': evaluations},
                                       absent=dict(sorted({**absent, **({'response_path': 'failed',
                                                                         'response_sha256': 'failed',
                                                                         'response_bytes': 'failed'}
                                                                        if failed else {})}.items()))))
                (out / f'{journey}.jsonl').write_text('\n'.join(lines) + '\n')
                journeys.append({'schema': 'kmp.bench.journey.v1', 'run_id': run_id, 'question_id': q.id,
                                 'variant': variant, 'sample': sample, 'repeat': repeat, 'journey': journey,
                                 'tool': q.tool, 'status': forced or 'completed', 'calls': len(answers),
                                 'max_calls': 256, 'censored': False, 'censor_reason': None,
                                 'startup_ns': None, 'error': None, 'absent': {'startup_ns': 'fixture'}})
                rows.append({'lesson': journey, 'case_id': q.id, 'budget': max_bytes, 'exit_code': 0,
                             'driver_status': forced or 'completed'})
    manifest = {'schema': 'kmp.bench.run.v1', 'run_id': run_id, 'key': key,
                'variant': {'name': variant, 'path': f'variants/{variant}.toml',
                            'preregistration_digest': '5' * 64, 'config_digest': config},
                'binary': {'sha256': binary, 'version': '0.23.0', 'provenance': None},
                'store': {'key': '3' * 64, 'content_digest': 'sha256:' + '6' * 64, 'label': 'fixture'},
                'questions': {'digest': digest, 'count': len(questions), 'by_corpus': {'synth-v1': len(questions)}},
                'driver_version': driver,
                'encoders': encoders or [{'encoding': k, 'asset_sha256': v} for k, v in PINNED_ASSETS.items()],
                'samples': samples, 'repeats': repeats, 'max_calls': 256, 'timeout_s': 60.0,
                'private': False, 'max_bytes': None if max_bytes is None else [max_bytes],
                'isolation': {'processes': [{'startup_ns': 4_000_000}]}, 'failures': list(failures)}
    (out / 'run.json').write_text(cachekey.canonical_json(manifest) + '\n')
    (out / 'calls.jsonl').write_text(jsonl.dump_lines(calls))
    (out / 'journeys.jsonl').write_text(jsonl.dump_lines(journeys))
    _seal(out, {'kind': 'test fixture', 'evidence_scope': 'native_replay', 'variant': variant,
                'binary_sha256': binary, 'run_id': run_id, 'journeys': rows, 'model_calls': 0})
    return out


class WordCounter:
    """A deterministic stand-in for a tiktoken counter: one token per whitespace word."""

    def __init__(self, encoding='o200k_base'):
        from ...token_harness.domain.tokenizer import TokenizerIdentity
        self._identity = TokenizerIdentity('words', '1', encoding, 'split', '0' * 64, '0' * 64)

    @property
    def identity(self):
        return self._identity

    def count(self, text):
        return len(text.replace(',', ' ').replace(':', ' ').split())
