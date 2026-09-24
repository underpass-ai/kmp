"""Versioned journey driver: follow the server's own next action, never a copied cursor.

Reads start from the scenario's arguments plus the budget under test and then
execute `projection.next_action` verbatim (a restart with a larger budget or a
cursor page) until the server returns none. A write that asks for review
follows `next_actions[0]` verbatim. Both variants run this same driver; the
candidate exposes no new capability this driver would have to use.
"""
from dataclasses import dataclass, field
import copy

from ..domain.errors import HarnessError

DRIVER_VERSION = 'kmp.native_driver.v1'
MAX_CALLS = 24


@dataclass
class JourneyOutcome:
    status: str = 'completed'  # completed | call_cap_reached | tool_error | rpc_error | transport_error
    calls: int = 0
    error: str | None = None
    results: list = field(default_factory=list)  # structuredContent per call, for read-back only

    def as_dict(self):
        return {'status': self.status, 'calls': self.calls, 'error': self.error}


def first_arguments(spec, budget):
    arguments = copy.deepcopy(spec.arguments)
    if budget is not None:
        arguments.setdefault('budget', {})['max_bytes'] = budget
    return arguments


def _next_read(structured):
    action = (structured.get('projection') or {}).get('next_action')
    return (action['tool'], action['arguments']) if action else None


def _next_write(structured):
    if structured.get('status') != 'needs_review':
        return None
    action = (structured.get('next_actions') or [None])[0]
    return (action['tool'], action['arguments']) if action else None


def run_journey(session, spec, budget):
    follow = _next_write if spec.tool == 'kmp_write_memory' else _next_read
    outcome, step = JourneyOutcome(), (spec.tool, first_arguments(spec, budget))
    try:
        while step is not None:
            if outcome.calls == MAX_CALLS:
                outcome.status = 'call_cap_reached'
                return outcome
            response = session.call(*step)
            outcome.calls += 1
            if 'error' in response:
                outcome.status, outcome.error = 'rpc_error', str(response['error'])[:500]
                return outcome
            result = response.get('result') or {}
            structured = result.get('structuredContent') or {}
            outcome.results.append(structured)
            if result.get('isError'):
                outcome.status = 'tool_error'
                outcome.error = str(structured.get('error') or result.get('content'))[:500]
                return outcome
            step = follow(structured)
    except HarnessError as error:
        outcome.status, outcome.error = 'transport_error', str(error)
    return outcome
