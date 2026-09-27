"""Parity oracle: do two captures hold the same tool calls, byte for byte?

Two captures of the same questions (a baseline and a candidate, or two fresh
processes of one binary) are aligned trace by trace and call by call. Each
`tools/call` exchange is one of:

- `identical`: the request and the response lines are the same bytes;
- `normalized`: they differ only in a value named by `VOLATILE_FIELDS`, and
  the volatile values are linked the same way on both sides (the handle a
  response returns is the one the next request sends back);
- `different`: anything else, with the JSON paths that differ.

`parity_rate` counts identical and normalized calls; `byte_identical_rate`
counts identical ones only, so a normalization is never hidden. How many raw
differences each volatile rule absorbed is reported too (`volatile_reconciled`):
a rule that was never needed shows 0.

Only the fields in `VOLATILE_FIELDS` are normalized, and only when their value
has the documented volatile shape; a cursor that turns null, or a timestamp in
another field, is a difference. Values never leave this module: reports carry
paths and counts, so a private corpus can be compared without copying its text.

Inputs: a token_harness capture or a bench run directory (trace lines as
written by `TraceRecorder`, plain or gzip, with or without `manifest.json`), a
`baseline.py` output directory (read through `baseline_compare.read_results`),
or, for `compare_rendered`, JSON/JSONL dumps of kmp-embedded `MemoryRecallView`s.
"""
from collections import Counter
from dataclasses import dataclass, field
import gzip
import json
from pathlib import Path
import re
import sys

from ...token_harness.domain.errors import HarnessError

SCHEMA = 'kmp.bench.parity.v1'
MAX_LISTED_PATHS = 50  # per call; the rest is counted in `truncated`
SETUP_SUFFIXES = ('.fixture', '.oracle', '.setup')
TIMESTAMP = r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{1,9})?(Z|[+-]\d{2}:\d{2})'


class ParityInputError(HarnessError):
    code = 'PARITY_INPUT_INVALID'


@dataclass(frozen=True)
class VolatileField:
    """A value that differs between two faithful runs, where it sits, and why.

    `path` is matched (search) against the generalized JSON path of the value
    (`$.result.structuredContent.page.next_cursor`, list indices as `[]`), and
    `pattern` in full against the value; a list of strings is matched item by
    item. Rules of one `group` share labels. A `linked` rule never mints a
    label: it fires only on a value its group already labelled in the same
    item, e.g. an `observed_at` the kernel defaulted to the ingestion instant.
    """
    name: str
    group: str
    path: str
    pattern: str
    reason: str
    linked: bool = False

    def matches(self, path, value):
        return (isinstance(value, str) and re.search(self.path, path) is not None
                and re.fullmatch(self.pattern, value) is not None)

    def as_dict(self):
        return {'name': self.name, 'group': self.group, 'path': self.path,
                'pattern': self.pattern, 'linked': self.linked, 'reason': self.reason}


VOLATILE_FIELDS = (
    VolatileField('continuation', 'continuation', r'\.continuation$', r'read_[0-9a-f]{32}',
                  'handle of a paged read that the server mints at random per process; two '
                  'fresh processes on one store return different handles for the same page '
                  '(measured: B-real wakes differ only here, and the request sends it back)'),
    VolatileField('next_cursor', 'next_cursor', r'\.next_cursor$',
                  r'kmp[12]:\d+:(?:[0-9a-f]{16}:)?[0-9a-f]{64}',
                  'opaque page cursor bound to server-side read state; the page it names is '
                  'compared in full, so only its encoding is set aside'),
    VolatileField('ingested_at', 'ingested', r'\.ingested_at(\[\])?$', TIMESTAMP,
                  'wall-clock instant the store stamps on a write; memory written during the '
                  'run (a judged case into its fresh store) carries the time of that run'),
    VolatileField('clocks.ingested', 'ingested', r'\.clocks\.ingested\.[a-z_]+(\[\])?$',
                  TIMESTAMP, 'the same ingestion instant, as the receipt\'s clock summary'),
    VolatileField('observed_at=ingested', 'ingested', r'\.observed_at(\[\])?$', TIMESTAMP,
                  'observed_at the kernel defaulted to the ingestion instant because the write '
                  'named none; linked: fires only on a value already seen as an ingestion '
                  'stamp of the same item, never on an observed_at the writer gave',
                  linked=True),
    VolatileField('review_token', 'review_token', r'\.(neighborhood\.token|review_token)$',
                  r'[0-9a-f]{64}',
                  'digest the server binds to the neighbourhood packet it showed a writer; the '
                  'packet carries ingestion clocks, so it follows them; the returned token and '
                  'the one sent back keep one label'),
)


@dataclass(frozen=True)
class Exchange:
    trace: str
    index: int  # position among the trace's tools/call exchanges
    tool: str | None
    request: str  # raw lexemes, exactly as recorded
    response: str


def _raw_members(line, origin):
    """The recorder writes `{"session":S,"request":R,"response":P}` by concatenation."""
    decoder = json.JSONDecoder()
    prefix = '{"session":'
    if not line.startswith(prefix):
        return None
    try:
        _, at = decoder.raw_decode(line, len(prefix))
        if not line.startswith(',"request":', at):
            return None
        start = at + len(',"request":')
        _, end = decoder.raw_decode(line, start)
        request = line[start:end]
        if not line.startswith(',"response":', end):
            return None
        start = end + len(',"response":')
        _, stop = decoder.raw_decode(line, start)
    except ValueError as error:
        raise ParityInputError(f'{origin}: unreadable trace line: {error}') from error
    if line[stop:] != '}':
        raise ParityInputError(f'{origin}: trailing bytes after the response')
    return request, line[start:stop]


def parse_trace(text, trace):
    """The `tools/call` exchanges of one trace, in order; markers and handshakes skipped."""
    exchanges = []
    for number, line in enumerate(text.splitlines()):
        members = _raw_members(line, f'{trace}#{number}')
        if members is None:
            continue
        request = json.loads(members[0])
        if request.get('method') != 'tools/call':
            continue
        exchanges.append(Exchange(trace, len(exchanges), (request.get('params') or {}).get('name'),
                                  members[0], members[1]))
    return tuple(exchanges)


def _read(path):
    raw = path.read_bytes()
    return (gzip.decompress(raw) if path.suffix == '.gz' else raw).decode('utf-8')


def _trace_files(root):
    manifest = root / 'manifest.json'
    if manifest.is_file():
        names = json.loads(manifest.read_text(encoding='utf-8')).get('uncompressed_files') or {}
        files = {}
        for name in names:
            if name.endswith('.jsonl'):
                plain, packed = root / name, root / (name + '.gz')
                files[name[:-len('.jsonl')]] = plain if plain.is_file() else packed
        return files
    files = {}
    for path in sorted(root.iterdir()):
        for suffix in ('.jsonl', '.jsonl.gz'):
            if path.name.endswith(suffix):
                files[path.name[:-len(suffix)]] = path
    return files


def load_capture(root):
    """{trace name: exchanges} of a capture or run directory, or of a baseline.py output."""
    root = Path(root)
    if not root.is_dir():
        raise ParityInputError(f'{root} is not a directory')
    if (root / 'results.json').is_file() and not (root / 'manifest.json').is_file():
        return load_baseline_results(root)
    traces = {}
    for name, path in _trace_files(root).items():
        if not path.is_file():
            raise ParityInputError(f'{path} is listed but missing')
        if name.startswith('process-'):
            continue  # handshake frames only
        exchanges = parse_trace(_read(path), name)
        if exchanges:
            traces[name] = exchanges
    if not traces:
        raise ParityInputError(f'{root} holds no tools/call exchange')
    return traces


def load_baseline_results(root):
    """baseline.py's complete read results, the fingerprints `baseline_compare` asserts on."""
    from ...baseline_compare import read_results
    traces = {}
    for shape, fingerprints in read_results(Path(root)).items():
        traces[shape] = tuple(
            Exchange(shape, index, name, json.dumps({'operation': name}),
                     json.dumps(fingerprints[name], sort_keys=True, ensure_ascii=False))
            for index, name in enumerate(sorted(fingerprints)))
    return traces


class Normalizer:
    """Replaces volatile values by `<group#n>`, numbered by first appearance in one item.

    The numbering keeps the links: a handle returned by call i and sent by call
    i+1 gets the same label on both sides, so a broken link is still a difference.
    Primary rules label an exchange first (request, then response), then linked
    rules look those labels up, so key order inside an object does not matter.
    """

    def __init__(self, rules=VOLATILE_FIELDS):
        self.rules, self.labels, self.seen = tuple(rules), {}, Counter()

    @staticmethod
    def _key(rule, value):
        """One label per instant: KMP writes one ingestion instant both with nine
        fractional digits and with trailing zeros trimmed (`.134080070Z` in a
        receipt, `.13408007Z` in a neighbourhood), so a timestamp is keyed by its
        value, not its spelling. Handles and digests are keyed as they are."""
        if rule.pattern != TIMESTAMP:
            return rule.group, value
        head, fraction, zone = re.fullmatch(r'(.*?:\d{2})(?:\.(\d+))?(Z|[+-]\d{2}:\d{2})', value).groups()
        fraction = (fraction or '').rstrip('0')
        return rule.group, head + ('.' + fraction if fraction else '') + zone

    def _rule(self, path, value, linked):
        for rule in self.rules:
            if rule.linked == linked and rule.matches(path, value):
                if linked and self._key(rule, value) not in self.labels:
                    continue
                return rule
        return None

    def _mint(self, rule, value):
        key = self._key(rule, value)
        if key not in self.labels:
            self.seen[rule.group] += 1
            self.labels[key] = f'<{rule.group}#{self.seen[rule.group]}>'

    def _collect(self, value, path):
        if isinstance(value, dict):
            for key, item in value.items():
                self._collect(item, f'{path}.{key}')
        elif isinstance(value, list):
            for item in value:
                self._collect(item, path + '[]')
        else:
            rule = self._rule(path, value, linked=False)
            if rule is not None:
                self._mint(rule, value)

    def _replace(self, value, path):
        if isinstance(value, dict):
            return {key: self._replace(item, f'{path}.{key}') for key, item in value.items()}
        if isinstance(value, list):
            return [self._replace(item, path + '[]') for item in value]
        rule = self._rule(path, value, linked=False) or self._rule(path, value, linked=True)
        return value if rule is None else self.labels[self._key(rule, value)]

    def apply_all(self, *values):
        """Normalize the parts of one exchange together (labels first, then links)."""
        for value in values:
            self._collect(value, '$')
        return [self._replace(value, '$') for value in values]


def json_diff(left, right, path='$'):
    """[(path, kind)] where two JSON values differ; kind is value, type, only_left,
    only_right or length. Lists are compared position by position."""
    if type(left) is not type(right):
        return [(path, 'type')]
    if isinstance(left, dict):
        found = []
        for key in sorted(set(left) | set(right)):
            where = f'{path}.{key}' if re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', key) else \
                f'{path}[{json.dumps(key, ensure_ascii=False)}]'
            if key not in right:
                found.append((where, 'only_left'))
            elif key not in left:
                found.append((where, 'only_right'))
            else:
                found.extend(json_diff(left[key], right[key], where))
        return found
    if isinstance(left, list):
        found = [] if len(left) == len(right) else [(path, 'length')]
        for index, (a, b) in enumerate(zip(left, right)):
            found.extend(json_diff(a, b, f'{path}[{index}]'))
        return found
    return [] if left == right else [(path, 'value')]


def generalize(path):
    return re.sub(r'\[\d+\]', '[]', path)


@dataclass
class CallParity:
    trace: str
    index: int
    tool: str | None
    status: str  # identical | normalized | different
    paths: list = field(default_factory=list)  # [(path, kind)], request paths prefixed `request`
    reconciled: Counter = field(default_factory=Counter)  # rule name -> raw differences absorbed

    def as_dict(self):
        listed = [{'path': p, 'kind': k} for p, k in self.paths[:MAX_LISTED_PATHS]]
        return {'trace': self.trace, 'index': self.index, 'tool': self.tool, 'status': self.status,
                'paths': listed, 'truncated': max(0, len(self.paths) - MAX_LISTED_PATHS)}


def _parsed(raw):
    try:
        return json.loads(raw)
    except ValueError:
        return None


def _reconciled(raw_paths, kept, rules):
    """Which rule absorbed each raw difference that normalization made disappear."""
    names = Counter()
    for path, _ in raw_paths:
        if path in kept:
            continue
        general = generalize(path)
        rule = next((r for r in rules if re.search(r.path, general)), None)
        names[rule.name if rule else 'unattributed'] += 1
    return names


def compare_exchange(left, right, left_norm, right_norm):
    parsed = [_parsed(raw) for raw in (left.request, left.response, right.request, right.response)]
    ok = all(value is not None for value in parsed)
    # Always feed the normalizers, so the numbering of later calls stays aligned.
    normal = (left_norm.apply_all(*parsed[:2]) + right_norm.apply_all(*parsed[2:])) if ok else parsed
    if left.request == right.request and left.response == right.response:
        return CallParity(left.trace, left.index, left.tool, 'identical')
    paths, reconciled = [], Counter()
    for part, at in (('request', 0), ('$', 1)):
        a, b = (left.request, right.request) if at == 0 else (left.response, right.response)
        if not ok:
            if a != b:
                paths.append((part, 'unparseable'))
            continue
        found = json_diff(normal[at], normal[at + 2], part)
        raw = json_diff(parsed[at], parsed[at + 2], part)
        reconciled += _reconciled(raw, {p for p, _ in found}, left_norm.rules)
        if not found and not raw and a != b:
            found = [(part, 'serialization')]  # same JSON, other bytes: not parity
        paths.extend(found)
    status = 'different' if paths else 'normalized'
    return CallParity(left.trace, left.index, left.tool, status, paths, reconciled)


def _missing(exchange, side):
    return CallParity(exchange.trace, exchange.index, exchange.tool, 'different', [('$', side)])


def item_of(trace):
    """Traces of one work item share volatile labels: its setup writes and its journey."""
    for suffix in SETUP_SUFFIXES:
        if trace.endswith(suffix):
            return trace[:-len(suffix)], 0
    return trace, 1


def compare_captures(left, right, rules=VOLATILE_FIELDS):
    """Align `{trace: exchanges}` maps by trace name and position; return a report dict.

    An item's setup traces are read before its journey, so a stamp its setup
    wrote is known when the journey shows it again.
    """
    calls, normalizers = [], {}
    for trace in sorted(set(left) | set(right), key=lambda name: (*item_of(name), name)):
        item = item_of(trace)[0]
        left_norm, right_norm = normalizers.setdefault(item, (Normalizer(rules), Normalizer(rules)))
        a, b = left.get(trace, ()), right.get(trace, ())
        for x, y in zip(a, b):
            calls.append(compare_exchange(x, y, left_norm, right_norm))
        calls.extend(_missing(x, 'only_left') for x in a[len(b):])
        calls.extend(_missing(y, 'only_right') for y in b[len(a):])
    return summarize(calls, rules)


def _kind(trace):
    return 'setup' if trace.endswith(SETUP_SUFFIXES) else 'journey'


def summarize(calls, rules=VOLATILE_FIELDS):
    counts = Counter(call.status for call in calls)
    reconciled = sum((call.reconciled for call in calls), Counter())
    total = len(calls)
    by_kind = {}
    for kind in ('journey', 'setup'):
        members = [c for c in calls if _kind(c.trace) == kind]
        if members:
            equal = sum(1 for c in members if c.status != 'different')
            by_kind[kind] = {'calls': len(members), 'parity_rate': equal / len(members)}
    paths = Counter(generalize(p) for c in calls if c.status == 'different' for p, _ in c.paths)
    return {'schema': SCHEMA, 'calls': total, 'identical': counts['identical'],
            'normalized': counts['normalized'], 'different': counts['different'],
            'parity_rate': (counts['identical'] + counts['normalized']) / total if total else None,
            'byte_identical_rate': counts['identical'] / total if total else None,
            'by_kind': by_kind,
            'volatile_fields': [rule.as_dict() for rule in rules],
            # raw differences each rule absorbed; 0 = the rule was never needed here
            'volatile_reconciled': {rule.name: reconciled.get(rule.name, 0) for rule in rules},
            'path_counts': dict(sorted(paths.items())),
            'differences': [c.as_dict() for c in calls if c.status == 'different']}


def load_views(path):
    """kmp-embedded recall views from a JSON array, a JSON object or JSONL; keyed by `id`."""
    text = Path(path).read_text(encoding='utf-8')
    try:
        value = json.loads(text)
        records = value if isinstance(value, list) else [value]
    except ValueError:
        records = [json.loads(line) for line in text.splitlines() if line.strip()]
    views = {}
    for index, record in enumerate(records):
        view = record.get('view', record) if isinstance(record, dict) else None
        if not isinstance(view, dict) or not isinstance((view.get('rendered') or {}).get('content'), str):
            raise ParityInputError(f'{path}[{index}]: no rendered.content')
        views[str(record.get('id', index))] = view
    return views


def compare_rendered(left, right):
    """Compare `rendered.content` of recall views byte for byte; other view paths are listed."""
    rows, same = [], 0
    for key in sorted(set(left) | set(right)):
        a, b = left.get(key), right.get(key)
        if a is None or b is None:
            rows.append({'id': key, 'content_identical': False,
                         'paths': [{'path': '$', 'kind': 'only_left' if b is None else 'only_right'}]})
            continue
        equal = a['rendered']['content'].encode('utf-8') == b['rendered']['content'].encode('utf-8')
        same += equal
        paths = json_diff(a, b)
        if paths or not equal:
            rows.append({'id': key, 'content_identical': equal,
                         'paths': [{'path': p, 'kind': k} for p, k in paths[:MAX_LISTED_PATHS]]})
    total = len(set(left) | set(right))
    return {'views': total, 'content_identical': same,
            'content_parity_rate': same / total if total else None, 'differences': rows}


def run_machine(root):
    """The machine a determinism run recorded (arch, kernel...), or None: cross-arch parity
    (x86_64 against aarch64) compares two such directories and says which is which."""
    meta = Path(root) / 'determinism-run.json'
    if not meta.is_file():
        return None
    return json.loads(meta.read_text(encoding='utf-8')).get('machine')


def main(argv=None):
    import argparse
    parser = argparse.ArgumentParser(prog='parity', description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('left', type=Path)
    parser.add_argument('right', type=Path)
    parser.add_argument('--rendered', nargs=2, type=Path, metavar=('LEFT_VIEWS', 'RIGHT_VIEWS'),
                        help='also compare rendered.content of kmp-embedded recall views')
    parser.add_argument('--out', type=Path, help='write the report here (must not exist)')
    args = parser.parse_args(argv)
    try:
        report = compare_captures(load_capture(args.left), load_capture(args.right))
        report['machines'] = {'left': run_machine(args.left), 'right': run_machine(args.right)}
        if args.rendered:
            report['rendered'] = compare_rendered(*(load_views(p) for p in args.rendered))
    except HarnessError as error:
        print(str(error), file=sys.stderr)
        return 2
    text = json.dumps(report, indent=2, sort_keys=True, ensure_ascii=False) + '\n'
    if args.out:
        with args.out.open('x', encoding='utf-8') as target:
            target.write(text)
    summary = {k: report[k] for k in ('calls', 'identical', 'normalized', 'different', 'parity_rate',
                                      'byte_identical_rate', 'volatile_reconciled', 'machines')}
    if 'rendered' in report:
        summary['rendered_content_parity_rate'] = report['rendered']['content_parity_rate']
    print(json.dumps(summary, sort_keys=True))
    rendered_ok = 'rendered' not in report or report['rendered']['content_parity_rate'] == 1.0
    return 0 if report['different'] == 0 and rendered_ok else 1


if __name__ == '__main__':
    sys.exit(main())
