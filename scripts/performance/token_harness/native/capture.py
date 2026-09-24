"""capture: run every scenario against one binary into a verifiable journey directory.

Layout (compatible with I0 `verify`/`measure`): one `<journey>.jsonl` trace per
measured session, listed in `manifest.json` `journeys`; fixture preparation and
oracle read-back traces sit beside them (`<case>.fixture.jsonl`,
`<case>.oracle.jsonl`) and are hashed but never measured as the journey.
"""
from datetime import datetime, timezone
import gzip
import hashlib
import itertools
import json
from pathlib import Path
import shutil

from ..application.provenance import harness_code_sha256
from ..domain.errors import HarnessError
from ..scenarios import scenarios_digest
from ..scenarios.model import journey_id
from .driver import DRIVER_VERSION, run_journey
from .fixture_loader import load_fixture
from .isolation import create_store
from .readback import read_back
from .recorder import TraceRecorder
from .transport import StdioSession

SCHEMA = 'kmp.token.capture.v1'


def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def _now():
    return datetime.now(timezone.utc).isoformat()


class _Run:
    def __init__(self, binary, out, work_dir, variant):
        self.binary, self.out, self.work_dir, self.variant = binary, out, work_dir, variant
        self.ids = itertools.count(1)  # JSON-RPC ids never collide across sessions of a run

    def session(self, store, trace, name, stderr_name):
        recorder = TraceRecorder(self.out / trace, [(str(store.root), '<store-root>')])
        session = StdioSession(self.binary, store, recorder, name, self.ids)
        recorder.marker({'session_start': name})
        return session, recorder, stderr_name

    def finish(self, store, session, recorder, stderr_name):
        code = session.close()
        recorder.marker({'session_end': session.name, 'exit_code': code})
        recorder.close()
        text = session.stderr_path.read_text(errors='replace').replace(str(store.root), '<store-root>')
        (self.out / stderr_name).write_text(text)
        return code


def _case(run, scenario, record):
    store = create_store(run.work_dir, f'{run.variant}-{scenario.case_id}')
    try:
        case = {'isolation': store.allowlisted_env(), 'fixture_refs': {}, 'journeys': []}
        s, rec, err = run.session(store, f'{scenario.case_id}.fixture.jsonl', 'fixture',
                                  f'{scenario.case_id}.fixture.stderr')
        try:
            case['protocol'] = s.handshake('token-harness-fixture')
            rec.marker({'fixture': 'fixture_preparation', 'case_id': scenario.case_id})
            case['fixture_refs'] = load_fixture(s, scenario) if scenario.fixture else {}
        finally:
            case['fixture_exit_code'] = run.finish(store, s, rec, err)
        for budget in scenario.journey.budgets:
            jid = journey_id(scenario.case_id, budget)
            s, rec, err = run.session(store, jid + '.jsonl', jid, jid + '.stderr')
            try:
                protocol = s.handshake('token-harness-journey')
                outcome = run_journey(s, scenario.journey, budget)
            finally:
                code = run.finish(store, s, rec, err)
            case['journeys'].append({'journey': jid, 'budget': budget, 'protocol': protocol,
                                     'outcome': outcome.as_dict(), 'exit_code': code})
            record.append({'lesson': jid, 'case_id': scenario.case_id, 'budget': budget,
                           'exit_code': code, 'driver_status': outcome.status})
            if scenario.readback:
                s, rec, err = run.session(store, scenario.case_id + '.oracle.jsonl', 'oracle',
                                          scenario.case_id + '.oracle.stderr')
                try:
                    s.handshake('token-harness-oracle')
                    rec.marker({'fixture': 'oracle_readback', 'case_id': scenario.case_id})
                    case['readback_calls'] = read_back(s, scenario, outcome.results)
                finally:
                    case['readback_exit_code'] = run.finish(store, s, rec, err)
        case['store_files'] = sorted(p.relative_to(store.data_dir).as_posix()
                                     for p in store.data_dir.rglob('*') if p.is_file())
        return case
    finally:
        shutil.rmtree(store.root)  # disposable by construction; nothing else lives there


def _seal(out, manifest_fields):
    files = {}
    for path in sorted(out.iterdir()):
        raw = path.read_bytes()
        files[path.name] = {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
        if path.suffix in ('.jsonl', '.json'):
            path.with_suffix(path.suffix + '.gz').write_bytes(gzip.compress(raw, mtime=0))
            path.unlink()
    manifest = {**manifest_fields, 'uncompressed_files': files}
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + '\n')
    return manifest


def capture(binary, variant, wake_contract, out, work_dir, scenarios, provenance=None):
    binary, out = Path(binary).resolve(), Path(out)
    binary_sha = sha256_file(binary)
    if provenance is not None and provenance.get('binary_sha256') != binary_sha:
        raise HarnessError('build provenance does not describe this binary')
    out.mkdir(parents=True, exist_ok=False)
    run, journeys, cases, failures = _Run(binary, out, work_dir, variant), [], {}, []
    started = _now()
    for scenario in scenarios:
        try:
            cases[scenario.case_id] = _case(run, scenario, journeys)
        except HarnessError as error:
            failures.append({'case_id': scenario.case_id, 'code': error.code, 'detail': error.detail})
    if sha256_file(binary) != binary_sha:
        raise HarnessError('binary changed during the capture')
    capture_record = {
        'schema_version': SCHEMA, 'evidence_scope': 'native_replay', 'variant': variant,
        'wake_contract': wake_contract, 'driver_version': DRIVER_VERSION,
        'scenarios_digest': scenarios_digest(scenarios),
        'scenarios': {s.case_id: {'digest': s.digest(), 'goal_id': s.goal_id} for s in scenarios},
        'binary_sha256': binary_sha, 'build_provenance': provenance,
        'harness_code_sha256': harness_code_sha256(), 'started_at': started, 'ended_at': _now(),
        'model_calls': 0, 'cases': cases, 'failures': failures}
    (out / 'capture.json').write_text(json.dumps(capture_record, indent=2, ensure_ascii=False) + '\n')
    return _seal(out, {'kind': 'native deterministic replay of synthetic scenarios; not agent or billing',
                       'evidence_scope': 'native_replay', 'variant': variant,
                       'binary_sha256': binary_sha, 'journeys': journeys,
                       'model_calls': 0})
