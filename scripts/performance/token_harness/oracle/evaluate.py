"""oracle: judge every journey of a verified native capture against its scenario."""
from ..application.verify import journey_traces
from ..capture import strict_json
from ..capture.manifest import verify_run
from ..capture.trace import read_trace
from ..domain.errors import CaptureIntegrityError, HarnessError
from ..scenarios import BY_CASE, scenarios_digest
from .calls import tool_calls
from .contracts import contract
from .journey import evaluate_journey

SCHEMA = 'kmp.token.oracle.v1'


class ScenarioDriftError(HarnessError):
    code = 'SCENARIO_DRIFT'


def _json(verified, name):
    if name not in verified.files:
        raise CaptureIntegrityError(f'{name} is not a verified original of this run')
    return strict_json.loads(verified.files[name].decode('utf-8'))


def _readback(verified, case_id):
    name = case_id + '.oracle.jsonl'
    if name not in verified.files:
        return None
    return [call.response.get('result') for call in tool_calls(read_trace(name, verified.files[name]))]


def evaluate_run(root, max_file_bytes):
    verified = verify_run(root, max_file_bytes)
    capture = _json(verified, 'capture.json')
    cases = capture.get('cases') or {}
    selected = [BY_CASE[case_id] for case_id in capture.get('scenarios', {}) if case_id in BY_CASE]
    for scenario in selected:
        if capture['scenarios'][scenario.case_id]['digest'] != scenario.digest():
            raise ScenarioDriftError(f'{scenario.case_id} changed since the capture')
    adapter = contract(capture['wake_contract'])
    journeys = []
    for trace in journey_traces(verified):
        entry = next(row for row in verified.manifest['journeys'] if row['lesson'] == trace.journey)
        scenario, case = BY_CASE[entry['case_id']], cases.get(entry['case_id'], {})
        record = next((j for j in case.get('journeys', []) if j['journey'] == trace.journey), {})
        journeys.append(evaluate_journey(scenario, trace, record, case.get('fixture_refs') or {},
                                         _readback(verified, scenario.case_id), adapter))
    return {'schema_version': SCHEMA, 'run': verified.root.name,
            'manifest_sha256': verified.manifest_sha256, 'variant': capture.get('variant'),
            'wake_contract': adapter.contract_id, 'driver_version': capture.get('driver_version'),
            'scenarios_digest': scenarios_digest(selected),
            'capture_scenarios_digest': capture.get('scenarios_digest'),
            'capture_failures': capture.get('failures', []),
            'captured_cases': sorted(capture.get('scenarios', {})), 'journeys': journeys}
