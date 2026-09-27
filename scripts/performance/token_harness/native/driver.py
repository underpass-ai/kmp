"""Versioned journey driver: follow the server's own next action, never a copied cursor.

Reads start from the scenario's arguments plus the budget under test and then
execute `projection.next_action` verbatim (a restart with a larger budget or a
cursor page) until the server returns none. A write that asks for review
follows `next_actions[0]` verbatim. Both variants run this same driver; the
candidate exposes no new capability this driver would have to use.

The call cap defaults to `MAX_CALLS` and may be raised up to `MAX_CALLS_LIMIT`.
A journey stopped by the cap is `call_cap_reached` and censored (`max_calls`);
one whose call timed out is `transport_error` and censored (`timeout`). A
censored journey is an observation with a lower bound, not a completed one.
"""
from dataclasses import dataclass, field
import copy

from ..domain.errors import HarnessError
from .transport import TransportTimeout

DRIVER_VERSION = 'kmp.native_driver.v1'
MAX_CALLS = 24
MAX_CALLS_LIMIT = 256


@dataclass
class JourneyOutcome:
    status: str = 'completed'  # completed | call_cap_reached | tool_error | rpc_error | transport_error
    calls: int = 0
    error: str | None = None
    results: list = field(default_factory=list)  # structuredContent per call, for read-back only
    max_calls: int = MAX_CALLS
    censored: bool = False
    censor_reason: str | None = None  # max_calls | timeout

    def as_dict(self):
        return {'status': self.status, 'calls': self.calls, 'error': self.error,
                'max_calls': self.max_calls, 'censored': self.censored,
                'censor_reason': self.censor_reason}

    def censor(self, reason):
        self.censored, self.censor_reason = True, reason


def check_max_calls(value):
    if isinstance(value, bool) or not isinstance(value, int) or not 1 <= value <= MAX_CALLS_LIMIT:
        raise HarnessError(f'max_calls must be an integer in 1..{MAX_CALLS_LIMIT}, got {value!r}')
    return value


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


def run_journey(session, spec, budget, max_calls=MAX_CALLS, follow_read=None):
    """`follow_read` replaces how a read's next call is chosen (the default
    follows `projection.next_action` only); writes always follow review."""
    follow = _next_write if spec.tool == 'kmp_write_memory' else (follow_read or _next_read)
    outcome = JourneyOutcome(max_calls=check_max_calls(max_calls))
    step = (spec.tool, first_arguments(spec, budget))
    try:
        while step is not None:
            if outcome.calls == max_calls:
                outcome.status = 'call_cap_reached'
                outcome.censor('max_calls')
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
    except TransportTimeout as error:
        outcome.status, outcome.error = 'transport_error', str(error)
        outcome.censor('timeout')
    except HarnessError as error:
        outcome.status, outcome.error = 'transport_error', str(error)
    return outcome
