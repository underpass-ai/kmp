"""One section of a mode: a question set on a store, both arms, one report.

A section is the unit a report can hold: one question set measured on one store
by the baseline and the candidate (`run_questions.run_arms`, ABAB in blocks), then
scored and compared by `report.build_report` and written as `report.json` and
`report.md` under `reports/<report_key>/` of its layout (the private one when a
corpus is private). The mode fixes the measurement parameters; a cached run
measured under other parameters is refused (`Mode.check_run`).

An A/A section (`replica_nonce`) runs the baseline as usual (a cache hit when it
was measured before) and then the same arm again under a nonce, so the replica
is always a fresh measurement.

`SectionResult` is what the mode summary keeps of a section: statuses, keys,
verdict, a few headline aggregates, parity and wall time. It names no question.
"""
from dataclasses import dataclass, field, replace
import gzip
import json
import time

from ..domain.errors import BenchError
from . import markdown, report as reports, tokens
from .arms import arm_on
from .run_data import load_run
from .run_questions import RunPlan, run_arms

HEADLINE_METRICS = ('useful_rate', 'false_answer_rate', 'false_unknown_rate', 'recall_at_5',
                    'core_precision', 'reason_accuracy')
RAN, SKIPPED, FAILED = 'ran', 'skipped', 'failed'


@dataclass
class SectionResult:
    name: str
    status: str
    reason: str | None = None
    layout: str | None = None  # public | private
    report_key: str | None = None
    report_dir: object = None  # Path of reports/<key>/ (kept out of the summary file)
    runs: dict = field(default_factory=dict)  # arm -> {run_id, cached}
    questions: dict = field(default_factory=dict)
    verdict: str | None = None
    reasons: list = field(default_factory=list)
    applicable: bool = True  # a target or guard of the claim was measured here
    headline: dict = field(default_factory=dict)
    parity: dict | None = None
    aa: dict | None = None
    limitations: list = field(default_factory=list)
    seconds: float = 0.0
    judged: list | None = None  # judged corpora: one comparison per corpus arm (judged_section)
    freeze: dict | None = None  # the B-real freeze the section read (real_section.freeze_identity)
    public: list | None = None  # public benchmarks: one row per corpus (public_section)

    def as_dict(self):
        return {'name': self.name, 'status': self.status, 'reason': self.reason, 'layout': self.layout,
                'report_key': self.report_key, 'runs': self.runs, 'questions': self.questions,
                'verdict': self.verdict, 'reasons': self.reasons, 'applicable': self.applicable,
                'headline': self.headline, 'parity': self.parity, 'aa': self.aa,
                'limitations': self.limitations, 'judged': self.judged, 'public': self.public,
                'freeze': self.freeze,
                'seconds': round(self.seconds, 3)}


def skipped(name, reason):
    return SectionResult(name, SKIPPED, reason)


def failed(name, error, seconds=0.0):
    detail = getattr(error, 'detail', None) or str(error)
    code = getattr(error, 'code', type(error).__name__)
    return SectionResult(name, FAILED, f'{code}: {detail}', verdict='captura_fallida',
                         reasons=[f'{code}: {detail}'], seconds=seconds)


def run_mode(mode, max_bytes=None):
    """The mode name in the result key; a sweep point is its own mode (`full-mb2048`), since
    max_bytes is not a key part and the sweep run would otherwise be the primary's cache hit."""
    return mode.run_mode if max_bytes is None else f'{mode.run_mode}-mb{max_bytes}'


def plan_for(mode, questions, max_bytes=None, nonce=None):
    return RunPlan(tuple(questions), run_mode(mode, max_bytes), samples=mode.samples, repeats=mode.repeats,
                   max_calls=mode.max_calls, timeout_s=mode.timeout_s,
                   max_bytes=max_bytes if max_bytes is not None else mode.max_bytes,
                   block=mode.block, warmup=mode.warmup, cpus=mode.cpus, nonce=nonce)


def measure(specs, stores, questions, mode, layout, max_bytes=None, replica_nonce=None):
    """[baseline ArmResult, candidate ArmResult]; both checked against the mode's parameters."""
    base, cand = specs
    plan = plan_for(mode, questions, max_bytes)
    base_arm = arm_on(base, stores[0], layout, questions)
    if replica_nonce is None:
        results = run_arms([base_arm, arm_on(cand, stores[1], layout, questions)], plan, layout)
    else:
        results = run_arms([base_arm], plan, layout)
        replica = arm_on(cand, stores[1], layout, questions)
        results += run_arms([replica], replace(plan, nonce=replica_nonce), layout)
    for result in results:
        mode.check_run(run_manifest(result), max_bytes)
    return results


def run_manifest(result):
    """run.json of an arm's run; a cache hit's ArmResult carries token_harness' manifest instead."""
    return json.loads(gzip.decompress((result.run_dir / 'run.json.gz').read_bytes()))


def applicable(verdict):
    """Does this section vote on the mode's verdict?

    It does when it failed, was not comparable or regressed (a broken guard counts
    anywhere), and otherwise only when it measured a target of the claim: a section
    whose questions never exercise the target (no wake question for a wake claim)
    says `indecidible` by construction and must not veto the sections that did."""
    if verdict['value'] in ('captura_fallida', 'no_comparable', 'regresion'):
        return True
    for row in verdict.get('targets') or ():
        if row.get('metric') == 'parity_rate':
            if row.get('decidable'):
                return True
        elif ((row.get('baseline') or {}).get('n') or 0) > 0:
            return True
    return False


def _headline(report):
    rows = {}
    for row in report['headlines']:
        arm = row['arm']
        if arm in rows:
            continue  # the primary run of each arm comes first
        rows[arm] = {name: (row['metrics'].get(name) or {}).get('value') for name in HEADLINE_METRICS}
        rows[arm]['n'] = max(((row['metrics'].get(name) or {}).get('n') or 0) for name in HEADLINE_METRICS)
    return rows


def _parity(report):
    parity = report['controls'].get('parity')
    if not parity:
        return None
    return {key: parity.get(key) for key in ('calls', 'identical', 'normalized', 'different',
                                              'parity_rate', 'byte_identical_rate')}


def report_section(name, results, questions, specs, mode, layout, extras=(), level=None, topology=None,
                   limitations=(), calibration=None, freeze=None):
    """Score and compare the arms' runs; write report.json and report.md; summarize."""
    base_runs = [reports.ArmRun(load_run(results[0].run_dir), level, topology)]
    cand_runs = [reports.ArmRun(load_run(results[1].run_dir), level, topology)]
    for extra_level, pair in extras:
        base_runs.append(reports.ArmRun(load_run(pair[0].run_dir), extra_level, topology))
        cand_runs.append(reports.ArmRun(load_run(pair[1].run_dir), extra_level, topology))
    baseline = reports.Arm('baseline', tuple(base_runs), specs[0].variant)
    candidate = reports.Arm('candidate', tuple(cand_runs), specs[1].variant)
    counters, reason = tokens.try_load_counters()
    options = reports.ReportOptions(mode=mode.name, bootstrap_b=mode.bootstrap_b,
                                    extra_limitations=tuple(limitations), calibration=calibration,
                                    rules={'modes_sha256': mode.sha256}, freeze=freeze)
    report = reports.build_report(baseline, candidate, questions, counters, options, reason)
    directory = reports.write_report(report, layout, markdown.render(report))[0].parent
    verdict = report['verdict']
    return SectionResult(
        name, RAN, layout='private' if layout.private else 'public', report_key=report['report_key'],
        report_dir=directory,
        runs={arm: {'run_id': r.run_id, 'cached': r.cached} for arm, r in zip(('baseline', 'candidate'), results)},
        questions=report['provenance']['questions']['by_corpus'], verdict=verdict['value'],
        reasons=list(verdict['reasons']), applicable=applicable(verdict), headline=_headline(report),
        parity=_parity(report), aa=report['controls'].get('aa'), limitations=list(report['limitations']),
        freeze=freeze)


def run_section(name, specs, stores, questions, mode, layout, replica_nonce=None, level=None,
                topology=None, limitations=(), sweep=(), calibration=None, freeze=None):
    """measure + report_section, with the section's wall time; a bench error fails only this section."""
    started = time.perf_counter()
    try:
        results = measure(specs, stores, questions, mode, layout, replica_nonce=replica_nonce)
        extras = [(level, measure(specs, stores, questions, mode, layout, max_bytes, replica_nonce))
                  for max_bytes in sweep]
        result = report_section(name, results, questions, specs, mode, layout, extras, level, topology,
                                limitations, calibration, freeze)
    except BenchError as error:
        result = failed(name, error, time.perf_counter() - started)
        result.freeze = freeze
        return result
    result.seconds = time.perf_counter() - started
    return result
