"""One bench question (kmp.bench.question.v1): what is asked, how, and its gold.

A question is data. `first_call` turns it into the exact first MCP tool call;
a runner then follows the server's own continuations, as token_harness' driver
does. Records round-trip byte for byte through `as_dict` / `from_dict`, and
`digest` is the anti-drift identity of the question (section 8).
"""
from dataclasses import dataclass
import copy
from pathlib import Path
import re

from . import cachekey, jsonl
from .errors import QuestionInvalid
from .fields import Fields
from .gold import Gold
from .question_rules import NEGATIVE_TYPES, TYPES, check_rules  # noqa: F401 (re-exported)

SCHEMA = 'kmp.bench.question.v1'
ID = r'[A-Za-z0-9][A-Za-z0-9_.-]{0,127}'
TAG = r'[a-z0-9][a-z0-9_.:-]{0,63}'
TOOLS = ('kmp_ask', 'kmp_wake', 'kmp_trace', 'kmp_curate')
ANSWER_POLICIES = ('evidence_or_unknown', 'show_conflicts', 'best_effort')
AXES = ('occurred', 'observed', 'ingested', 'validity')
CORPORA = ('synth-v1', 'b-real', 'hard-negatives', 'identifier-guards', 'retrieval-judged',
           'jev-judged', 'relate-judged', 'token-harness', 'longmemeval-s', 'locomo',
           'factconsolidation-sh', 'factconsolidation-mh', 'musique', '2wiki')
# Never inside the repository: real store material, and LoCoMo's NC licence.
PRIVATE_CORPORA = ('b-real', 'hard-negatives', 'identifier-guards', 'locomo')
# Call-level selections each tool's input schema accepts (tools_list.json at v0.23.0).
TOOL_SELECTIONS = {'kmp_ask': ('as_of', 'interval', 'axis', 'dimensions'),
                   'kmp_wake': ('as_of', 'interval', 'axis', 'dimensions'),
                   'kmp_trace': ('as_of', 'interval', 'axis'),
                   'kmp_curate': ('interval', 'axis', 'dimensions')}
# Owned by dedicated fields, or by the runner (continuations, guidance context).
RESERVED_ARGUMENTS = ('about', 'question', 'answer_policy', 'asked_as', 'as_of', 'interval',
                      'axis', 'dimensions', 'continuation', 'context_id')


def _selection(fields, key, allowed):
    value = fields.mapping(key)
    if value is None:
        return None
    if set(value) - set(allowed) or not value:
        fields.fail(key, f'takes {" and/or ".join(allowed)} only')
    for name, item in value.items():
        if not isinstance(item, str) or not item:
            fields.fail(key, f'{name} must be a non-empty string')
    return dict(value)


@dataclass(frozen=True)
class Question:
    id: str
    corpus: str
    type: str
    about: str
    tool: str
    gold: Gold
    question: str | None = None
    asked_as: str | None = None
    answer_policy: str | None = None
    as_of: dict | None = None
    interval: dict | None = None
    axis: str | None = None
    dimensions: dict | None = None
    arguments: dict | None = None
    anchors: tuple = ()
    tags: tuple = ()
    private: bool = False
    min_level: int | None = None
    source: dict | None = None

    @classmethod
    def from_dict(cls, data, where='question'):
        fields = Fields(data, where, QuestionInvalid)
        fields.text('schema', choices=(SCHEMA,))
        identifier = fields.text('id', pattern=ID)
        where = f'{where} {identifier}'
        fields.where = where
        gold = fields.mapping('gold', required=True)
        tags = fields.texts('tags', pattern=TAG)
        question = cls(
            id=identifier, corpus=fields.text('corpus', choices=CORPORA),
            type=fields.text('type'), about=fields.text('about', pattern=r'\S+'),
            tool=fields.text('tool', choices=TOOLS), gold=Gold.from_dict(gold, where + '.gold'),
            question=fields.text('question', required=False),
            asked_as=fields.text('asked_as', required=False),
            answer_policy=fields.text('answer_policy', required=False, choices=ANSWER_POLICIES),
            as_of=_selection(fields, 'as_of', ('time', 'ref')),
            interval=_selection(fields, 'interval', ('start', 'end')),
            axis=fields.text('axis', required=False, choices=AXES),
            dimensions=copy.deepcopy(fields.mapping('dimensions')),
            arguments=copy.deepcopy(fields.mapping('arguments') or {}),
            anchors=fields.texts('anchors'), tags=tuple(sorted(tags)),
            private=fields.flag('private', False),
            min_level=fields.integer('min_level', required=False, minimum=1),
            source=_source(fields))
        fields.done()
        question._check_call(fields)
        check_rules(question, fields)
        return question

    def _check_call(self, fields):
        allowed = TOOL_SELECTIONS[self.tool]
        for name in ('as_of', 'interval', 'axis', 'dimensions'):
            if getattr(self, name) is not None and name not in allowed:
                fields.fail(name, f'{self.tool} takes no {name}')
        if self.as_of is not None and len(self.as_of) != 1:
            fields.fail('as_of', 'exactly one of time or ref')
        if self.as_of is not None and self.interval is not None:
            fields.fail('interval', 'as_of and interval are exclusive')
        if self.axis is not None and self.as_of is None and self.interval is None:
            fields.fail('axis', 'applies only with as_of or interval')
        is_ask = self.tool == 'kmp_ask'
        if is_ask and (self.question is None or self.answer_policy is None):
            fields.fail('question', 'kmp_ask needs question and answer_policy')
        if not is_ask and (self.answer_policy is not None or self.asked_as is not None):
            fields.fail('answer_policy', f'{self.tool} takes no answer_policy or asked_as')
        clash = sorted(set(self.arguments) & set(RESERVED_ARGUMENTS))
        if clash:
            fields.fail('arguments', f'{", ".join(clash)} belong to dedicated fields or the runner')
        if self.tool == 'kmp_trace' and ('from' not in self.arguments
                                         or not {'to', 'search'} & set(self.arguments)):
            fields.fail('arguments', 'kmp_trace needs from, and to or search')
        if self.tool == 'kmp_curate' and 'mode' not in self.arguments:
            fields.fail('arguments', 'kmp_curate needs mode')
        if self.corpus in PRIVATE_CORPORA and not self.private:
            fields.fail('private', f'corpus {self.corpus} is private by definition')
        if self.min_level is not None and self.corpus != 'synth-v1':
            fields.fail('min_level', 'only synth-v1 questions sit on the ladder')

    def first_call(self, max_bytes=None, default_budget=None):
        """(tool, arguments) of the first call (SCHEMAS.md 2.1).

        `default_budget` (a mode's budget) applies only when the question pins none;
        `max_bytes` then sets budget.max_bytes as token_harness' `first_arguments` does.
        """
        arguments = copy.deepcopy(self.arguments)
        if default_budget is not None and 'budget' not in arguments:
            arguments['budget'] = copy.deepcopy(default_budget)
        arguments['about'] = self.about
        if self.tool == 'kmp_ask':
            arguments['question'] = self.question
            arguments['answer_policy'] = self.answer_policy
            if self.asked_as is not None:
                arguments['asked_as'] = self.asked_as
        for name in ('as_of', 'interval', 'axis', 'dimensions'):
            if getattr(self, name) is not None:
                arguments[name] = copy.deepcopy(getattr(self, name))
        if max_bytes is not None:
            arguments.setdefault('budget', {})['max_bytes'] = max_bytes
        return self.tool, arguments

    def as_dict(self):
        return {'schema': SCHEMA, 'id': self.id, 'corpus': self.corpus, 'type': self.type,
                'about': self.about, 'tool': self.tool, 'question': self.question,
                'asked_as': self.asked_as, 'answer_policy': self.answer_policy,
                'as_of': copy.deepcopy(self.as_of), 'interval': copy.deepcopy(self.interval),
                'axis': self.axis,
                'dimensions': copy.deepcopy(self.dimensions),
                'arguments': copy.deepcopy(self.arguments),
                'anchors': list(self.anchors), 'gold': self.gold.as_dict(),
                'tags': list(self.tags), 'private': self.private,
                'min_level': self.min_level, 'source': copy.deepcopy(self.source)}

    def digest(self):
        return cachekey.digest(self.as_dict())


def _source(fields):
    value = fields.mapping('source')
    if value is None:
        return None
    for key, item in value.items():
        if not re.fullmatch(r'[a-z][a-z0-9_]{0,63}', key) or isinstance(item, (dict, list)):
            fields.fail('source', f'{key!r}: flat snake_case keys with scalar values only')
    return dict(value)


def questions_digest(questions):
    """Order-free identity of a question set; duplicate ids are refused."""
    return cachekey.ordered_digest((question.id, question.digest()) for question in questions)


def parse_questions(text, origin='<questions>'):
    """Every line validated; ids unique across the file; file order kept."""
    questions, seen = [], {}
    for number, record in jsonl.parse_lines(text, origin, QuestionInvalid):
        question = Question.from_dict(record, f'{origin}:{number}')
        if question.id in seen:
            raise QuestionInvalid(f'{origin}:{number}: id {question.id} already on line {seen[question.id]}')
        seen[question.id] = number
        questions.append(question)
    return tuple(questions)


def load_questions(path):
    path = Path(path)
    return parse_questions(path.read_text(encoding='utf-8'), str(path))


def write_questions(path, questions, overwrite=False, repo_root=jsonl.REPO_ROOT):
    """Canonical JSONL; a file holding any private question may not live in the repo."""
    questions = tuple(questions)
    questions_digest(questions)  # refuses duplicate ids before anything is written
    if any(question.private for question in questions):
        jsonl.guard_private_path(path, repo_root)
    return jsonl.write_text(path, jsonl.dump_lines(q.as_dict() for q in questions), overwrite)
