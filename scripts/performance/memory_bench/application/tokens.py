"""Agent tokens of a run, counted by token_harness `measure` itself (BENCH_SPEC section 7).

Nothing is re-tokenized here: every journey trace goes through token_harness'
`measure_journey` with the pinned tiktoken 0.14.0 counters (o200k_base and
cl100k_base), and the bench reads its totals and per-exposure rows. So a journey
total is, by construction, the `totals[encoding][json_compact_lexical_v1].all.tokens`
that `python3 -m scripts.performance.token_harness measure --run DIR` prints for
the same trace.

Per journey the bench keeps the tokens of each `tools/call` exchange (request and
response, in order), which yields the first page, the tokens up to any call (first
judged evidence, task ready) and the whole journey. The startup catalogue lives in
`process-<n>.jsonl`, never in a journey; it is measured the same way and reported
amortized over the journeys the run measured.

Without tiktoken (plain `python3`), `unavailable(reason)` stands in: every token
figure is then null with that reason, never an invented zero.
"""
from dataclasses import dataclass, field
import os
from pathlib import Path

from ...token_harness.adapters import tiktoken_counter
from ...token_harness.application.measure import measure_journey
from ...token_harness.domain.accounting import DEFAULT_MAX_UNIT_BYTES
from ...token_harness.domain.errors import HarnessError
from ...token_harness.domain.representation import RepresentationId
from ..domain.jsonl import REPO_ROOT

PRIMARY_REPRESENTATION = RepresentationId.JSON_COMPACT_LEXICAL_V1  # token_harness compare's primary
PRIMARY_ENCODING = 'o200k_base'
ENCODINGS = tuple(tiktoken_counter.PINNED_ASSETS)
CACHE_CANDIDATES = ('tmp/tiktoken-cache', 'tmp/memory-bench/tiktoken-cache')
NO_TIKTOKEN = 'tiktoken==0.14.0 not loaded (run under `uv run --no-project --with tiktoken==0.14.0`)'


@dataclass(frozen=True)
class CallTokens:
    event_index: int
    request: int | None
    response: int | None

    @property
    def total(self):
        return None if self.request is None or self.response is None else self.request + self.response


@dataclass(frozen=True)
class JourneyTokens:
    journey: str
    encoding: str
    total: int | None  # token_harness journey total (requests and responses)
    utf8_bytes: int | None
    calls: tuple = ()  # CallTokens of each tools/call exchange, in trace order
    absent: dict = field(default_factory=dict)

    def through(self, count):
        """Tokens of the first `count` tool calls; None when any of them was not counted."""
        if count < 1 or count > len(self.calls):
            return None
        totals = [call.total for call in self.calls[:count]]
        return None if None in totals else sum(totals)


@dataclass(frozen=True)
class RunTokens:
    representation: str
    encodings: tuple
    journeys: dict  # (encoding, journey) -> JourneyTokens
    startup: dict  # encoding -> tokens of every handshake trace
    absent_reason: str | None = None

    def of(self, journey, encoding=PRIMARY_ENCODING):
        return self.journeys.get((encoding, journey))

    def startup_amortized(self, encoding, journeys):
        total = self.startup.get(encoding)
        return None if total is None or journeys <= 0 else total / journeys


def unavailable(reason=NO_TIKTOKEN):
    return RunTokens(PRIMARY_REPRESENTATION.value, (), {}, {}, reason)


def default_cache_dir(repo_root=REPO_ROOT):
    """TIKTOKEN_CACHE_DIR, else the first existing bench/token_harness cache."""
    configured = os.environ.get('TIKTOKEN_CACHE_DIR')
    if configured:
        return Path(configured)
    for candidate in CACHE_CANDIDATES:
        if (Path(repo_root) / candidate).is_dir():
            return Path(repo_root) / candidate
    return Path(repo_root) / CACHE_CANDIDATES[0]


def load_counters(cache_dir=None, encodings=ENCODINGS):
    """Pinned, offline tiktoken counters (token_harness adapter); raises TokenizerNotReady."""
    tiktoken_counter.configure_cache(cache_dir or default_cache_dir())
    return [tiktoken_counter.load_counter(name) for name in encodings]


def try_load_counters(cache_dir=None, encodings=ENCODINGS):
    """(counters, None) or ([], reason): a report without tiktoken says why."""
    try:
        return load_counters(cache_dir, encodings), None
    except HarnessError as error:
        return [], f'{NO_TIKTOKEN}: {error}'


def _journey_tokens(measured, encoding):
    representation = PRIMARY_REPRESENTATION.value
    total = measured['totals'][encoding][representation]['all']
    exposures = {e['exposure_id']: e for e in measured['exposures']}
    per_event = {}
    for row in measured['measurements']:
        if row['encoding'] != encoding or row['representation'] != representation:
            continue
        exposure = exposures[row['exposure_id']]
        if exposure['method'] != 'tools/call' or exposure['kind'] not in ('request', 'response'):
            continue
        per_event.setdefault(exposure['event_index'], {})[exposure['kind']] = row['tokens']
    calls = tuple(CallTokens(index, sides.get('request'), sides.get('response'))
                  for index, sides in sorted(per_event.items()))
    return JourneyTokens(measured['journey'], encoding, total['tokens'] if total['units'] else None,
                         total['utf8_bytes'] if total['units'] else None, calls, dict(total['absent']))


def measure_run(run, counters, max_unit_bytes=DEFAULT_MAX_UNIT_BYTES):
    """Count every measured journey and handshake of a run (see the module docstring)."""
    if not counters:
        return unavailable()
    representations = [PRIMARY_REPRESENTATION]
    journeys = {}
    for trace in run.traces:
        measured = measure_journey(trace, counters, representations, max_unit_bytes)
        for counter in counters:
            encoding = counter.identity.encoding
            journeys[(encoding, trace.journey)] = _journey_tokens(measured, encoding)
    startup = {counter.identity.encoding: 0 for counter in counters}
    for trace in run.process_traces():
        measured = measure_journey(trace, counters, representations, max_unit_bytes)
        for counter in counters:
            encoding = counter.identity.encoding
            startup[encoding] += measured['totals'][encoding][PRIMARY_REPRESENTATION.value]['all']['tokens']
    return RunTokens(PRIMARY_REPRESENTATION.value, tuple(c.identity.encoding for c in counters),
                     journeys, startup)
