"""B-real intake: the real `kmp_ask` calls of the local transcripts, as labeling work items.

An intake record (`kmp.bench.breal.intake.v1`) is one real question before any
gold exists: what was asked, where, when, plus what can be derived from the
words alone and is declared as mechanical — the anchors (`anchor_rule`) and a
proposed facet template (`template_rule`) the labeler confirms or edits. It is
not a `kmp.bench.question.v1` record: a runner only ever reads the resolved
questions `labeling.labels.resolve` writes to `gold.jsonl`.

The transcript rule is the one that produced the first 33 asks (2026-09-15 on):
Claude Code `tool_use` parts named `mcp__plugin_kmp_memory__kmp_ask` and Codex
`McpToolCall` items of server `kmp`; paging calls (a `continuation` or a
cursor) are reading, not asking, and are skipped; the host-owned keys
`context_id`, `purpose`, `page`, `review_token` are dropped; identical calls
count once, at their first time. Ids are content hashes, so a re-import keeps
every existing id and only adds new ones.
"""
from dataclasses import dataclass
import datetime
import glob
import hashlib
import json
import os
from pathlib import Path
import re
import unicodedata

from ..domain import cachekey, jsonl
from ..domain.errors import BenchError
from ..domain.fields import Fields
from ..domain.question import ANSWER_POLICIES, AXES
from .anchor_forms import Anchor

SCHEMA = 'kmp.bench.breal.intake.v1'
ANCHOR_RULE = 'anchors-in-words.v1'
TEMPLATE_RULE = 'facet-split.v1'
DEFAULT_SINCE = datetime.date(2026, 9, 15)
CLAUDE_TOOL = 'mcp__plugin_kmp_memory__kmp_ask'
DROPPED_KEYS = ('context_id', 'purpose', 'continuation', 'page', 'review_token')
CALL_KEYS = ('about', 'question', 'asked_as', 'answer_policy', 'dimensions', 'budget', 'as_of',
             'interval', 'axis', 'depth')
STATUSES = ('labelable', 'not_labelable')
SPLITS = ('development', 'validation')
HOSTS = ('claude', 'codex', 'author')


class IntakeInvalid(BenchError):
    """An intake record or a transcript call breaks kmp.bench.breal.intake.v1."""
    code = 'BREAL_INTAKE_INVALID'


# --- anchors -------------------------------------------------------------------------------

_HASH = re.compile(r'#(\d{2,6})\b')
_NAMED = re.compile(r'(?i)\b(?:issues?|pr|pull request)\s*#?(\d{2,6})'
                    r'((?:\s*(?:,|and|y|&|/|through|to|a|hasta)\s*#?\d{2,6})*)')
_RANGE = re.compile(r'(?i)(\d{2,6})\s*(?:through|to|a|hasta)\s*#?(\d{2,6})')
MAX_RANGE = 20
_DOTTED = re.compile(r'(?i)(?<![\w.])c(\d{1,2})((?:\.\d{1,3})+)(?!\d)')
_VERSION = re.compile(r'(?i)(?<![\w.])v(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?!\d)')


def extract_anchors(*texts):
    """Anchor identifiers the words name, in order of first appearance (declared mechanical)."""
    found = []
    for text in texts:
        if not text:
            continue
        hits = []
        for match in _HASH.finditer(text):
            hits.append((match.start(), Anchor('hash', (int(match.group(1)),))))
        for match in _NAMED.finditer(text):
            hits.append((match.start(), Anchor('hash', (int(match.group(1)),))))
            numbers = [int(n) for n in re.findall(r'\d{2,6}', match.group(0))]
            for low, high in _RANGE.findall(match.group(0)):
                if 0 < int(high) - int(low) <= MAX_RANGE:
                    numbers += range(int(low), int(high) + 1)
            for number in sorted(set(numbers)):
                hits.append((match.start(), Anchor('hash', (number,))))
        for match in _DOTTED.finditer(text):
            parts = (int(match.group(1)),) + tuple(int(p) for p in match.group(2).split('.') if p)
            hits.append((match.start(), Anchor('dotted', parts)))
        for match in _VERSION.finditer(text):
            hits.append((match.start(), Anchor('version', tuple(int(g) for g in match.groups()))))
        for _, anchor in sorted(hits, key=lambda hit: hit[0]):
            if anchor.display not in found:
                found.append(anchor.display)
    return tuple(found)


# --- facet template ------------------------------------------------------------------------

_LEAD = re.compile(r'(?i)^\s*(?:¿\s*)?(?:what|which|qu[eé]|cu[aá]l(?:es)?(?: es| son)?)\s+(?:(?:is|are|was|were)\s+(?:the\s+)?)?'
                   r'(?:(?:current|stored|exact|concrete|preserved|durable|specific)\s+)*')
_STOP = re.compile(r'(?i)\s(?:are|is|was|were|exist|exists|existe|existen|están|esta|está|están|'
                   r'must|should|did|do|does|has|have|hay|stored|recorded|registrad[oa]s?|almacenad[oa]s?|'
                   r'concerns?|describes?|describe|quedaron|debo|deben|'
                   r'for|para|sobre|about|of|de|del|in|en|from|concerning|regarding)\s')
_SPLIT = re.compile(r'(?i)\s*(?:,\s*(?:(?:and|or|y|o|e)\s+)?|\s+(?:and|or|y|o|e|&)\s+)\s*')
_LIST_LEAD = re.compile(r'(?i)(?:^|\s)(?:including|incluid[oa]s?|incluyendo|such as|sobre|about|for|para|en|on|:)\s')
MAX_FACET_WORDS = 5


def slug(words):
    """A facet name from the reader's words: ascii, lowercase, [a-z0-9-], at most 64 chars."""
    folded = unicodedata.normalize('NFKD', words).encode('ascii', 'ignore').decode('ascii').lower()
    name = re.sub(r'[^a-z0-9]+', '-', folded).strip('-')[:64].strip('-')
    return name or 'answer'


def facet_template(question):
    """[{name, words}] proposed from the question's own enumeration (declared mechanical).

    The asked-for things are the list between the interrogative and the first
    verb or preposition: "What decisions, implementation status and known gaps
    are stored for #203?" proposes decisions / implementation status / known
    gaps. A question with no such list gets one facet, `answer`, with its whole
    text as words. The labeler confirms, renames, adds or removes facets.
    """
    head = _LEAD.sub('', question, count=1)
    parts = _parts(head[:_stop(head)]) if head != question else []
    if len(parts) < 2:
        parts = _trailing_list(question)
    facets, seen = [], set()
    for words in parts:
        name = slug(words)
        if name not in seen:
            seen.add(name)
            facets.append({'name': name, 'words': words})
    return tuple(facets) if len(facets) >= 2 else ({'name': 'answer', 'words': question.strip()},)


def _stop(head):
    stop = _STOP.search(' ' + head + ' ')
    return max(stop.start() - 1, 0) if stop else len(head)


def _parts(segment):
    parts = [p.strip(' ¿?.;:') for p in _SPLIT.split(segment) if p and p.strip(' ¿?.;:')]
    return parts if all(len(p.split()) <= MAX_FACET_WORDS for p in parts) else []


def _trailing_list(question):
    """The comma list of the question's last clause: "..., including A, B and C?"."""
    first_comma = question.find(',')
    if first_comma < 0:
        return []
    leads = [m.end() for m in _LIST_LEAD.finditer(question[:first_comma])]
    if not leads:
        return []
    tail = re.split(r'[?¿]|\s(?:excluding|excepto|sin)\s', question[leads[-1]:])[0]
    return _parts(tail)


# --- intake record -------------------------------------------------------------------------

@dataclass(frozen=True)
class Intake:
    id: str
    call: dict  # the call as asked (CALL_KEYS only), about may be absent
    asked_at: str
    host: str
    session_sha256: str | None
    anchors: tuple
    facet_template: tuple  # ({name, words}, ...)
    status: str = 'labelable'
    status_reason: str | None = None
    duplicate_of: str | None = None
    split: str = 'development'

    @property
    def about(self):
        return self.call.get('about')

    @property
    def question(self):
        return self.call['question']

    def as_dict(self):
        return {'schema': SCHEMA, 'id': self.id, 'call': dict(self.call), 'asked_at': self.asked_at,
                'host': self.host, 'session_sha256': self.session_sha256, 'anchors': list(self.anchors),
                'anchor_rule': ANCHOR_RULE,
                'facet_template': [dict(f) for f in self.facet_template], 'template_rule': TEMPLATE_RULE,
                'status': self.status, 'status_reason': self.status_reason,
                'duplicate_of': self.duplicate_of, 'split': self.split}

    @classmethod
    def from_dict(cls, data, where='intake'):
        fields = Fields(data, where, IntakeInvalid)
        fields.text('schema', choices=(SCHEMA,))
        identifier = fields.text('id', pattern=r'breal-[0-9a-f]{12}')
        call = fields.mapping('call', required=True)
        _check_call(call, f'{where}.call', fields)
        facets = fields.objects('facet_template')
        for index, facet in enumerate(facets):
            if not (isinstance(facet, dict) and set(facet) == {'name', 'words'}
                    and re.fullmatch(r'[a-z0-9][a-z0-9_-]{0,63}', str(facet.get('name')))
                    and isinstance(facet.get('words'), str) and facet['words']):
                fields.fail(f'facet_template[{index}]', 'expected {name, words}')
        record = cls(identifier, dict(call), fields.text('asked_at'), fields.text('host', choices=HOSTS),
                     fields.text('session_sha256', required=False, pattern=r'[0-9a-f]{64}'),
                     fields.texts('anchors'), tuple(dict(f) for f in facets),
                     fields.text('status', choices=STATUSES), fields.text('status_reason', required=False),
                     fields.text('duplicate_of', required=False), fields.text('split', choices=SPLITS))
        fields.text('anchor_rule', choices=(ANCHOR_RULE,))
        fields.text('template_rule', choices=(TEMPLATE_RULE,))
        fields.done()
        if not record.facet_template:
            fields.fail('facet_template', 'at least one facet')
        if record.status == 'labelable' and not record.about:
            fields.fail('status', 'a question without an about cannot be labeled')
        if record.status != 'labelable' and not record.status_reason:
            fields.fail('status_reason', 'say why the question is not labelable')
        return record


def _check_call(call, where, fields):
    unknown = sorted(set(call) - set(CALL_KEYS))
    if unknown:
        fields.fail('call', f'unknown call key(s) {", ".join(unknown)}')
    if not isinstance(call.get('question'), str) or not call['question'].strip():
        fields.fail('call', 'question is required')
    if call.get('answer_policy') is not None and call['answer_policy'] not in ANSWER_POLICIES:
        fields.fail('call', f'answer_policy {call["answer_policy"]!r} is not a policy')
    if call.get('axis') is not None and call['axis'] not in AXES:
        fields.fail('call', f'axis {call["axis"]!r} is not an axis')


def intake_id(call):
    return 'breal-' + cachekey.digest(call)[:12]


def make_intake(call, asked_at, host, session_sha256=None, split='development'):
    call = {key: call[key] for key in CALL_KEYS if call.get(key) is not None}
    question = call.get('question')
    if not isinstance(question, str) or not question.strip():
        raise IntakeInvalid('a real ask without a question')
    status, reason = 'labelable', None
    if not call.get('about'):
        status, reason = 'not_labelable', 'about_missing: the server refused the call (about is required)'
    return Intake(intake_id(call), call, asked_at, host, session_sha256,
                  extract_anchors(question, call.get('asked_as')), facet_template(question),
                  status, reason, None, split)


def mark_retries(records):
    """A refused call without an about whose question was asked again with one is a duplicate."""
    by_question = {}
    for record in records:
        if record.about:
            by_question.setdefault(record.question, record.id)
    out = []
    for record in records:
        if not record.about and record.question in by_question:
            record = Intake(**{**record.__dict__, 'duplicate_of': by_question[record.question]})
        out.append(record)
    return out


# --- transcripts ---------------------------------------------------------------------------

@dataclass(frozen=True)
class TranscriptCall:
    arguments: dict
    asked_at: str
    host: str
    session_sha256: str


def _keep(arguments):
    if not isinstance(arguments, dict) or 'continuation' in arguments or 'cursor' in json.dumps(arguments):
        return None
    arguments = {k: v for k, v in arguments.items() if k not in DROPPED_KEYS}
    return arguments if isinstance(arguments.get('question'), str) and arguments['question'].strip() else None


def _day(stamp):
    try:
        return datetime.date.fromisoformat((stamp or '')[:10])
    except ValueError:
        return None


def _session(path):
    return hashlib.sha256(str(path).encode('utf-8')).hexdigest()


def _json(line):
    try:
        value = json.loads(line)
    except ValueError:
        return None
    return value if isinstance(value, dict) else None


def read_claude(paths):
    for path in paths:
        try:
            lines = Path(path).read_text(encoding='utf-8', errors='replace').splitlines()
        except OSError:
            continue
        for line in lines:
            if CLAUDE_TOOL not in line:
                continue
            row = _json(line)
            content = ((row or {}).get('message') or {}).get('content')
            if not isinstance(content, list):
                continue
            for part in content:
                if isinstance(part, dict) and part.get('type') == 'tool_use' and part.get('name') == CLAUDE_TOOL:
                    yield TranscriptCall(part.get('input'), row.get('timestamp') or '', 'claude', _session(path))


def read_codex(paths):
    for path in paths:
        try:
            lines = Path(path).read_text(encoding='utf-8', errors='replace').splitlines()
        except OSError:
            continue
        for line in lines:
            if '"McpToolCall"' not in line or '"server":"kmp"' not in line:
                continue
            row = _json(line)
            item = ((row or {}).get('payload') or {}).get('item') or {}
            if item.get('type') == 'McpToolCall' and item.get('server') == 'kmp' and item.get('tool') == 'kmp_ask':
                yield TranscriptCall(item.get('arguments'), row.get('timestamp') or '', 'codex', _session(path))


def transcript_paths(home=None):
    home = Path(home or os.path.expanduser('~'))
    claude = sorted(glob.glob(str(home / '.claude' / 'projects' / '**' / '*.jsonl'), recursive=True))
    codex = sorted(glob.glob(str(home / '.codex' / 'sessions' / '**' / '*.jsonl'), recursive=True))
    return claude, codex


def collect(calls, since=DEFAULT_SINCE, until=None, validation_from=None):
    """Intake records from transcript calls: window, paging skipped, first occurrence kept."""
    first = {}
    for call in calls:
        day = _day(call.asked_at)
        if day is None or day < since or (until is not None and day > until):
            continue
        arguments = _keep(call.arguments)
        if arguments is None:
            continue
        key = cachekey.canonical_json(arguments)
        if key not in first or call.asked_at < first[key].asked_at:
            first[key] = TranscriptCall(arguments, call.asked_at, call.host, call.session_sha256)
    records = []
    for call in sorted(first.values(), key=lambda c: (c.asked_at, cachekey.canonical_json(c.arguments))):
        split = 'validation' if validation_from is not None and call.asked_at >= validation_from else 'development'
        records.append(make_intake(call.arguments, call.asked_at, call.host, call.session_sha256, split))
    return mark_retries(records)


def merge(existing, fresh):
    """Existing records win (a labeler may already work on them); new ids are appended."""
    known = {record.id for record in existing}
    return list(existing) + [record for record in fresh if record.id not in known]


def load_intake(path):
    path = Path(path)
    if not path.exists():
        return []
    records, seen = [], set()
    for number, row in jsonl.parse_lines(path.read_text(encoding='utf-8'), str(path), IntakeInvalid):
        record = Intake.from_dict(row, f'{path}:{number}')
        if record.id in seen:
            raise IntakeInvalid(f'{path}:{number}: duplicate id {record.id}')
        seen.add(record.id)
        records.append(record)
    return records


def dump_intake(records):
    return jsonl.dump_lines(record.as_dict() for record in records)
