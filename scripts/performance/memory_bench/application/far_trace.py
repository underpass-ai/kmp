"""The far trace cut of the synth section: `trace_far` questions and their per-rung row.

A mode whose synth ladder has `[modes.<m>.synth.far_trace]` (`min_hops`, `pairs`) adds
`pairs` questions of type `trace_far` per topology (`domain/far_trace.py`): one
`kmp_trace` from one entry to another the world joins only through `min_hops` or
more declared hops, followed in their declared direction. They are part of the question set, so every rung traces the same
pairs, and they are scored like `path_between` (`path_found`, `step_precision`).

`rows(states)` is `scale.far_trace` of the report: per arm and run (one per rung),
the pairs traced, their `path_found` rate and the wall time of their `kmp_trace`
calls (ok calls of repeat 0; a timed-out call is counted apart, never averaged in).
The P11 objective is recorded next to each row: p95 at or under `TARGET_MS` at
`TARGET_LEVEL`; `within_target` is null at any other level.
"""
import json

from ..domain import stats
from ..domain.far_trace import declared_graph, far_pairs
from ..domain.question import Question
from ..generator.questions import gold

TYPE = 'trace_far'
RULE = 'trace_far.v1'
TARGET_MS = 30.0  # P11: a far trace answers in 30 ms at 10^5 (DISENO section 8, P11)
TARGET_LEVEL = 100000
TARGET_STATISTIC = 'wall_ms_p95'
MIN_HOPS_TAG = 'far_min_hops'


def records_below(loaded, section, level):
    """The records of `section` a `level` store holds (every rung above holds them too)."""
    limit = level // loaded.params.block_size
    return [json.loads(line) for line, block in zip(loaded.lines[section], loaded.blocks_of[section])
            if block < limit]


def question(pair, spec, seed, topology, number, min_level):
    path = {'from': pair.start, 'to': pair.end, 'steps': list(pair.steps), 'required': [],
            'accept_edges': [list(edge) for edge in pair.accept_edges],
            'accept_alternatives': True, 'directed': True}
    return Question.from_dict({
        'schema': 'kmp.bench.question.v1', 'id': f'synth{seed}-{topology}-far-{number:02d}',
        'corpus': 'synth-v1', 'type': TYPE, 'about': pair.about, 'tool': 'kmp_trace',
        'question': None, 'answer_policy': None, 'anchors': [],
        'gold': gold('KNOWN', path=path), 'tags': [f'hops:{pair.hops}', f'{MIN_HOPS_TAG}:{spec.min_hops}'], 'private': False,
        'min_level': min_level, 'arguments': {'from': pair.start, 'to': pair.end},
        'source': {'kind': 'bench', 'rule': RULE, 'hops': pair.hops}})


def questions(loaded, ladder, topology):
    """The `trace_far` questions of a ladder on a world loaded at (at least) its lowest level."""
    spec = ladder.far_trace
    if spec is None:
        return ()
    level = ladder.levels[0]
    nodes = {entry['ref'] for entry in records_below(loaded, 'entries', level)}
    graph = declared_graph(records_below(loaded, 'relations', level), nodes)
    pairs = far_pairs(graph, spec, ladder.seed)
    return tuple(question(pair, spec, ladder.seed, topology, number, loaded.params.block_size)
                 for number, pair in enumerate(pairs, 1))


def _tag(tag, key):
    name, sep, value = tag.partition(':')
    return int(value) if sep and name == key and value.isdigit() else None


def _quantile(values, q):
    return None if not values else stats.quantile(values, q)


def row(arm, item, scores):
    """One rung of one arm: `item` is a report ArmRun, `scores` its QuestionScores."""
    far = [s for s in scores if s.type == TYPE]
    if not far:
        return None
    ids = {s.question_id for s in far}
    calls = [c for c in item.run.calls if c.question_id in ids and c.repeat == 0 and c.tool == 'kmp_trace']
    wall = [c.wall_ns / 1e6 for c in calls if c.status == 'ok' and c.wall_ns is not None and not c.censored]
    found = [s.values['path_found'] for s in far if 'path_found' in s.values]
    hops = sorted({_tag(tag, 'hops') for s in far for tag in s.tags} - {None})
    floor = sorted({_tag(tag, MIN_HOPS_TAG) for s in far for tag in s.tags} - {None})
    p95 = _quantile(wall, 0.95)
    return {'arm': arm, 'run_id': item.run.run_id, 'level': item.level, 'topology': item.topology,
            'pairs': len(ids), 'min_hops': floor[0] if floor else None, 'hops': hops,
            'path_found': {'value': sum(found) / len(found) if found else None, 'n': len(found)},
            'calls': len(calls), 'censored': sum(1 for c in calls if c.censored),
            'wall_ms_p50': _quantile(wall, 0.5), 'wall_ms_p95': p95,
            'target': {'statistic': TARGET_STATISTIC, 'max_ms': TARGET_MS, 'level': TARGET_LEVEL},
            'within_target': (p95 <= TARGET_MS if p95 is not None else None)
            if item.level == TARGET_LEVEL else None}


def rows(states):
    """`scale.far_trace`: every arm's rungs that traced far pairs, in run order."""
    out = []
    for state in states:
        for item in state.arm.runs:
            found = row(state.arm.name, item, state.scores[item.run.run_id])
            if found is not None:
                out.append(found)
    return out
