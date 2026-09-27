"""report.json (`kmp.bench.report.v1`, SCHEMAS.md section 5): the only source of numbers.

`build_report` scores one arm (a baseline alone) or two (baseline and candidate,
paired), and fills the eleven sections of BENCH_SPEC section 12 in order. Each arm
is a primary run (the paired one) plus optional extra runs of the same variant: a
`max_bytes` sweep (the quality-tokens curve) or other ladder levels (headlines per
level and the scale exponent).

The report is deterministic: no clock, no host path, every float from a
deterministic computation (bootstrap seeds are fixed), canonical JSON on disk.
Private corpora appear only as aggregates: no `per_question` row, no parity
difference that would name a question.
"""
from dataclasses import dataclass, field
import hashlib
import json
from pathlib import Path
import platform

from .. import BENCH_VERSION
from ..domain import cachekey, jsonl, power, scoring_rules, stats
from ..domain.metric_catalog import BY_NAME
from ..domain.question import PRIVATE_CORPORA, questions_digest
from . import aggregate, controls, far_trace, jev_cost, latency
from .compare import Pairing, delta, drift
from .score import ScoringRefused, score_run
from .tokens import ENCODINGS, PRIMARY_ENCODING, measure_run, unavailable
from .verdict import cost_summary, decide, guard_row, target_row

SCHEMA = 'kmp.bench.report.v1'
PACKAGE = Path(__file__).resolve().parents[1]
CONFIG = PACKAGE / 'config'
HEADLINE = ('useful_rate', 'answer_correct_rate', 'partial_useful_rate', 'false_unknown_rate',
            'false_answer_rate', 'reason_accuracy', 'facet_coverage', 'recall_at_5', 'core_precision',
            'stale_serve_rate', 'future_leak_rate', 'as_of_accuracy', 'path_found',
            'wake_obligations_met', 'censored_rate')
STRATA_KEYS = ('bridge', 'form', 'degree', 'abouts', 'supersession', 'interval', 'k', 'hops',
               'undeclared', 'lang')
STRATA_METRICS = ('useful_rate', 'false_answer_rate', 'recall_at_5', 'full_chain_recovery_at_5',
                  'path_found', 'stale_serve_rate', 'as_of_accuracy')
TOKEN_FIELDS = ('journey', 'first_page', 'to_first_evidence', 'to_task_ready')
REFERENCE_D = (0.08, 0.30)  # BENCH_SPEC section 11 table
B_REAL_MIN = 60
JEV_NO_COMPARABLE = 'JEV_NO_COMPARABLE'  # run_questions: a Jev sample the fixture could not answer


def jev_samples(run):
    """The Jev fixture outcomes `run_questions` sealed into run.json (empty without a fixture)."""
    jev = (run.manifest.get('isolation') or {}).get('jev') or {}
    return list(jev.get('samples') or ())


@dataclass(frozen=True)
class ArmRun:
    run: object  # run_data.RunData
    level: int | None = None
    topology: str | None = None


@dataclass(frozen=True)
class Arm:
    name: str  # 'baseline' | 'candidate'
    runs: tuple  # ArmRun; runs[0] is the paired one
    variant: object = None  # domain.variant.Variant (pre-registration)

    @property
    def primary(self):
        return self.runs[0].run


@dataclass(frozen=True)
class ReportOptions:
    mode: str
    bootstrap_b: int | None = stats.BOOTSTRAP_B  # paired deltas of targets, guards, cost, headline means
    latency_b: int | None = None  # bootstrap of latency quantiles (None: point values only)
    determinism: object = None  # path of a determinism.py report
    calibration: dict | None = None  # synth-v1 world manifest `calibration`
    extra_limitations: tuple = ()
    rules: dict = field(default_factory=dict)  # {modes_sha256: ...} when a mode file pins them
    freeze: dict | None = None  # real_section.freeze_identity of the B-real freeze the section read

    def key_material(self):
        return {'mode': self.mode, 'bootstrap_b': self.bootstrap_b, 'latency_b': self.latency_b,
                'determinism': None if self.determinism is None else Path(self.determinism).name,
                'freeze': self.freeze}


@dataclass
class _ArmState:
    arm: Arm
    tokens: dict  # run_id -> RunTokens
    scores: dict  # run_id -> tuple of QuestionScore

    @property
    def primary_scores(self):
        return self.scores[self.arm.primary.run_id]


def code_sha256():
    digest = hashlib.sha256()
    for path in sorted(PACKAGE.rglob('*.py')):
        relative = path.relative_to(PACKAGE).as_posix()
        if not relative.startswith('tests/'):
            digest.update(relative.encode() + b'\0' + path.read_bytes() + b'\0')
    return digest.hexdigest()


def _pinned_sha(name):
    path = CONFIG / name
    if not path.is_file():
        return None
    return path.read_text(encoding='utf-8').split()[0]


def infer_topology(questions):
    """`mono` / `multi` for a synth-v1 question set on one topology, else None."""
    kinds = {q.about.split('-')[0] for q in questions if q.about.startswith('synth:')}
    names = {'synth:mono': 'mono', 'synth:multi': 'multi'}
    return names.get(kinds.pop()) if len(kinds) == 1 else None


def _check_questions(run, digest):
    if run.manifest['questions']['digest'] != digest:
        raise ScoringRefused(f'run {run.run_id} was measured on another question set '
                             f'({run.manifest["questions"]["digest"]}, not {digest})')


def _prepare(arm, questions, counters, tokens_reason, digest):
    tokens, scores = {}, {}
    for item in arm.runs:
        _check_questions(item.run, digest)
        if counters:
            counted = measure_run(item.run, counters)
        else:
            counted = unavailable(tokens_reason or unavailable().absent_reason)
        tokens[item.run.run_id] = counted
        scores[item.run.run_id] = score_run(item.run, questions, counted)
    return _ArmState(arm, tokens, scores)


# --- sections -----------------------------------------------------------------------------

def _provenance(states, questions, digest, comparable, differing, options):
    def arm_row(state):
        run = state.arm.primary
        manifest, variant = run.manifest, state.arm.variant
        return {'variant': run.variant, 'run_ids': [item.run.run_id for item in state.arm.runs],
                'binary_sha256': manifest['binary']['sha256'],
                'binary_version': manifest['binary'].get('version'),
                'config_digest': manifest['variant']['config_digest'],
                'preregistration_digest': (variant.preregistration_digest() if variant is not None
                                           else manifest['variant']['preregistration_digest']),
                'store': {'key': manifest['store']['key'],
                          'content_digest': manifest['store'].get('content_digest')},
                'samples': manifest['samples'], 'repeats': manifest['repeats'],
                'max_calls': manifest['max_calls'], 'max_bytes': [item.run.max_bytes for item in state.arm.runs]}

    by_corpus = {}
    for question in questions:
        by_corpus[question.corpus] = by_corpus.get(question.corpus, 0) + 1
    base = states[0].arm.primary.manifest
    return {'baseline': arm_row(states[0]), 'candidate': arm_row(states[1]) if len(states) > 1 else None,
            'questions': {'digest': digest, 'by_corpus': dict(sorted(by_corpus.items()))},
            'freeze': options.freeze,
            'driver_version': base['driver_version'],
            'encoders': [item['encoding'] for item in base['encoders']],
            'rules': {'scoring_rules_sha256': scoring_rules.rules_sha256(),
                      'negatives_sha256': _pinned_sha('negatives.sha256'),
                      'modes_sha256': options.rules.get('modes_sha256', _pinned_sha('modes.sha256'))},
            'comparable': comparable, 'drift': list(differing)}


def _headlines(states, b):
    rows = []
    for state in states:
        for item in state.arm.runs:
            scores = state.scores[item.run.run_id]
            table = {name: aggregate.compute(scores, name, b if BY_NAME[name].kind == 'mean' else None)
                     for name in HEADLINE}
            confidence = aggregate.confidence_table(scores)
            per_useful, reason = aggregate.tokens_per_useful(scores)
            rows.append({'level': item.level, 'topology': item.topology, 'arm': state.arm.name,
                         'run_id': item.run.run_id, 'max_bytes': item.run.max_bytes,
                         'metrics': table,
                         'false_unknown': aggregate.false_unknown_both(scores),
                         'high_precision': confidence['high_precision'],
                         'high_certified': confidence['high_certified'],
                         'tokens_per_useful': {'value': per_useful, 'encoding': PRIMARY_ENCODING,
                                               'absent_reason': reason},
                         'errors': sum(1 for s in scores if s.error)})
    return rows


def _strata(scores):
    groups = {}
    for score in scores:
        for tag in score.tags:
            key, sep, _ = tag.partition(':')
            if sep and key in STRATA_KEYS:
                groups.setdefault(tag, []).append(score)
    return {tag: aggregate.summarize(members, names=STRATA_METRICS) for tag, members in sorted(groups.items())}


def _by_type(states, pairing, types):
    table = {}
    for kind in types:
        entry = {'n': 0, 'metrics': {}, 'deltas': {}, 'per_question': {}}
        for state in states:
            scores = [s for s in state.primary_scores if s.type == kind]
            entry['n'] = max(entry['n'], len({s.question_id for s in scores}))
            entry['metrics'][state.arm.name] = aggregate.summarize(scores)
            entry.setdefault('outcomes', {})[state.arm.name] = aggregate.outcomes_table(scores)
            entry.setdefault('reasons', {})[state.arm.name] = aggregate.reasons_table(scores)
            entry.setdefault('confidence', {})[state.arm.name] = aggregate.confidence_table(scores)
            entry.setdefault('scorecard', {})[state.arm.name] = aggregate.scorecard_port(scores)
            entry.setdefault('strata', {})[state.arm.name] = _strata(scores)
            entry['per_question'][state.arm.name] = [s.as_row() for s in scores if not s.private]
        if pairing is not None:
            typed = pairing.restricted(lambda s, kind=kind: s.type == kind)
            names = sorted(set().union(*(entry['metrics'][s.arm.name] for s in states)))
            entry['deltas'] = {name: delta(typed, name, b=None) for name in names}
        table[kind] = entry
    return table


def _tokens(states, types, reason):
    encodings = [e for e in ENCODINGS if any(e in t.encodings for s in states for t in s.tokens.values())]
    section = {'encoders': encodings, 'representation': 'json_compact_lexical_v1',
               'absent_reason': None if encodings else reason, 'by_type': {}, 'arms': {}, 'curve': {}}
    for kind in types:
        section['by_type'][kind] = {}
        for state in states:
            scores = [s for s in state.primary_scores if s.type == kind]
            section['by_type'][kind][state.arm.name] = {
                encoding: {name: aggregate.compute(scores, 'tokens_' + name, encoding=encoding,
                                                   reason='not counted') for name in TOKEN_FIELDS}
                for encoding in encodings}
    for state in states:
        scores, run = state.primary_scores, state.arm.primary
        counted = state.tokens[run.run_id]
        journeys = len(scores)
        tools = sorted({s.tool for s in scores})
        section['arms'][state.arm.name] = {
            'startup_amortized': {e: counted.startup_amortized(e, journeys) for e in encodings},
            'tokens_per_useful': {e: dict(zip(('value', 'absent_reason'), aggregate.tokens_per_useful(scores, e)))
                                  for e in encodings},
            'agent_load': {tool: aggregate.agent_load([s for s in scores if s.tool == tool]) for tool in tools},
            'wake_load': aggregate.agent_load([s for s in scores if s.tool == 'kmp_wake'])}
        section['curve'][state.arm.name] = _curve(state)
    return section


def _curve(state):
    points = []
    for item in state.arm.runs:
        scores = state.scores[item.run.run_id]
        per_useful, reason = aggregate.tokens_per_useful(scores)
        points.append({'max_bytes': item.run.max_bytes, 'level': item.level, 'run_id': item.run.run_id,
                       'useful_rate': aggregate.compute(scores, 'useful_rate'),
                       'tokens_journey': aggregate.compute(scores, 'tokens_journey', reason='not counted'),
                       'tokens_per_useful': per_useful, 'tokens_per_useful_absent_reason': reason})
    return sorted(points, key=lambda p: (p['level'] or 0, p['max_bytes'] or 0, p['run_id']))


def _scale(states, calibration):
    exponents = []
    for state in states:
        by_topology = {}
        for item in state.arm.runs:
            if item.level is not None:
                by_topology.setdefault(item.topology, []).extend(latency.warm_ask_points(item.run, item.level))
        for topology, points in sorted(by_topology.items(), key=lambda kv: str(kv[0])):
            fit = latency.scale_exponent(points)
            if fit is not None:
                exponents.append({'arm': state.arm.name, 'topology': topology, **fit})
    return {'exponents': exponents, 'calibration': calibration, 'far_trace': far_trace.rows(states)}


def _latency(states, b):
    rows, startup = [], {}
    for state in states:
        section = latency.section(state.arm.primary, b)
        rows.extend({'arm': state.arm.name, **row} for row in section['by_tool_phase'])
        startup[state.arm.name] = section['startup_ms']
    return {'by_tool_phase': rows, 'startup_ms': startup}


def _jev(states, pairing, deltas):
    arms = {state.arm.name: jev_cost.jev_by_site(state.arm.primary) for state in states}
    section = {'price_usd_per_mtok': jev_cost.PRICE_USD_PER_MTOK, 'arms': arms,
               'delta_usd': None, 'marginal_cost_per_useful': None, 'marginal_absent_reason': None}
    if pairing is None:
        section['marginal_absent_reason'] = 'single arm'
        return section
    usd = deltas.get('jev_usd')
    useful = deltas.get('useful_rate')
    if usd is None or usd['delta'] is None:
        section['marginal_absent_reason'] = 'Jev telemetry missing on an arm'
        return section
    section['delta_usd'] = usd['delta'] * len(pairing.keys)
    gained = None if useful is None or useful['delta'] is None else useful['delta'] * len(pairing.keys)
    if gained and gained > 0:
        section['marginal_cost_per_useful'] = section['delta_usd'] / gained
    else:
        section['marginal_absent_reason'] = 'no useful answer gained'
    return section


def _power(states, pairing, rows):
    table = []
    for row in rows:
        entry = {'metric': row['metric'], 'n': None, 'discordant_rate': None, 'mde': row.get('mde'),
                 'effect': row.get('effect'), 'decidable': bool(row.get('decidable'))}
        if row.get('discordant') is not None:
            entry['discordant_rate'] = row['discordant']['rate']
        base = row.get('baseline') or {}
        entry['n'] = base.get('n')
        table.append(entry)
    listed = {row['metric'] for row in table}
    if pairing is None:
        scores = states[0].primary_scores
        for name in HEADLINE:
            n = aggregate.compute(scores, name)['n']
            if n and BY_NAME[name].kind == 'rate':
                table.append({'metric': name, 'n': n, 'discordant_rate': None, 'mde': None, 'effect': None,
                              'decidable': False,
                              'mde_at_reference_d': {str(d): power.mde(n, d) for d in REFERENCE_D}})
        return table
    for name in HEADLINE:
        if name in listed:
            continue
        row = delta(pairing, name, b=None)
        if row['discordant'] is not None:
            table.append({'metric': name, 'n': row['baseline']['n'],
                          'discordant_rate': row['discordant']['rate'], 'mde': row['mde'],
                          'effect': None, 'decidable': False})
    return table


def _limitations(states, questions, tokens_reason, options):
    notes = list(options.extra_limitations)
    real = sum(1 for q in questions if q.corpus == 'b-real')
    if real and real < B_REAL_MIN:
        notes.append(f'B-real has {real} questions, below the {B_REAL_MIN} of phase A')
    if not real:
        notes.append('B-real not in this question set')
    if tokens_reason:
        notes.append(f'tokens not counted: {tokens_reason}')
    for state in states:
        reason = jev_cost.missing_reason(state.arm.primary)
        if reason:
            notes.append(f'{state.arm.name}: Jev cost unknown ({reason})')
        unverified = [s for s in jev_samples(state.arm.primary) if s.get('status') == 'unverified']
        if unverified:
            notes.append(f'{state.arm.name}: Jev replay unverified ({unverified[0].get("reason")})')
        if any(s.private for s in state.primary_scores):
            notes.append(f'{state.arm.name}: private corpora reported as aggregates only')
    if len(states) < 2:
        notes.append('single arm: no paired comparison')
    notes.append('Rust ranker guard tests (answer_ranker.rs) are checked by CI, not by this report')
    return sorted(set(notes))


def _capture_failures(states):
    reasons = []
    for state in states:
        run = state.arm.primary
        for failure in run.manifest.get('failures') or []:
            if failure.get('code') == JEV_NO_COMPARABLE:
                continue  # a verdict of its own (no_comparable), not a failed capture
            reasons.append(f'{state.arm.name}: {failure.get("code")}')
        errors = [s for s in state.primary_scores if s.error]
        if errors:
            statuses = sorted({s.status for s in errors})
            reasons.append(f'{state.arm.name}: {len(errors)} journey(s) not scored ({", ".join(statuses)})')
    return sorted(set(reasons))


def _target_rows(variant, pairing, b, parity):
    rows = []
    for target in (variant.targets if variant is not None else ()):
        if target.metric == 'parity_rate':
            rate = None if parity is None else parity.get('parity_rate')
            rows.append({'metric': 'parity_rate', 'baseline': None, 'candidate': None,
                         'delta': None if rate is None else rate - 1.0, 'ci95': None, 'method': None,
                         'p_value': None, 'discordant': None, 'mde': None, 'effect': None,
                         'decidable': rate is not None, 'status': None})
            continue
        try:
            row = delta(pairing, target.metric, target.effect, b)
        except KeyError:
            row = {'metric': target.metric, 'delta': None, 'ci95': None, 'mde': None,
                   'effect': target.effect, 'decidable': False,
                   'baseline': aggregate.absent('metric unknown to the bench catalog'),
                   'candidate': aggregate.absent('metric unknown to the bench catalog')}
        rows.append(target_row(row))
    return rows


def _guard_rows(variant, pairing, b):
    rows = []
    for guard in (variant.guards if variant is not None else ()):
        try:
            rows.append(guard_row(delta(pairing, guard.metric, None, b), guard.margin))
        except KeyError:
            rows.append({'metric': guard.metric, 'margin': guard.margin, 'broken': False,
                         'delta': None, 'worsening': None,
                         'absent_reason': 'metric unknown to the bench catalog'})
    return rows


def build_report(baseline, candidate, questions, counters=(), options=None, tokens_reason=None):
    """The report dict. `counters`: token_harness counters (empty: tokens absent, `tokens_reason`)."""
    options = options or ReportOptions(mode='adhoc')
    questions = tuple(questions)
    digest = questions_digest(questions)
    types = list(dict.fromkeys(q.type for q in questions))
    if not counters and tokens_reason is None:
        tokens_reason = unavailable().absent_reason
    arms = [baseline] + ([candidate] if candidate is not None else [])
    states = [_prepare(arm, questions, counters, tokens_reason, digest) for arm in arms]
    differing = drift(baseline.primary, candidate.primary) if candidate is not None else ()
    pairing = None if candidate is None else Pairing.of(states[0].primary_scores, states[1].primary_scores)
    b = options.bootstrap_b
    parity = controls.parity_between(baseline.primary, candidate.primary) if candidate is not None else None
    variant = candidate.variant if candidate is not None else baseline.variant
    deltas, targets, guards, cost = {}, [], [], None
    if pairing is not None and not differing:
        targets = _target_rows(variant, pairing, b, parity)
        guards = _guard_rows(variant, pairing, b)
        if counters:
            deltas['tokens_journey'] = delta(pairing, 'tokens_journey', b=b)
        usd = [jev_cost.per_question_usd(state.arm.primary) for state in states]
        if None not in usd:
            deltas['jev_usd'] = delta(pairing, 'jev_usd', b=b, extra=tuple(usd))
        deltas['useful_rate'] = delta(pairing, 'useful_rate', b=None)
        cost = cost_summary(deltas.get('tokens_journey'), deltas.get('jev_usd'))
    same_arm = (candidate is not None and baseline.primary.manifest['key']['binary_sha256']
                == candidate.primary.manifest['key']['binary_sha256']
                and baseline.primary.manifest['key']['config_digest']
                == candidate.primary.manifest['key']['config_digest'])
    determinism = None
    if options.determinism is not None:
        binaries = {state.arm.primary.manifest['binary']['sha256'] for state in states}
        determinism = controls.determinism_summary(options.determinism, binaries)
    controls_section = {
        'aa': controls.aa(pairing) if same_arm and pairing is not None and not differing else None,
        'determinism_rate': None if determinism is None else determinism['determinism_rate'],
        'determinism': determinism, 'parity': parity,
        'repeat': {state.arm.name: controls.repeat_consistency(state.arm.primary) for state in states},
        'unpaired_questions': None if pairing is None else pairing.unpaired}
    misses = sum(jev_cost.cassette_misses(state.arm.primary) for state in states
                 if (state.arm.variant is None or state.arm.variant.jev == 'replay'))
    misses += sum(1 for state in states for sample in jev_samples(state.arm.primary)
                  if sample.get('status') == 'no_comparable')
    verdict = decide(variant.claim if variant is not None else None, targets, guards, cost,
                     single_arm=candidate is None, drift=differing,
                     capture_failures=_capture_failures(states), replay_misses=misses, parity=parity)
    power_rows = _power(states, pairing, targets + [{**g, 'mde': None, 'effect': None} for g in guards])
    report = {
        'schema': SCHEMA, 'bench_version': BENCH_VERSION,
        'report_key': cachekey.report_key(
            result_keys=[item.run.run_id for arm in arms for item in arm.runs],
            options={**options.key_material(),
                     'encoders': sorted(c.identity.encoding for c in counters)}),
        'mode': options.mode,
        'generated_by': {'code_sha256': code_sha256(), 'python': platform.python_version()},
        'provenance': _provenance(states, questions, digest, not differing, differing, options),
        'headlines': _headlines(states, b),
        'by_type': _by_type(states, pairing if not differing else None, types),
        'scale': _scale(states, options.calibration),
        'latency_resources': _latency(states, options.latency_b),
        'tokens': _tokens(states, types, tokens_reason),
        'jev_by_site': _jev(states, pairing, deltas),
        'controls': controls_section,
        'power': power_rows,
        'limitations': _limitations(states, questions, tokens_reason if not counters else None, options),
        'verdict': {**verdict, 'deltas': deltas},
    }
    return json.loads(dump(report))


def dump(report):
    """report.json text: compact UTF-8 JSON, no NaN, keys in the order SCHEMAS.md section 5 gives
    (the sections in BENCH_SPEC order), which a deterministic build makes byte-stable."""
    cachekey.canonical_json(report)  # refuses non-JSON values and non-finite numbers
    return json.dumps(report, ensure_ascii=False, allow_nan=False, separators=(',', ':'))


def write_report(report, layout, markdown=None):
    """Write report.json (and report.md) under `reports/<report_key>/` of the layout.

    A report over a private corpus holds aggregates only, but it still names private
    material (store digests, question-set digests): it goes to the private layout."""
    if not layout.private and mentions_private(report):
        raise ScoringRefused('a report over private corpora belongs in the private layout')
    directory = layout.report(report['report_key'])
    paths = [jsonl.write_text(directory / 'report.json', dump(report) + '\n', overwrite=True)]
    if markdown is not None:
        paths.append(jsonl.write_text(directory / 'report.md', markdown, overwrite=True))
    return paths


def mentions_private(report):
    corpora = report['provenance']['questions']['by_corpus']
    return any(name in PRIVATE_CORPORA for name in corpora)
