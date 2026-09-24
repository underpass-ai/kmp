"""Scenario vocabulary: synthetic fixtures, the measured journey and its obligations.

A scenario is data, not code: the capture driver loads its fixture through the
binary under test, runs its journey, and the oracle judges the result against
the obligations declared here. Nothing in a scenario is derived from either
variant's output.
"""
from dataclasses import asdict, dataclass, field
from enum import Enum
import hashlib
import json


class ObligationKind(str, Enum):
    # The text (a memory summary) appears in some wake.current_state line.
    STATE_MENTIONS = 'state_mentions'
    # Neither the text nor the canonical ref of `local_id` is shown as state.
    # Used for dimensional exclusion: requested dimensions bind context too.
    STATE_EXCLUDES = 'state_excludes'
    # A memory later than as_of is not presented as holding at as_of. When the
    # packet declares wake.current_state as time-unbounded context (scope), the
    # check moves to the declared selection (proof, causal spine); otherwise
    # the state surfaces must exclude it too.
    AS_OF_EXCLUDES = 'as_of_excludes'
    # proof.evidence holds an item whose text equals the stored body exactly.
    EVIDENCE_BODY = 'evidence_body'
    # At least `count` evidence items carry this body with distinct ids and sources.
    DISTINCT_PROVENANCE = 'distinct_provenance'
    # The measured write was accepted with a receipt and every local id mapped.
    WRITE_ACCEPTED = 'write_accepted'
    # The oracle's own read-back after the session finds the body persisted.
    READBACK_BODY = 'readback_body'


@dataclass(frozen=True)
class Obligation:
    kind: ObligationKind
    text: str | None = None
    local_id: str | None = None  # fixture local id whose canonical ref is checked
    count: int = 1

    def as_dict(self):
        value = asdict(self)
        value['kind'] = self.kind.value
        return value


@dataclass(frozen=True)
class FixtureWrite:
    """One kmp_write_memory call; `relations` may name earlier local ids as `@id`."""
    arguments: dict


@dataclass(frozen=True)
class JourneySpec:
    tool: str
    arguments: dict  # first request; the driver adds budget.max_bytes per budget
    budgets: tuple = (4096, 10000)  # None: the tool takes no projection budget


@dataclass(frozen=True)
class Scenario:
    case_id: str
    goal_id: str
    about: str
    description: str
    fixture: tuple
    journey: JourneySpec
    obligations: tuple
    unit_bodies: tuple  # stored bodies whose delivery counts as a usable unit
    stored_bodies: tuple  # every body the fixture stores (ref-field disguise check)
    readback: bool = False
    tags: tuple = field(default_factory=tuple)

    def as_dict(self):
        return {'case_id': self.case_id, 'goal_id': self.goal_id, 'about': self.about,
                'description': self.description,
                'fixture': [w.arguments for w in self.fixture],
                'journey': {'tool': self.journey.tool, 'arguments': self.journey.arguments,
                            'budgets': list(self.journey.budgets)},
                'obligations': [o.as_dict() for o in self.obligations],
                'unit_bodies': list(self.unit_bodies), 'stored_bodies': list(self.stored_bodies),
                'readback': self.readback}

    def digest(self):
        text = json.dumps(self.as_dict(), sort_keys=True, ensure_ascii=False, separators=(',', ':'))
        return hashlib.sha256(text.encode()).hexdigest()


def journey_id(case_id, budget):
    return case_id if budget is None else f'{case_id}-b{budget}'
