"""Label records (`kmp.bench.breal.label.v1`) and their resolution into B-real questions.

A label is one labeler's gold for one question. It is valid when it passes
`gold_schema.json`, the gold invariants of `domain/gold.py`, and the frozen
store: every ref it names is an entry of the question's about (the pool, or
anything the labeler found by searching). It must name the pool digest and the
rules hash it was made under, so a pool rebuilt under other rules cannot
silently inherit old labels.

`resolve` turns the labels into `kmp.bench.question.v1` records (corpus
`b-real`, private). Per question: an adjudicated label wins; otherwise labels
that agree exactly are taken; a question labeled by one labeler only takes that
label; labels that disagree without adjudication leave the question unresolved.
The question type is mechanical and declared (`TYPE_RULE`): anchors in the words
and an enumerative gold make `enumerative_anchored`, anchors and a singular gold
`singular_anchored`, no anchor `lookup_exact`.
"""
from dataclasses import dataclass
import json

from ..domain.cachekey import canonical_json
from ..domain.errors import BenchError, QuestionInvalid
from ..domain.gold import Gold
from ..domain.question import Question
from .schema import load_schema, validate

LABEL_SCHEMA = 'kmp.bench.breal.label.v1'
ADJUDICATOR = 'adjudicated'
TYPE_RULE = 'breal-type.v1'
DEFAULT_POLICY = 'evidence_or_unknown'  # kmp_ask's own default (MemoryAnswerPolicy::default)


class LabelInvalid(BenchError):
    """A label breaks the schema, the gold invariants, or the frozen store."""
    code = 'BREAL_LABEL_INVALID'


@dataclass(frozen=True)
class Label:
    question_id: str
    labeler: str
    gold: Gold
    record: dict

    def gold_key(self):
        return canonical_json(self.gold.as_dict())


def check_label(record, intake, known_refs, pool_digest, rules_sha256, schema=None):
    """A validated Label, or LabelInvalid naming the first broken rule."""
    try:
        validate(record, schema or load_schema())
        gold = Gold.from_dict(record['gold'], f'label {record["question_id"]}.gold')
    except (BenchError, QuestionInvalid) as failure:
        raise LabelInvalid(str(failure)) from failure
    where = f'{record["labeler"]}/{record["question_id"]}'
    if record['question_id'] != intake.id:
        raise LabelInvalid(f'{where}: labels {record["question_id"]}, expected {intake.id}')
    if record['pool_digest'] != pool_digest:
        raise LabelInvalid(f'{where}: made on another pool ({record["pool_digest"][:12]})')
    if record['rules_sha256'] != rules_sha256:
        raise LabelInvalid(f'{where}: made under other labeling rules ({record["rules_sha256"][:12]})')
    named = set(gold.wrong_subject)
    for facet in gold.facets:
        named |= set(facet.answers) | set(facet.related)
    outside = sorted(named - set(known_refs))
    if outside:
        raise LabelInvalid(f'{where}: refs not in about {intake.about}: {outside[:3]}')
    if gold.shape == 'enumerative' and len(gold.facets) < 2 or gold.shape == 'singular' and len(gold.facets) != 1:
        raise LabelInvalid(f'{where}: shape {gold.shape} with {len(gold.facets)} facet(s)')
    return Label(record['question_id'], record['labeler'], gold, record)


def label_record(question_id, labeler, labeled_at, pool_digest, rules_sha256, gold, notes=None, searches=()):
    return {'schema': LABEL_SCHEMA, 'question_id': question_id, 'labeler': labeler, 'labeled_at': labeled_at,
            'pool_digest': pool_digest, 'rules_sha256': rules_sha256, 'gold': gold, 'notes': notes,
            'searches': list(searches)}


def load_record(path):
    return json.loads(path.read_text(encoding='utf-8'))


@dataclass(frozen=True)
class Resolution:
    gold: Gold | None
    how: str  # adjudicated | agreed | single | unresolved | unlabeled


def resolve_one(labels, adjudicated=None):
    """labels: the non-adjudicator Labels of one question."""
    if adjudicated is not None:
        return Resolution(adjudicated.gold, 'adjudicated')
    if not labels:
        return Resolution(None, 'unlabeled')
    if len({label.gold_key() for label in labels}) == 1:
        return Resolution(labels[0].gold, 'agreed' if len(labels) > 1 else 'single')
    return Resolution(None, 'unresolved')


def question_type(anchors, gold):
    if not anchors:
        return 'lookup_exact'
    return 'enumerative_anchored' if gold.shape == 'enumerative' else 'singular_anchored'


def to_question(intake, gold, rules_sha256, how):
    """The kmp.bench.question.v1 record a runner reads (validated by Question.from_dict)."""
    call = intake.call
    arguments = {key: call[key] for key in ('budget', 'depth') if call.get(key) is not None}
    record = {'schema': 'kmp.bench.question.v1', 'id': intake.id, 'corpus': 'b-real',
              'type': question_type(intake.anchors, gold), 'about': intake.about, 'tool': 'kmp_ask',
              'question': call['question'], 'asked_as': call.get('asked_as'),
              'answer_policy': call.get('answer_policy') or DEFAULT_POLICY,
              'as_of': call.get('as_of'), 'interval': call.get('interval'), 'axis': call.get('axis'),
              'dimensions': call.get('dimensions'), 'arguments': arguments,
              'anchors': list(intake.anchors), 'gold': gold.as_dict(),
              'tags': sorted(['b-real', f'split:{intake.split}', f'gold:{how}']), 'private': True,
              'min_level': None,
              'source': {'kind': 'transcript', 'ref': intake.id, 'asked_at': intake.asked_at,
                         'host': intake.host, 'rules_sha256': rules_sha256, 'type_rule': TYPE_RULE}}
    return Question.from_dict(record, f'b-real {intake.id}')
