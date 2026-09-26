"""The judged Jev corpus (`crates/kmp-testkit/judged/jev_cases.json`) over stdio, in replay.

A port of the parts of `crates/kmp-testkit/src/bin/jev_kmp_scorecard.rs` the
bench decides with: where `kmp_ask` puts the answer without and with Jev
re-ranking (`ask_*`), whether `kmp_curate mode=paths` finds a route with
declared relations only and with Jev's proposed steps (`path_found_*`,
`proposed_hops_right`, `paths_clean_*`, `avoided_are_bad`), which required
memories a wake delivers on its first page and over every page
(`wake_*_pages_*`), and the review, audit and pre-write check whose applies
change the store the paths run on (`missing_*`, `suspect_*`,
`good_declarations_kept`, `precheck_*`).

Every arm is its own seeded store on the release binary through MCP stdio,
with the store files the scorecard writes. Jev answers only from the cassette
(`KMP_TYPESAFE_CASSETTE`, replay): a warning that a judgement was not in it
fails the run instead of being guessed (`refuse_unrecorded`). The arithmetic
is the scorecard's, including its conventions: a tally over nothing is 1.0,
the mean of no ranks is 1.0.

Not ported (reported by the Rust bin, see `external_scorecards`): the focused
write reviews, `kmp_write_memory` proposals, summaries, labels and the byte and
latency lines, none of which changes the store the ported arms read.
"""
from contextlib import ExitStack
from dataclasses import dataclass, field
import json

from ..domain.errors import BenchError
from .judged_stdio import compact, inline_store_file, require_accepted

CORPUS = 'jev-judged'
DEFAULT_CASES = 'crates/kmp-testkit/judged/jev_cases.json'
DEFAULT_BASELINE = 'docs/development/jev-baseline.tsv'
DEFAULT_CASSETTE = 'crates/kmp-testkit/judged/jev.cassette.json'
ACTOR = 'jev-scorecard'
TYPESAFE = '{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.13.0","timeout_ms":20000}'
RERANK_NARROW = '{"pool_size":40}'
RERANK_WIDE = '{"pool_size":400,"excerpt_chars":300}'
WAKE_FOCUS = '{"pool_size":400,"excerpt_chars":300}'
ASK_TOP = 5
WAKE_MAX_PAGES = 32
UNRECORDED = ('not in the cassette', 'Jev unavailable', 'Jev disabled', 'rerank disabled',
              'rerank unavailable', 'pre-write check skipped')
# Store files per arm, exactly as `seeded_server` writes them.
ARMS = {
    'plain': (),
    'judged': (('typesafe.json', TYPESAFE),),
    'reranked': (('typesafe.json', TYPESAFE), ('rerank.json', RERANK_NARROW)),
    'wide': (('typesafe.json', TYPESAFE), ('rerank.json', RERANK_WIDE)),
    'focused': (('typesafe.json', TYPESAFE), ('wake-focus.json', WAKE_FOCUS)),
}
# The ported columns, in the order jev-baseline.tsv records them.
COLUMNS = ('missing_found', 'missing_typed', 'distractors_rejected', 'missing_found_by_jev',
           'suspect_recall', 'suspect_precision', 'good_declarations_kept', 'precheck_available',
           'precheck_right', 'precheck_right_no_direction', 'ask_mrr_plain', 'ask_mrr_rerank',
           'ask_mrr_wide', 'ask_top5_plain', 'ask_top5_rerank', 'ask_top5_wide',
           'path_found_declared_only', 'path_found_with_jev', 'proposed_hops_right',
           'paths_clean_declared_only', 'paths_clean_with_jev', 'avoided_are_bad',
           'wake_first_page_plain', 'wake_first_page_focused', 'wake_all_pages_plain',
           'wake_all_pages_focused')


class JevUnrecorded(BenchError):
    """A replayed judgement was missing from the cassette, or Jev did not run."""
    code = 'JEV_UNRECORDED'


class WakeRunaway(BenchError):
    code = 'WAKE_RUNAWAY'


def refuse_unrecorded(value):
    warnings = compact(value.get('warnings') if isinstance(value, dict) else None)
    for marker in UNRECORDED:
        if marker in warnings:
            raise JevUnrecorded(f'a judgement was not answered from the cassette: {warnings[:400]}')


def _get(value, *path):
    for key in path:
        if isinstance(key, int):
            value = value[key] if isinstance(value, list) and len(value) > key else None
        else:
            value = value.get(key) if isinstance(value, dict) else None
    return value


def text(value):
    return value if isinstance(value, str) else ''


def _items(value):
    return value if isinstance(value, list) else []


# --- Pure readings ------------------------------------------------------------------------------

@dataclass
class Tally:
    """`Tally`: hits over total; a rate over nothing is 1.0, as in the scorecard."""
    hits: int = 0
    total: int = 0

    def add(self, hit):
        self.total += 1
        self.hits += int(bool(hit))

    def rate(self):
        return 1.0 if self.total == 0 else self.hits / self.total


def mean(values):
    """The scorecard's `mean`: 1.0 over nothing."""
    values = list(values)
    if not values:
        return 1.0
    total = 0.0
    for value in values:
        total += value
    return total / len(values)


def rank(answer, gold):
    """1-based rank of the first proof item that supports, or is, a gold ref."""
    for index, item in enumerate(_items(_get(answer, 'proof', 'evidence'))):
        supports = _items(_get(item, 'supports'))
        identifier = _get(item, 'id')
        if any(support in gold for support in supports if isinstance(support, str)) or \
                (isinstance(identifier, str) and any(identifier.endswith(g) for g in gold)):
            return index + 1
    return None


def reciprocal(position):
    return 0.0 if position is None else 1.0 / position


def continuation(page):
    """`projection.next_action` as (tool, arguments); None on the last page."""
    action = _get(page, 'projection', 'next_action')
    tool = _get(action, 'tool')
    arguments = _get(action, 'arguments')
    if not isinstance(tool, str) or not isinstance(arguments, dict):
        return None
    return tool, arguments


def delivered(page):
    """What a wake hands the reader: the packet without the lists that only name the withheld."""
    page = json.loads(json.dumps(page)) if isinstance(page, dict) else {}
    if isinstance(page.get('proof'), dict):
        page['proof'].pop('missing', None)
    for key in ('page', 'next_action', 'projection', 'warnings'):
        page.pop(key, None)
    return compact(page)


def pair_matches(first, second, pair):
    return (first == pair[0] and second == pair[1]) or (first == pair[1] and second == pair[0])


@dataclass(frozen=True)
class Found:
    item_id: str
    source: str
    target: str
    suggested: str | None
    proposed_by: str


@dataclass(frozen=True)
class Review:
    token: str
    found: tuple
    flagged: frozenset
    flagged_no_direction: frozenset


def read_review_page(page, found, flagged, flagged_no_direction):
    for item in _items(_get(page, 'missing')):
        suggested = _get(item, 'suggested_rel')
        found.append(Found(text(_get(item, 'item_id')), text(_get(item, 'from', 'ref')),
                           text(_get(item, 'to', 'ref')),
                           suggested if isinstance(suggested, str) else None,
                           text(_get(item, 'proposed_by'))))
    for item in _items(_get(page, 'suspect')):
        declaration = (text(_get(item, 'from', 'ref')), text(_get(item, 'rel')),
                       text(_get(item, 'to', 'ref')))
        if any(reason != 'direction' for reason in _items(_get(item, 'reasons'))):
            flagged_no_direction.add(declaration)
        flagged.add(declaration)


def score_review(expected, review, scores):
    """Missing relations, distractors and the suspect audit, as the scorecard tallies them."""
    for gold in expected['missing']:
        hit = next((item for item in review.found
                    if pair_matches(item.source, item.target, gold['pair'])), None)
        scores['missing_found'].add(hit is not None)
        scores['missing_found_by_jev'].add(hit is not None and hit.proposed_by == 'jev')
        if hit is not None:
            scores['missing_typed'].add(hit.suggested is not None and hit.suggested in gold['types'])
    for distractor in expected['distractors']:
        typed = any(pair_matches(item.source, item.target, distractor['pair'])
                    and item.suggested is not None for item in review.found)
        scores['distractors_rejected'].add(not typed)
    bad = {tuple(d) for d in expected['declared_bad']}
    for declaration in expected['declared_bad']:
        scores['suspect_recall'].add(tuple(declaration) in review.flagged)
    for declaration in sorted(review.flagged):
        scores['suspect_precision'].add(declaration in bad)
    for declaration in expected['declared_good']:
        scores['good_declarations_kept'].add(tuple(declaration) not in review.flagged)


def score_paths(case_path, declared_bad, answer, arm, scores):
    """One `mode=paths` answer: found, clean, avoided and (with Jev) proposed hops."""
    edges = [tuple(edge) for edge in case_path['edges']]

    def acceptable(a, b):
        return any(pair_matches(a, b, edge) for edge in edges)

    def bad(hop):
        return any(text(_get(hop, 'rel')) == rel and
                   pair_matches(text(_get(hop, 'from', 'ref')), text(_get(hop, 'to', 'ref')),
                                (source, target))
                   for source, rel, target in declared_bad)

    paths = _items(_get(answer, 'paths'))
    clean = scores['paths_clean_declared_only' if arm == 0 else 'paths_clean_with_jev']
    for path in paths:
        clean.add(not any(_get(hop, 'declared') is True and bad(hop) for hop in _items(_get(path, 'hops'))))
    for avoided in _items(_get(answer, 'avoided')):
        scores['avoided_are_bad'].add(bad(avoided))
    top = _items(_get(answer, 'paths', 0, 'hops'))
    nodes = {text(_get(hop, end, 'ref')) for hop in top for end in ('from', 'to')}
    to = case_path.get('to')
    reaches = to is None or (bool(top) and text(_get(top[-1], 'to', 'ref')) == to)
    hops_ok = all(_get(hop, 'declared') is True or
                  acceptable(text(_get(hop, 'from', 'ref')), text(_get(hop, 'to', 'ref'))) for hop in top)
    found = bool(top) and reaches and hops_ok and all(r in nodes for r in case_path['required'])
    scores['path_found_declared_only' if arm == 0 else 'path_found_with_jev'].add(found)
    if arm == 1:
        for path in paths:
            for hop in _items(_get(path, 'hops')):
                if _get(hop, 'declared') is not True:
                    scores['proposed_hops_right'].add(
                        acceptable(text(_get(hop, 'from', 'ref')), text(_get(hop, 'to', 'ref'))))
    return found


@dataclass
class JevScores:
    tallies: dict = field(default_factory=lambda: {name: Tally() for name in COLUMNS
                                                   if not name.startswith('ask_mrr')})
    ranks: dict = field(default_factory=lambda: {'plain': [], 'rerank': [], 'wide': []})
    cases: int = 0
    lines: list = field(default_factory=list)  # human-readable trace of what each arm did

    def __getitem__(self, name):
        return self.tallies[name]

    def columns(self):
        values = {'ask_mrr_plain': mean(self.ranks['plain']),
                  'ask_mrr_rerank': mean(self.ranks['rerank']),
                  'ask_mrr_wide': mean(self.ranks['wide'])}
        return tuple((name, values[name] if name in values else self.tallies[name].rate())
                     for name in COLUMNS)

    def row_map(self):
        return {'cases': float(self.cases), **dict(self.columns())}


# --- Driving the arms ---------------------------------------------------------------------------

def load_cases(path):
    with open(path, encoding='utf-8') as handle:
        return tuple(json.load(handle)['cases'])


def _seed(server, case):
    for seeded in case['memories']:
        require_accepted(server.call('kmp_ingest', {
            'about': seeded['about'], 'idempotency_key': f'judged:{case["id"]}:{seeded["about"]}',
            'memory': seeded['memory']}), f'case {case["id"]} kmp_ingest {seeded["about"]}')


def _open(stores, case, arm):
    files = tuple(inline_store_file(name, body) for name, body in ARMS[arm])
    return stores.open(f'jev-{case["id"]}-{arm}', files)


def _dimensions(case):
    return {'scope': 'abouts', 'abouts': list(case['abouts'])}


def review(server, case):
    """Every page of `mode=review` on the judged store."""
    page = server.call('kmp_curate', {'mode': 'review', 'about': case['about'],
                                      'dimensions': _dimensions(case), 'max_pairs': 40,
                                      'page': {'entries': 20}})
    refuse_unrecorded(page)
    token = text(_get(page, 'review_token'))
    found, flagged, no_direction = [], set(), set()
    while True:
        read_review_page(page, found, flagged, no_direction)
        following = _get(page, 'next_actions', 0, 'arguments')
        if not isinstance(following, dict):
            break
        page = server.call('kmp_curate', following)
    return Review(token, tuple(found), frozenset(flagged), frozenset(no_direction))


def precheck(server, case, reviewed, scores):
    """One apply per pre-write check, so each doubt is attributable (writes the relation)."""
    for check in case['expected']['precheck']:
        item = next((i for i in reviewed.found if pair_matches(i.source, i.target, check['pair'])), None)
        scores['precheck_available'].add(item is not None)
        if item is None:
            scores.lines.append(f'precheck {check["pair"][0]} ~ {check["pair"][1]}: not reviewed')
            continue
        owner = next((about for about in case['abouts'] if check['from'].startswith(about + ':')), None)
        if owner is None:
            raise BenchError(f'case {case["id"]}: precheck `from` belongs to no selected about')
        applied = server.call('kmp_curate', {
            'mode': 'apply', 'about': owner, 'actor': ACTOR, 'review_token': reviewed.token,
            'accepted': [{'item_id': item.item_id, 'rel': check['rel'], 'why': check['why'],
                          'evidence': check['evidence'], 'reverse': item.source != check['from']}]})
        curate = _get(applied, 'curate')
        refuse_unrecorded(curate)
        doubts = _items(_get(curate, 'doubted'))
        doubted = bool(doubts)
        no_direction = any(reason != 'direction' for doubt in doubts
                           for reason in _items(_get(doubt, 'reasons')))
        scores['precheck_right'].add(doubted == check['doubt'])
        scores['precheck_right_no_direction'].add(no_direction == check['doubt'])


def asks(servers, case, scores):
    for ask in case['expected']['ask']:
        arguments = {'about': case['about'], 'question': ask['question'],
                     'dimensions': _dimensions(case),
                     'budget': {'depth': 3, 'tokens': 2048, 'max_entries': 10}}
        positions = []
        for arm in ('plain', 'reranked', 'wide'):
            answered = servers[arm].call('kmp_ask', arguments)
            refuse_unrecorded(answered)
            positions.append(rank(answered, ask['gold']))
        for name, top, position in zip(('plain', 'rerank', 'wide'),
                                       ('ask_top5_plain', 'ask_top5_rerank', 'ask_top5_wide'), positions):
            scores.ranks[name].append(reciprocal(position))
            scores[top].add(position is not None and position <= ASK_TOP)
        scores.lines.append('ask ' + ' '.join('-' if p is None else f'#{p}' for p in positions)
                            + f' {ask["question"]}')


def paths(servers, case, scores):
    for case_path in case['expected'].get('paths', ()):
        arguments = {'mode': 'paths', 'about': case['about'], 'dimensions': _dimensions(case),
                     'from': case_path['from'], 'max_hops': 6}
        if case_path.get('to') is not None:
            arguments['to'] = case_path['to']
        shown = []
        for arm, name in enumerate(('plain', 'judged')):
            answer = servers[name].call('kmp_curate', arguments)
            if arm == 1:
                refuse_unrecorded(answer)
            found = score_paths(case_path, case['expected']['declared_bad'], answer, arm, scores)
            shown.append(f'{name} {"found" if found else "not found"}')
        scores.lines.append(f'paths {case_path["from"]} -> {case_path.get("to")} | ' + ' | '.join(shown))


def wakes(servers, case, scores):
    for wake in case['expected'].get('wake', ()):
        arguments = {'about': case['about'], 'intent': wake['intent'],
                     'dimensions': _dimensions(case), 'budget': {'max_bytes': 10000}}
        for name, first_key, all_key in (('plain', 'wake_first_page_plain', 'wake_all_pages_plain'),
                                         ('focused', 'wake_first_page_focused',
                                          'wake_all_pages_focused')):
            page = servers[name].call('kmp_wake', arguments)
            refuse_unrecorded(page)
            first = delivered(page)
            everything, pages = first, 1
            while (following := continuation(page)) is not None:
                if pages == WAKE_MAX_PAGES:
                    raise WakeRunaway(f'wake {wake["intent"]!r} still continues after {WAKE_MAX_PAGES} pages')
                page = servers[name].call(*following)
                refuse_unrecorded(page)
                everything += delivered(page)
                pages += 1
            for required in wake['required']:
                scores[first_key].add(required in first)
                scores[all_key].add(required in everything)
            scores.lines.append(f'wake {name} {pages} pages | {wake["intent"]}')


def run_case(stores, case, scores):
    """All five arms of one case, open together as the scorecard keeps them."""
    with ExitStack() as stack:
        servers = {}
        for name in ARMS:
            servers[name] = stack.enter_context(_open(stores, case, name))
            _seed(servers[name], case)
        reviewed = review(servers['judged'], case)
        score_review(case['expected'], reviewed, scores)
        precheck(servers['judged'], case, reviewed, scores)
        asks(servers, case, scores)
        paths(servers, case, scores)
        wakes(servers, case, scores)
    scores.cases += 1


def run_collection(stores, cases):
    scores = JevScores()
    for case in cases:
        run_case(stores, case, scores)
    return scores
