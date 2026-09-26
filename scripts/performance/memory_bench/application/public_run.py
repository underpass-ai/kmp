"""Evidence recall on the public corpora, without a reader (BENCH_SPEC 4.6, BT17).

`run_public` asks a corpus' questions on its cached `import` store through the
bench runner (`run_questions.run_arms`: release binary, MCP stdio, a fresh copy of
the template per process) and `recall_summary` reads the answers the way the Rust
scorecards read them (`domain/refs.py`):

- retrieved = `proof.evidence[].id` of the journey's pages, in order; R@k is the
  ported `RetrievalOutcome.recall_at` (fraction of gold refs in the first k);
- core = `because[].ref`; observed = retrieved + core, which is what
  `longmemeval_kmp_runner.rs` classifies into Full / Partial / Missing;
- per corpus: R@1/2/5/10 and MRR over the answering refs; full-chain recovery @2/5/10
  for chains (MuSiQue, 2Wiki, FC-MH); session-level R@k and Full/Partial/Missing for
  LongMemEval; stale served / stale ranked before the current fact for FC-SH.

This is recall of evidence only. QA accuracy needs a fixed reader and a judge of
another family (BENCH_SPEC 4.6) and is not measured here. Every figure is a plain
proportion or mean with its n; a figure that does not apply is absent, not zero.
"""
from collections import defaultdict
from pathlib import Path

from ..corpora import longmemeval
from ..domain import metrics, refs, stats
from ..domain.jsonl import REPO_ROOT
from ..domain.variant import load_variant
from .run_data import load_run
from .run_questions import Arm, RunPlan, StoreRef, run_arms

MODE = 'public-recall'
DEFAULT_VARIANT = REPO_ROOT / 'scripts' / 'performance' / 'memory_bench' / 'variants' / 'baseline.toml'
KS = (1, 2, 5, 10)
CHAIN_KS = (2, 5, 10)


def store_ref(built):
    record = built['import'].record
    return StoreRef(record.store_key, record.content_digest, record.label, built['import'].template)


def arm_for(variant_path, built, binary=None, repo_root=REPO_ROOT):
    variant = load_variant(variant_path, repo_root)
    if binary is None:
        if variant.binary.path is None:
            raise ValueError(f'{variant_path}: give --binary for a git_ref variant')
        binary = Path(variant.binary.path)
        binary = binary if binary.is_absolute() else Path(repo_root) / binary
    return Arm(variant=variant, variant_path=str(variant_path), binary=Path(binary).resolve(),
               store=store_ref(built))


def run_public(corpus, built, layout, variant_path=DEFAULT_VARIANT, binary=None, max_calls=1,
               cpus=None, warmup=1, timeout_s=120.0, samples=1, repeats=1):
    """(ArmResult, RunData) of the corpus questions on its import store."""
    arm = arm_for(variant_path, built, binary)
    plan = RunPlan(tuple(corpus.questions), MODE, samples=samples, repeats=repeats, max_calls=max_calls,
                   timeout_s=timeout_s, warmup=warmup, cpus=cpus)
    [result] = run_arms([arm], plan, layout)
    return result, load_run(result.run_dir)


# --- reading a run ------------------------------------------------------------------------

def journey_pages(run):
    """{question id: (status, [structured page...])} of sample 0, repeat 0."""
    pages = {}
    for record in run.journeys:
        if record.sample != 0 or record.repeat != 0:
            continue
        structured = [run.structured(call) for call in run.calls_of(record.journey)]
        pages[record.question_id] = (record.status, [page for page in structured if page is not None])
    return pages


def observe(pages):
    """(retrieved in order, core set, unknown) of a journey's pages."""
    retrieved = [ref for page in pages for ref in refs.evidence_refs(page)]
    core = set()
    for page in pages:
        core.update(refs.cited_refs(page))
    unknown = bool(pages) and refs.is_unknown(pages[0])
    return retrieved, core, unknown


def _first_index(items, wanted):
    for index, item in enumerate(items):
        if item in wanted:
            return index
    return None


def _dedup(items):
    seen, out = set(), []
    for item in items:
        if item not in seen:
            seen.add(item)
            out.append(item)
    return out


def question_values(question, pages):
    """Per-question recall values (bool = one trial of a rate, float = one observation)."""
    retrieved, core, unknown = observe(pages)
    gold = question.gold
    judged = gold.answer_refs()
    values = {'unknown': unknown, 'retrieved': float(len(retrieved))}
    if judged:
        outcome = metrics.RetrievalOutcome.of(judged, retrieved, core, unknown)
        for k in KS:
            values[f'recall_at_{k}'] = outcome.recall_at(k)
            values[f'complete_at_{k}'] = outcome.has_complete_support_at(k)
        values['mrr'] = outcome.reciprocal_rank()
        values['any_hit'] = bool(judged & set(retrieved))
        values['core_cites_gold'] = bool(judged & core)
    if gold.chain is not None:
        for k in CHAIN_KS:
            values[f'full_chain_at_{k}'] = metrics.full_chain_recovered_at(gold.chain.refs, retrieved, k)
    if gold.stale_refs:
        current = gold.current_ref
        values['stale_served'] = metrics.serves_stale(gold.stale_refs, core)
        values['successor_rescued'] = current in core
        stale_at, current_at = _first_index(retrieved, set(gold.stale_refs)), _first_index(retrieved, {current})
        values['stale_before_current'] = stale_at is not None and (current_at is None or stale_at < current_at)
    if question.corpus == longmemeval.CORPUS:
        abstention = gold.answerable == 'UNKNOWN'
        values['evidence_hit'] = longmemeval.classify_evidence(
            sorted(judged), set(retrieved) | core, abstention)
        if abstention:
            values['abstained'] = unknown
        else:
            sessions = _dedup(s for s in (longmemeval.session_of(r) for r in retrieved) if s)
            wanted = set(filter(None, (question.source or {}).get('answer_sessions', '').split(',')))
            if wanted:
                session = metrics.RetrievalOutcome.of(wanted, sessions)
                for k in KS:
                    values[f'session_recall_at_{k}'] = session.recall_at(k)
    return values


def _rate(flags):
    rate = metrics.Rate.of(flags)
    low, high = stats.wilson(rate.hits, rate.n) if rate.n else (None, None)
    return {'value': rate.value, 'hits': rate.hits, 'n': rate.n, 'wilson95': [low, high]}


def _mean(values):
    values = [value for value in values if value is not None]
    return {'value': sum(values) / len(values) if values else None, 'n': len(values)}


def aggregate(rows):
    """{metric: rate or mean} over per-question value dicts."""
    by_metric = defaultdict(list)
    for values in rows:
        for name, value in values.items():
            by_metric[name].append(value)
    out = {}
    for name in sorted(by_metric):
        items = by_metric[name]
        if name == 'evidence_hit':
            counts = defaultdict(int)
            for item in items:
                counts[item] += 1
            out[name] = dict(sorted(counts.items()))
        elif all(isinstance(item, bool) for item in items):
            out[name] = _rate(items)
        else:
            out[name] = _mean(float(item) for item in items)
    return out


def recall_summary(corpus, run):
    """Aggregates per question corpus, per corpus x type, and per tag stratum (hops, qtype)."""
    pages = journey_pages(run)
    groups, statuses, missing = defaultdict(list), defaultdict(int), []
    for question in corpus.questions:
        if question.id not in pages:
            missing.append(question.id)
            continue
        status, answer = pages[question.id]
        statuses[status] += 1
        if status not in ('completed', 'call_cap_reached') or not answer:
            continue
        values = question_values(question, answer)
        groups[(question.corpus, 'all')].append(values)
        groups[(question.corpus, f'type:{question.type}')].append(values)
        for tag in question.tags:
            if tag.split(':', 1)[0] in ('hops', 'qtype', 'category', 'size') or tag == 'abstention':
                groups[(question.corpus, tag)].append(values)
    summary = defaultdict(dict)
    for (name, stratum), rows in sorted(groups.items()):
        summary[name][stratum] = {'questions': len(rows), **aggregate(rows)}
    return {'run_id': run.run_id, 'statuses': dict(statuses), 'not_run': missing,
            'corpora': dict(summary)}
