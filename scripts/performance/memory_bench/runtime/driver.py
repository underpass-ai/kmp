"""One question journey on a BenchProcess, and the records it becomes.

The journey itself is token_harness' versioned driver, unchanged: the first
call is `Question.first_call`, then `projection.next_action` (or a write's
`next_actions[0]`) is executed verbatim until the server returns none or
`max_calls` (<= 256) is reached. The bench only taps the calls, so a record
can carry the exact arguments sent.

Records are built once the process has closed, because the server's journal
(server_log) supplies `server_ms`/`server_us` and the Jev evaluations. Every
measured value that could not be measured is None with its reason in
`absent`; a timed-out call is a censored `transport_error` whose `wall_ns` is
a lower bound (kmp.bench.call.v1).
"""
from dataclasses import dataclass

from ...token_harness.native.driver import run_journey
from ..domain.run_record import (CALL_MEASURED, MAX_CALLS_LIMIT, CallRecord, JourneyRecord,
                                 journey_name)
from . import server_log

FAILED_STATUSES = ('tool_error', 'rpc_error', 'transport_error')
WARMUP_STARTED = 'process started by a warm-up journey (startup_ns in run.json isolation.processes)'


@dataclass(frozen=True)
class JourneyPlan:
    question: object  # domain.question.Question
    sample: int
    repeat: int
    max_calls: int = MAX_CALLS_LIMIT
    max_bytes: int | None = None
    default_budget: dict | None = None

    @property
    def name(self):
        return journey_name(self.question.id, self.sample, self.repeat)


@dataclass(frozen=True)
class CallSpec:
    """What token_harness' driver reads from a scenario journey."""
    tool: str
    arguments: dict


@dataclass(frozen=True)
class JourneyRun:
    plan: JourneyPlan
    process_index: int
    outcome: object  # token_harness JourneyOutcome
    attempts: tuple  # session.CallAttempt
    startup_ns: int | None
    startup_absent: str | None
    warmup: bool = False

    def statuses(self):
        """kmp.bench.call.v1 status of each attempt: only the last one can have failed."""
        statuses = ['ok'] * len(self.attempts)
        if statuses and self.outcome.status in FAILED_STATUSES:
            statuses[-1] = self.outcome.status
        return statuses


def drive(process, plan, warmup=False):
    """Run one journey; a warm-up journey keeps its frames in the process trace."""
    tool, arguments = plan.question.first_call(plan.max_bytes, plan.default_budget)
    fresh = not process.attempts
    if fresh:
        startup, absent = process.startup_ns, None
    elif process.journeys_served == 0:
        startup, absent = None, WARMUP_STARTED
    else:
        startup, absent = None, f'process {process.index} already served {process.journeys_served} journey(s)'
    with process.journey(None if warmup else plan.name) as tap:
        outcome = run_journey(tap, CallSpec(tool, arguments), None, plan.max_calls)
    return JourneyRun(plan, process.index, outcome, tuple(tap.attempts), startup, absent, warmup)


def telemetry_bound(report, runs):
    """Positions below the bound can be attributed by order (server_log.for_call)."""
    if len(report.telemetry.calls) == report.tool_calls:
        return None
    failed = [attempt.process_call_index for run in runs
              for attempt, status in zip(run.attempts, run.statuses()) if status != 'ok']
    return min(failed) if failed else len(report.telemetry.calls)


def _phase(attempt, plan):
    if attempt.process_call_index == 0:
        return 'first'
    return 'repeat' if plan.repeat > 0 else 'warm'


def _server_fields(attempt, report, bound):
    found, reason = report.telemetry.for_call(attempt.process_call_index, attempt.tool, bound)
    if found is None:
        return {'server_ms': None, 'server_us': None, 'jev': None}, \
            {'server_ms': reason, 'server_us': reason, 'jev': reason}
    values = {'server_ms': found.tool.duration_ms, 'server_us': found.tool.duration_us}
    absent = {}
    if values['server_ms'] is None:
        absent['server_ms'] = 'kmp_mcp_tool line without duration_ms'
    if values['server_us'] is None:
        absent['server_us'] = server_log.NO_DURATION_US
    values['jev'], reason = found.jev()
    if reason is not None:
        absent['jev'] = reason
    return values, absent


def _observed_fields(observation):
    if observation is None:
        values = {name: None for name in ('rpc_id', 'response_path', 'response_sha256', 'response_bytes',
                                          'wall_ns', 'cpu_ns', 'rss_peak_kb', 'io')}
        reason = 'the call got no observation (transport failed before or during the request)'
        return values, {name: reason for name in values if name != 'rpc_id'}, False, None
    resources = observation.resources
    values = {'rpc_id': observation.rpc_id, 'response_path': observation.response_path,
              'response_sha256': observation.response_sha256,
              'response_bytes': observation.response_bytes, 'wall_ns': observation.wall_ns,
              'cpu_ns': resources.get('cpu_ns'), 'rss_peak_kb': resources.get('rss_peak_kb'),
              'io': resources.get('io')}
    absent = dict(resources.get('absent') or {})
    if observation.response_path is None:
        for name in ('response_path', 'response_sha256', 'response_bytes'):
            absent[name] = f'no response ({observation.censor_reason or "transport failed"})'
    return values, absent, observation.censored, observation.censor_reason


def call_records(run, report, bound, identity):
    """kmp.bench.call.v1 records of one journey; identity = {run_id, variant, binary_sha256}."""
    records = []
    for attempt, status in zip(run.attempts, run.statuses()):
        observed, absent, censored, censor_reason = _observed_fields(attempt.observation)
        server, server_absent = _server_fields(attempt, report, bound)
        absent.update(server_absent)
        values = {**observed, **server}
        data = {'schema': 'kmp.bench.call.v1', **identity, 'question_id': run.plan.question.id,
                'sample': run.plan.sample, 'repeat': run.plan.repeat, 'journey': run.plan.name,
                'call_index': attempt.call_index, 'process_call_index': attempt.process_call_index,
                'phase': _phase(attempt, run.plan), 'tool': attempt.tool,
                'arguments': attempt.arguments, 'status': status, **values,
                'jev': None if values['jev'] is None
                else {'evaluations': [item.as_dict() for item in values['jev']]},
                'censored': censored, 'censor_reason': censor_reason,
                'absent': {name: absent[name] for name in CALL_MEASURED
                           if values.get(name) is None and name in absent}}
        for name in CALL_MEASURED:
            if data[name] is None and name not in data['absent']:
                data['absent'][name] = 'not measured'
        records.append(CallRecord.from_dict(data, f'{run.plan.name}#{attempt.call_index}'))
    return records


def journey_record(run, identity):
    outcome = run.outcome
    data = {'schema': 'kmp.bench.journey.v1', 'run_id': identity['run_id'],
            'variant': identity['variant'], 'question_id': run.plan.question.id,
            'sample': run.plan.sample, 'repeat': run.plan.repeat, 'journey': run.plan.name,
            'tool': run.plan.question.tool, 'status': outcome.status, 'calls': len(run.attempts),
            'max_calls': outcome.max_calls, 'censored': outcome.censored,
            'censor_reason': outcome.censor_reason, 'startup_ns': run.startup_ns,
            'error': outcome.error,
            'absent': {} if run.startup_ns is not None else {'startup_ns': run.startup_absent}}
    return JourneyRecord.from_dict(data, run.plan.name)
