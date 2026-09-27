"""`quick-a`, `full`, `jev` and `aa`: a mode's sections, one verdict, one summary.

  TT='uv run --no-project --with tiktoken==0.14.0 python -m scripts.performance.memory_bench'
  $TT quick-a --candidate scripts/performance/memory_bench/variants/X.toml
  $TT aa --variant scripts/performance/memory_bench/variants/baseline.toml

A mode (config/modes.toml) names its sections; each is measured on both arms and
written as its own `report.json` + `report.md` (application/sections.py):

- `real-store`: B-real (when labelled), hard negatives and identifier guards on the
  frozen real store; private cache (application/real_section.py);
- `synth-<topology>`: synth-v1 ladder, shared store imported by each arm
  (application/synth_section.py);
- `retrieval-judged`, `jev-judged`: the repository's judged corpora in replay
  (application/judged_section.py);
- parity: every run section compares the two arms call by call (report controls);
- `public`: evidence recall on the fetched public benchmarks, per arm
  (application/public_section.py); descriptive, it does not vote.

The mode's verdict combines the sections' by the precedence modes.toml
pre-registers; sections where the claim measured nothing do not vote. The summary
(`kmp.bench.mode_summary.v1`, `summary.json` + `summary.md`) lands under
`reports/<summary_key>/` of the private cache when a private section ran, else of the
public one. It holds aggregates, keys and timings only.
"""
from dataclasses import dataclass
from datetime import datetime, timezone
import json
import platform
import sys
import time

from .. import BENCH_VERSION
from ..domain import cachekey, jsonl
from ..domain.errors import BenchError
from ..domain.jsonl import REPO_ROOT
from ..runtime.layout import private_layout, public_layout
from . import judged_section, public_section, real_section, sections, summary_markdown, synth_section
from .arms import load_spec
from .modes import load_modes
from .report import code_sha256

SCHEMA = 'kmp.bench.mode_summary.v1'


class ModeRefused(BenchError):
    code = 'MODE_REFUSED'


@dataclass(frozen=True)
class ModeRequest:
    mode: str
    baseline: object  # Path of a variant TOML
    candidate: object  # Path; None for aa (the replica is the baseline itself)
    freeze: object = None
    private_root: object = None
    record: bool = False  # jev: record with the provider key taken from the environment
    out: object = None  # public cache root


def _now():
    return datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')


def _specs(request, mode, layout, log):
    base = load_spec(request.baseline, layout, REPO_ROOT, request.record, log)
    if mode.replica_of:
        return (base, base), f'aa-{datetime.now(timezone.utc).strftime("%Y%m%dt%H%M%S%f")}'
    cand = load_spec(request.candidate, layout, REPO_ROOT, request.record, log)
    if base.identity() == cand.identity():
        raise ModeRefused('baseline and candidate are the same binary and configuration: '
                          'run `aa --variant` for the A/A control')
    if base.name == cand.name:
        raise ModeRefused(f'both variants are named {base.name!r}: arms need distinct names')
    for spec in (base, cand):
        if spec.variant.jev == 'record' and mode.name != 'jev':
            raise ModeRefused(f'{spec.name}: jev = "record" runs only in the `jev` mode (quick modes replay)')
    return (base, cand), None


def combine(modes, results):
    """(verdict, reasons): the precedence order over the sections that vote."""
    voting = [r for r in results if r.status != sections.SKIPPED and r.verdict is not None and r.applicable]
    if not voting:
        return 'indecidible', ['no section measured a target or guard of the claim']
    value = modes.combine({r.verdict for r in voting})
    reasons = [f'{r.name}: {reason}' for r in voting if r.verdict == value for reason in r.reasons]
    return value, reasons


def used_freeze(results):
    """The B-real freeze the mode read (`real_section.freeze_identity`), or None when no section did."""
    return next((r.freeze for r in results if r.freeze), None)


def summarize(request, mode, modes, specs, results, started, total_s):
    material = {'mode': mode.name, 'modes_sha256': modes.sha256,
                'arms': [spec.identity() for spec in specs],
                'sections': [(r.name, r.status, r.report_key,
                              cachekey.digest(r.judged) if r.judged is not None else None) for r in results],
                'freeze': used_freeze(results)}
    verdict, reasons = combine(modes, results)
    return {
        'schema': SCHEMA, 'bench_version': BENCH_VERSION,
        'summary_key': cachekey.digest({'kind': 'mode_summary', 'bench_version': BENCH_VERSION,
                                        'material': material}),
        'mode': mode.name, 'modes_sha256': modes.sha256, 'description': mode.description,
        'generated_by': {'code_sha256': code_sha256(), 'python': platform.python_version()},
        'arms': {'baseline': specs[0].provenance(),
                 'candidate': None if mode.replica_of else specs[1].provenance()},
        'replica': bool(mode.replica_of),
        'freeze': used_freeze(results),
        'parameters': {**mode.run_parameters(), 'warmup': mode.warmup, 'block': mode.block,
                       'cpus': mode.cpus, 'bootstrap_b': mode.bootstrap_b, 'run_mode': mode.run_mode},
        'sections': [r.as_dict() for r in results],
        'timings': {'started_at': started, 'total_s': round(total_s, 3), 'budget_s': mode.budget_s or None,
                    'within_budget': None if not mode.budget_s else total_s <= mode.budget_s,
                    'by_section': {r.name: round(r.seconds, 3) for r in results}},
        'verdict': {'value': verdict, 'reasons': reasons,
                    'by_section': {r.name: r.verdict for r in results if r.status != sections.SKIPPED}}}


def write_summary(summary, layout):
    directory = layout.report(summary['summary_key'])
    text = json.dumps(summary, ensure_ascii=False, allow_nan=False, indent=1) + '\n'
    paths = [jsonl.write_text(directory / 'summary.json', text, overwrite=True),
             jsonl.write_text(directory / 'summary.md', summary_markdown.render(summary), overwrite=True)]
    return paths


def run_mode(request, log=sys.stderr):
    """Every section of the mode; returns (summary dict, [summary paths], results)."""
    modes = load_modes()
    mode = modes.get(request.mode)
    public = public_layout(request.out)
    started_at, started = _now(), time.perf_counter()
    specs, nonce = _specs(request, mode, public, log)
    results = []
    private = None
    for name in mode.sections:
        print(f'memory_bench {mode.name}: section {name}', file=log, flush=True)
        if name == 'real-store':
            result = real_section.run(specs, mode, request.freeze, request.private_root, nonce)
            if result.layout == 'private':
                fd = real_section.find_freeze(request.freeze, request.private_root)
                private = private_layout(fd.private_root)
            results.append(result)
        elif name == 'synth':
            results.extend(synth_section.run(specs, mode, nonce, public))
        elif name in judged_section.CORPORA:
            results.append(judged_section.run(name, specs, mode, public, REPO_ROOT, nonce is not None))
        elif name == 'public':
            results.append(public_section.run(specs, mode, public, request.private_root))
        print(f'memory_bench {mode.name}: {results[-1].name} {results[-1].status} '
              f'{results[-1].verdict or ""} {results[-1].seconds:.1f}s', file=log, flush=True)
    summary = summarize(request, mode, modes, specs, results, started_at, time.perf_counter() - started)
    paths = write_summary(summary, private or public)
    return summary, paths, results
