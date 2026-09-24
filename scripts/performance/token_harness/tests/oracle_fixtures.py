"""Small wake/ask packets and a synthetic native run for oracle and comparator tests."""
import gzip
import hashlib
import json
from pathlib import Path

from ..capture.trace import read_trace
from ..scenarios.model import JourneySpec, Obligation, ObligationKind as K, Scenario
from .fakes import rpc, tool

BODY = 'S1: the change log moves the backup to 02:30.'
SUMMARY = 'The backup moved to 02:30.'
MEMORY = 'fixture:t:entry:observation:backup'
EVIDENCE_ID = 'detail:evidence:' + MEMORY + ':current'

SCENARIO = Scenario(
    case_id='t01', goal_id='orient_test', about='fixture:t', description='unit-test scenario',
    fixture=(), journey=JourneySpec('kmp_wake', {'about': 'fixture:t'}),
    obligations=(Obligation(K.STATE_MENTIONS, SUMMARY), Obligation(K.EVIDENCE_BODY, BODY)),
    unit_bodies=(BODY,), stored_bodies=(SUMMARY, BODY))


def evidence(text=BODY, identifier=EVIDENCE_ID, source='kmp_write_memory:w', owner=None):
    return {'id': identifier, 'source': source, 'text': text,
            'supports': [owner or 'evidence:' + MEMORY + ':current']}


def wake_page(claims, items, state=None, has_more=False, next_action=None, shortened=False,
              next_actions=(), path=(), missing=(), scope=None):
    return {'scope': scope or {},'wake': {'current_state': list(state if state is not None else ['(observation) ' + SUMMARY]),
                     'causal_spine': claims, 'next_actions': list(next_actions), 'open_loops': []},
            'proof': {'evidence': items, 'path': list(path), 'missing': list(missing)},
            'summary': 'Objective: t\nNext: none recorded',
            'projection': {'next_action': next_action, 'core_text_shortened': shortened,
                           'page': {'has_more': has_more}, 'excluded_by_detail': 0,
                           'selection_omitted': 0,
                           'sections': {'proof.evidence': {'remaining': 1 if has_more else 0}}}}


UNBOUNDED_SCOPE = {'context': ['summary', 'wake.current_state', 'wake.next_actions'],
                   'context_time': 'unbounded', 'request': ['wake.objective'],
                   'selection': ['proof', 'resume_cursor', 'wake.causal_spine', 'wake.guardrails']}


def claim_v2(refs=(EVIDENCE_ID,)):
    return {'claim': 'a -> b', 'because': 'why', 'evidence_refs': list(refs)}


def claim_v1(ref=BODY):
    return {'claim': 'a -> b', 'because': 'why', 'evidence_ref': ref}


def journey_events(pages, arguments=None):
    """initialize, initialized, tools/list, then one kmp_wake call per page."""
    events = [{'session_start': 'j'},
              rpc(1, 'initialize', {'protocolVersion': '2024-11-05'}, {'protocolVersion': '2024-11-05'}),
              {'request': {'jsonrpc': '2.0', 'method': 'notifications/initialized'}},
              rpc(2, 'tools/list', {}, {'tools': []})]
    arguments = arguments or [{'about': 'fixture:t', 'budget': {'max_bytes': 4096}}] * len(pages)
    for offset, (args, page) in enumerate(zip(arguments, pages)):
        events.append(tool(3 + offset, 'kmp_wake', args, page))
    return events + [{'session_end': 'j', 'exit_code': 0}]


def trace_of(events, name='t01-b4096'):
    raw = ''.join(json.dumps(e) + '\n' for e in events).encode()
    return read_trace(name, raw)


RECORD = {'budget': 4096, 'outcome': {'status': 'completed'}, 'exit_code': 0}


def write_native_run(folder, journeys, variant='baseline', scenarios_digest='d', driver='v1'):
    """A sealed run in the capture layout: journeys maps name -> event list."""
    folder = Path(folder)
    folder.mkdir(parents=True)
    capture = {'variant': variant, 'wake_contract': 'kmp.wake_claim.v2', 'driver_version': driver,
               'scenarios_digest': scenarios_digest, 'scenarios': {}, 'cases': {}, 'failures': []}
    originals = {name + '.jsonl': ''.join(json.dumps(e) + '\n' for e in events).encode()
                 for name, events in journeys.items()}
    originals['capture.json'] = json.dumps(capture).encode()
    files = {}
    for name, content in originals.items():
        files[name] = {'bytes': len(content), 'sha256': hashlib.sha256(content).hexdigest()}
        (folder / (name + '.gz')).write_bytes(gzip.compress(content, mtime=0))
    rows = [{'lesson': name, 'case_id': 't01', 'budget': 4096, 'exit_code': 0,
             'driver_status': 'completed'} for name in journeys]
    manifest = {'binary_sha256': 'synthetic', 'journeys': rows, 'uncompressed_files': files}
    (folder / 'manifest.json').write_text(json.dumps(manifest) + '\n')
    return folder
