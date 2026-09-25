"""Pre-registered scoring rules (BENCH_SPEC section 6).

Given a question's gold and what the binary answered, `score` returns one
`Scored` verdict. The rules are frozen by hash: `rules_sha256()` hashes this
file's bytes and a report records it (`provenance.rules.scoring_rules_sha256`);
changing any rule here is a `BENCH_VERSION` bump (SCHEMAS.md §0).

Positive questions (gold `KNOWN`):

- ANSWER correct: at least one core citation is in `answers` of an asked facet, no
  citation is in `wrong_subject`, and none is in `excluded_refs` (negated_anchor).
- PARTIAL useful: the same, plus `missing` names no facet the core covers per gold,
  and the question is enumerative. A PARTIAL on any other shape is an error.
- UNKNOWN: a false UNKNOWN; its reason is kept.
- ANSWER and PARTIAL are separate outcomes; useful = either one right.

Negative questions (gold `UNKNOWN`):

- UNKNOWN: correct abstention; the reason is right when it equals `unknown_reason`.
- PARTIAL: abstention only when `missing` names an `absent_terms` entry, else a false answer.
- ANSWER: a false answer.

"Names" is one textual rule for both checks (`names`): after case folding, turning
`_` and `-` into spaces and collapsing whitespace, the term occurs inside a
`missing` item. A facet is named by its `name` or by its reader `words`.
"""
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re

from . import refs

ANSWER = 'ANSWER'
PARTIAL = 'PARTIAL'
UNKNOWN = 'UNKNOWN'
DECISIONS = (ANSWER, PARTIAL, UNKNOWN)

# Outcome labels, stable strings a report groups by.
ANSWER_CORRECT = 'answer_correct'
ANSWER_WRONG = 'answer_wrong'
PARTIAL_USEFUL = 'partial_useful'
PARTIAL_WRONG = 'partial_wrong'  # right shape, but its citations or its `missing` fail
PARTIAL_ON_SINGULAR = 'partial_on_singular'
FALSE_UNKNOWN = 'false_unknown'
ABSTAIN_CORRECT = 'abstain_correct'
FALSE_ANSWER = 'false_answer'
POSITIVE_OUTCOMES = (ANSWER_CORRECT, ANSWER_WRONG, PARTIAL_USEFUL, PARTIAL_WRONG,
                     PARTIAL_ON_SINGULAR, FALSE_UNKNOWN)
NEGATIVE_OUTCOMES = (ABSTAIN_CORRECT, FALSE_ANSWER)


def rules_sha256():
    """The pre-registration hash of these rules: SHA-256 of this file's bytes."""
    return hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


@dataclass(frozen=True)
class Observed:
    """What one answer said, reduced to what the rules read."""
    decision: str
    cited: tuple = ()  # core refs, canonical
    missing: tuple = ()  # proof.missing items as text
    reason: str | None = None  # the binary's reason, mapped to bench vocabulary (BT10)
    confidence: str | None = None

    def __post_init__(self):
        if self.decision not in DECISIONS:
            raise ValueError(f'decision must be one of {DECISIONS}, got {self.decision!r}')

    @classmethod
    def from_structured(cls, structured, reason=None):
        """Read a v0.23.0 `kmp_ask` answer: ANSWER or UNKNOWN (PARTIAL arrives with P4).

        A structured answer that carries `decision: "PARTIAL"` is read as PARTIAL, so the
        reader works unchanged once a binary emits it.
        """
        proof = structured.get('proof') if isinstance(structured, dict) else None
        missing = proof.get('missing') if isinstance(proof, dict) else None
        items = tuple(item if isinstance(item, str) else json.dumps(item, sort_keys=True, ensure_ascii=False)
                      for item in (missing if isinstance(missing, list) else ()))
        if refs.is_unknown(structured):
            decision = UNKNOWN
        elif isinstance(structured, dict) and structured.get('decision') == PARTIAL:
            decision = PARTIAL
        else:
            decision = ANSWER
        confidence = proof.get('confidence') if isinstance(proof, dict) else None
        return cls(decision, refs.cited_refs(structured), items, reason,
                   confidence if isinstance(confidence, str) else None)


@dataclass(frozen=True)
class Scored:
    outcome: str
    positive: bool
    useful: bool | None  # positives only
    false_answer: bool | None  # negatives only
    false_unknown: bool | None  # positives only
    reason_correct: bool | None  # UNKNOWN with an expected reason and a reported one
    facet_coverage: float | None
    distractor_cited: bool
    detail: str


_SEPARATORS = re.compile(r'[\s_\-]+')


def _fold(text):
    return _SEPARATORS.sub(' ', text.casefold()).strip()


def names(missing, term):
    """Whether any `missing` item names `term` (see the module docstring)."""
    folded = _fold(term)
    return bool(folded) and any(folded in _fold(item) for item in missing)


def _facet_named(facet, missing):
    return names(missing, facet.name) or (facet.words is not None and names(missing, facet.words))


def _coverage(gold, cited):
    answerable = [facet for facet in gold.facets if facet.answerable_facet]
    if not answerable:
        return None
    return sum(1 for facet in answerable if cited & set(facet.answers)) / len(answerable)


def _citations_right(gold, cited):
    """ANSWER-correct citations: some answer, no wrong subject, nothing excluded."""
    if not cited & gold.answer_refs():
        return False, 'no core citation answers an asked facet'
    if cited & set(gold.wrong_subject):
        return False, 'cites a wrong-subject entry'
    if cited & set(gold.excluded_refs):
        return False, 'cites an excluded entry'
    return True, 'cites an answer'


def _reason_correct(gold, observed):
    if observed.decision != UNKNOWN or gold.unknown_reason is None or observed.reason is None:
        return None
    return observed.reason == gold.unknown_reason


def score(gold, observed):
    """Score one answer against its gold under the pre-registered rules."""
    cited = set(observed.cited)
    distractor = bool(cited & set(gold.wrong_subject))
    coverage = _coverage(gold, cited)
    if gold.answerable == 'KNOWN':
        return _score_positive(gold, observed, cited, distractor, coverage)
    return _score_negative(gold, observed, distractor, coverage)


def _score_positive(gold, observed, cited, distractor, coverage):
    def scored(outcome, useful, detail, reason_correct=None):
        return Scored(outcome, True, useful, None, outcome == FALSE_UNKNOWN, reason_correct,
                      coverage, distractor, detail)

    if observed.decision == UNKNOWN:
        return scored(FALSE_UNKNOWN, False, f'UNKNOWN on an answerable question (reason {observed.reason})')
    right, detail = _citations_right(gold, cited)
    if observed.decision == ANSWER:
        return scored(ANSWER_CORRECT if right else ANSWER_WRONG, right, detail)
    if gold.shape != 'enumerative':
        return scored(PARTIAL_ON_SINGULAR, False, 'PARTIAL is only admitted on enumerative questions')
    if not right:
        return scored(PARTIAL_WRONG, False, detail)
    covered = [facet.name for facet in gold.facets if cited & set(facet.answers)]
    wrongly_missing = [facet.name for facet in gold.facets
                       if facet.name in covered and _facet_named(facet, observed.missing)]
    if wrongly_missing:
        return scored(PARTIAL_WRONG, False,
                      f'missing names facets the core covers: {", ".join(wrongly_missing)}')
    return scored(PARTIAL_USEFUL, True, 'cites an answer; missing names only uncovered facets')


def _score_negative(gold, observed, distractor, coverage):
    def scored(outcome, detail, reason_correct=None):
        return Scored(outcome, False, None, outcome == FALSE_ANSWER, None, reason_correct,
                      coverage, distractor, detail)

    if observed.decision == UNKNOWN:
        return scored(ABSTAIN_CORRECT, 'UNKNOWN on an unanswerable question',
                      _reason_correct(gold, observed))
    if observed.decision == PARTIAL:
        named = [term for term in gold.absent_terms if names(observed.missing, term)]
        if named:
            return scored(ABSTAIN_CORRECT, f'PARTIAL naming what is absent: {", ".join(named)}')
        return scored(FALSE_ANSWER, 'PARTIAL whose missing names nothing absent')
    return scored(FALSE_ANSWER, 'ANSWER on an unanswerable question')


def confidence_correct(scored):
    """Correctness for confidence calibration: a useful positive or a correct abstention."""
    return scored.useful if scored.positive else scored.outcome == ABSTAIN_CORRECT
