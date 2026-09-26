"""The neutral pool a labeler judges (BENCH_SPEC section 4.1, "Etiquetado ciego").

Three sources, fixed by the registered labeling rules:

- every entry of the about that contains an anchor of the question (a written
  occurrence, `Anchor.strict_pattern`, over the entry's own text and the texts
  that come with it);
- the kernel's top `kernel_top` entries for the question under `best_effort`,
  from the pinned reference binary on a disposable copy of the frozen store;
- `random` entries of the about drawn with the rules' seed, outside the two
  sets above.

The union is shuffled with a seed derived from the rules' seed and the
question id, so the order carries no rank. The pool record keeps only refs and
counts; which source contributed each ref lives in a separate provenance file
the labeling CLI never shows. Everything here is deterministic: same frozen
store, same binary, same rules, same pool (its `digest`).
"""
from dataclasses import dataclass
import copy

from ..domain import cachekey, refs
from ..domain.errors import BenchError
from ..domain.prng import SplitMix64, derive_seed
from ..corpora.anchor_forms import anchors_in

POOL_SCHEMA = 'kmp.bench.breal.pool.v1'
PROVENANCE_SCHEMA = 'kmp.bench.breal.pool_provenance.v1'
SOURCES = ('anchor', 'kernel', 'random')


class PoolFailed(BenchError):
    """The pool of a question could not be built as the rules say."""
    code = 'BREAL_POOL_FAILED'


@dataclass(frozen=True)
class PoolRules:
    seed: int
    kernel_top: int
    random: int
    answer_policy: str
    budget: dict
    max_calls: int

    @classmethod
    def from_rules(cls, rules):
        pool = rules['pool']
        return cls(rules['seed'], pool['kernel_top'], pool['random'], pool['answer_policy'],
                   dict(pool['budget']), pool['max_calls'])


@dataclass(frozen=True)
class Pool:
    question_id: str
    about: str
    refs: tuple  # shuffled
    counts: dict
    rules_sha256: str
    sources: dict  # ref -> sorted sources; provenance only, never shown to a labeler

    def digest(self):
        return cachekey.digest({'question_id': self.question_id, 'about': self.about,
                                'refs': list(self.refs), 'rules_sha256': self.rules_sha256})

    def as_dict(self):
        return {'schema': POOL_SCHEMA, 'question_id': self.question_id, 'about': self.about,
                'rules_sha256': self.rules_sha256, 'refs': list(self.refs), 'counts': dict(self.counts),
                'digest': self.digest()}

    def provenance(self):
        return {'schema': PROVENANCE_SCHEMA, 'question_id': self.question_id, 'digest': self.digest(),
                'note': 'do not open while labeling: it names the kernel-selected entries',
                'sources': {ref: list(self.sources[ref]) for ref in sorted(self.sources)}}


def parse_anchor(display):
    """'#188' / 'C6.4' / 'v0.18.6' -> Anchor (the intake's display forms)."""
    found = anchors_in([display.lower()])
    if len(found) != 1:
        raise PoolFailed(f'{display!r} is not one anchor')
    return next(iter(found))


def anchor_entries(about_docs, anchors):
    """Refs of the entries whose texts carry a written occurrence of any anchor."""
    patterns = [parse_anchor(a).strict_pattern() for a in anchors]
    return tuple(doc.ref for doc in about_docs.entries
                 if any(pattern.search(doc.joined()) for pattern in patterns))


def kernel_arguments(call, rules):
    """The question as asked (scope and words), under the pool's policy and budget."""
    arguments = {key: copy.deepcopy(call[key]) for key in
                 ('about', 'question', 'asked_as', 'dimensions', 'as_of', 'interval', 'axis')
                 if call.get(key) is not None}
    arguments['answer_policy'] = rules.answer_policy
    arguments['budget'] = copy.deepcopy(rules.budget)
    return arguments


def _entry_of(item, known):
    """The about entry an evidence item stands for: its id, else what it supports, else its source."""
    candidates = [refs.memory_ref(item)]
    supports = item.get('supports') if isinstance(item, dict) else None
    candidates += list(supports) if isinstance(supports, list) else []
    candidates.append(item.get('source') if isinstance(item, dict) else None)
    for candidate in candidates:
        if isinstance(candidate, str) and refs.normalize(candidate) in known:
            return refs.normalize(candidate)
    return None


def kernel_top(call_tool, call, rules, known):
    """The first `kernel_top` distinct about entries of the kernel's evidence, in its order.

    `call_tool(tool, arguments)` returns the structured content of one call. The
    server's own `projection.next_action` is followed verbatim until enough
    entries arrived, no action remains, or `max_calls` is spent.
    """
    tool, arguments = 'kmp_ask', kernel_arguments(call, rules)
    found = []
    for _ in range(rules.max_calls):
        structured = call_tool(tool, arguments)
        if not isinstance(structured, dict) or 'error' in structured:
            detail = (structured or {}).get('error') if isinstance(structured, dict) else structured
            raise PoolFailed(f'kernel call failed: {detail}')
        for item in (structured.get('proof') or {}).get('evidence') or []:
            ref = _entry_of(item, known)
            if ref is not None and ref not in found:
                found.append(ref)
        action = (structured.get('projection') or {}).get('next_action')
        if len(found) >= rules.kernel_top or not isinstance(action, dict) or not action.get('tool'):
            break
        tool, arguments = action['tool'], action.get('arguments') or {}
    return tuple(found[:rules.kernel_top])


def build_pool(question_id, about_docs, anchors, kernel_refs, rules, rules_sha256):
    known = {doc.ref for doc in about_docs.entries}
    unknown = [ref for ref in kernel_refs if ref not in known]
    if unknown:
        raise PoolFailed(f'{question_id}: kernel refs outside the about: {unknown[:3]}')
    sources = {}
    for ref in anchor_entries(about_docs, anchors):
        sources.setdefault(ref, set()).add('anchor')
    for ref in kernel_refs:
        sources.setdefault(ref, set()).add('kernel')
    rest = sorted(known - set(sources))
    stream = SplitMix64(derive_seed(rules.seed, f'random:{question_id}'))
    for ref in stream.sample(rest, min(rules.random, len(rest))):
        sources.setdefault(ref, set()).add('random')
    order = SplitMix64(derive_seed(rules.seed, f'shuffle:{question_id}')).shuffled(sorted(sources))
    counts = {source: sum(1 for s in sources.values() if source in s) for source in SOURCES}
    counts['total'] = len(order)
    return Pool(question_id, about_docs.about, tuple(order), counts, rules_sha256,
                {ref: tuple(sorted(s)) for ref, s in sources.items()})


def parse_pool(data, question_id=None):
    if not isinstance(data, dict) or data.get('schema') != POOL_SCHEMA:
        raise PoolFailed('not a kmp.bench.breal.pool.v1 record')
    if question_id is not None and data.get('question_id') != question_id:
        raise PoolFailed(f'pool of {data.get("question_id")}, expected {question_id}')
    pool = Pool(data['question_id'], data['about'], tuple(data['refs']), dict(data['counts']),
                data['rules_sha256'], {})
    if pool.digest() != data.get('digest'):
        raise PoolFailed(f'pool {pool.question_id}: digest mismatch (edited by hand?)')
    return pool

