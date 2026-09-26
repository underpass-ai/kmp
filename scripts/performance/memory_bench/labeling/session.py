"""The interactive labeling session: one labeler, one question, one pool.

What the labeler sees: the question (the agent's words and the user's own), the
proposed facets, and the pool in its shuffled order with each entry's kind and
text. What they never see: the kernel's order, which source put an entry in the
pool, or anything the kernel answered. They may search the whole about freely
(`search`), open any entry, and mark entries from the pool or from a search.

Derived, not typed: a facet is answerable in the store iff it has answers, and
the question is KNOWN iff some facet has answers. The shape is always typed by
the labeler (`shape singular|enumerative`); `save` refuses without it.
"""
import re
import shlex
import unicodedata

from ..domain.errors import BenchError

HELP = """commands (N = pool number, fN = search hit, or a full ref):
  show                       question, facets and pool
  entry N                    an entry's full text (own text, then texts that come with it)
  search WORDS | "PHRASE"    free search in the about's texts (accents and case folded)
  facet add NAME WORDS...    add a facet asked by the question, in the reader's words
  facet rm NAME              remove a facet
  facet words NAME WORDS...  change a facet's words
  facet rename OLD NEW       rename a facet
  answers NAME N...          these entries answer facet NAME
  related NAME N...          same subject as facet NAME, but they do not answer it
  wrong N...                 entries about another subject
  clear N...                 remove every mark of these entries
  shape singular|enumerative the question's form (required)
  reason REASON|none         optional expected UNKNOWN reason (bench vocabulary)
  note TEXT                  free note kept with the label
  save                       validate and write the label
  quit                       leave (unsaved marks are lost)"""
SNIPPET = 160


def fold(text):
    return unicodedata.normalize('NFKD', text).encode('ascii', 'ignore').decode('ascii').lower()


def search(docs, query, limit=40):
    """Refs of the about entries whose texts match: a quoted phrase, or every word."""
    query = query.strip()
    phrase = len(query) > 1 and query[0] == query[-1] == '"'
    needles = [fold(query[1:-1])] if phrase else [fold(w) for w in query.split() if w]
    if not needles or not any(needles):
        return []
    hits = [doc.ref for doc in docs if all(n in fold(doc.joined()) for n in needles)]
    return hits[:limit]


class LabelingSession:
    def __init__(self, intake, pool, about_docs, previous=None, out=print):
        self.intake, self.pool, self.out = intake, pool, out
        self.docs = {doc.ref: doc for doc in about_docs.entries}
        self.found, self.searches, self.notes, self.shape, self.reason = [], [], None, None, None
        self.facets = [{'name': f['name'], 'words': f['words'], 'answers': set(), 'related': set()}
                       for f in intake.facet_template]
        self.wrong = set()
        if previous is not None:
            self._restore(previous)

    def _restore(self, record):
        gold = record['gold']
        self.facets = [{'name': f['name'], 'words': f['words'], 'answers': set(f['answers']),
                        'related': set(f['related'])} for f in gold['facets']]
        self.wrong = set(gold['wrong_subject'])
        self.shape, self.reason = gold['shape'], gold.get('unknown_reason')
        self.notes, self.searches = record.get('notes'), list(record.get('searches') or [])

    # --- reading ---------------------------------------------------------------------------

    def _marks(self, ref):
        marks = [f'a:{f["name"]}' for f in self.facets if ref in f['answers']]
        marks += [f'r:{f["name"]}' for f in self.facets if ref in f['related']]
        return marks + (['wrong'] if ref in self.wrong else [])

    def _line(self, label, ref):
        doc = self.docs[ref]
        text = ' '.join(doc.text.split())
        text = text if len(text) <= SNIPPET else text[:SNIPPET - 1] + '…'
        marks = self._marks(ref)
        return f'{label:>4}. [{doc.kind}]{" {" + " ".join(marks) + "}" if marks else ""} {text}'

    def show(self):
        call = self.intake.call
        self.out(f'question {self.intake.id}  about {self.intake.about}')
        self.out(f'  asked:    {call["question"]}')
        if call.get('asked_as'):
            self.out(f'  user said: {call["asked_as"]}')
        self.out(f'  anchors:  {", ".join(self.intake.anchors) or "-"}   shape: {self.shape or "(not set)"}')
        self.out('facets:')
        for facet in self.facets:
            self.out(f'  {facet["name"]}: "{facet["words"]}"  answers {len(facet["answers"])}'
                     f'  related {len(facet["related"])}')
        self.out(f'pool ({len(self.pool.refs)} entries, shuffled):')
        for number, ref in enumerate(self.pool.refs, start=1):
            self.out(self._line(str(number), ref))

    def entry(self, token):
        ref = self._ref(token)
        doc = self.docs[ref]
        self.out(f'{ref}  [{doc.kind}]{"" if doc.active else "  (inactive)"}  {" ".join(self._marks(ref))}')
        for index, text in enumerate(doc.texts):
            self.out(('  own: ' if index < (doc.own_count or len(doc.texts)) else '  with: ') + text)

    def search(self, query):
        self.found = search(self.docs.values(), query)
        self.searches.append(query)
        in_pool = {ref: n for n, ref in enumerate(self.pool.refs, start=1)}
        self.out(f'{len(self.found)} hit(s){" (first 40)" if len(self.found) == 40 else ""}:')
        for number, ref in enumerate(self.found, start=1):
            where = f' (pool {in_pool[ref]})' if ref in in_pool else ''
            self.out(self._line(f'f{number}', ref) + where)

    # --- marking ---------------------------------------------------------------------------

    def _ref(self, token):
        if re.fullmatch(r'\d+', token):
            index = int(token) - 1
            if not 0 <= index < len(self.pool.refs):
                raise ValueError(f'no pool entry {token}')
            return self.pool.refs[index]
        if re.fullmatch(r'f\d+', token):
            index = int(token[1:]) - 1
            if not 0 <= index < len(self.found):
                raise ValueError(f'no search hit {token}')
            return self.found[index]
        if token in self.docs:
            return token
        raise ValueError(f'{token!r} is neither a pool number, a search hit nor an entry of the about')

    def _facet(self, name):
        for facet in self.facets:
            if facet['name'] == name:
                return facet
        raise ValueError(f'no facet {name!r}')

    def mark(self, kind, name, tokens):
        refs = [self._ref(t) for t in tokens]
        facet = self._facet(name)
        other = 'related' if kind == 'answers' else 'answers'
        for ref in refs:
            facet[kind].add(ref)
            facet[other].discard(ref)
            if kind == 'answers':
                self.wrong.discard(ref)

    def wrong_subject(self, tokens):
        for ref in (self._ref(t) for t in tokens):
            self.wrong.add(ref)
            for facet in self.facets:
                facet['answers'].discard(ref)

    def clear(self, tokens):
        for ref in (self._ref(t) for t in tokens):
            self.wrong.discard(ref)
            for facet in self.facets:
                facet['answers'].discard(ref)
                facet['related'].discard(ref)

    def facet(self, args):
        if not args:
            raise ValueError('facet add|rm|words|rename ...')
        verb, rest = args[0], args[1:]
        if verb == 'add' and len(rest) >= 2:
            if any(f['name'] == rest[0] for f in self.facets):
                raise ValueError(f'facet {rest[0]!r} exists')
            self.facets.append({'name': rest[0], 'words': ' '.join(rest[1:]), 'answers': set(), 'related': set()})
        elif verb == 'rm' and len(rest) == 1:
            self.facets.remove(self._facet(rest[0]))
        elif verb == 'words' and len(rest) >= 2:
            self._facet(rest[0])['words'] = ' '.join(rest[1:])
        elif verb == 'rename' and len(rest) == 2:
            self._facet(rest[0])['name'] = rest[1]
        else:
            raise ValueError('facet add NAME WORDS... | rm NAME | words NAME WORDS... | rename OLD NEW')

    # --- the gold --------------------------------------------------------------------------

    def gold(self):
        facets = [{'name': f['name'], 'words': f['words'], 'answers': sorted(f['answers']),
                   'related': sorted(f['related']), 'answerable_facet': bool(f['answers'])}
                  for f in self.facets]
        known = any(f['answers'] for f in facets)
        return {'answerable': 'KNOWN' if known else 'UNKNOWN', 'shape': self.shape, 'facets': facets,
                'wrong_subject': sorted(self.wrong), 'chain': None, 'path': None, 'current_ref': None,
                'stale_refs': [], 'nearest_outside': None,
                'unknown_reason': None if known else self.reason, 'absent_terms': [],
                'excluded_refs': [], 'wake_required': []}

    def run(self, lines, save):
        """Read commands from `lines`; `save(gold, notes, searches)` validates and writes."""
        self.out('type `help` for commands')
        for raw in lines:
            line = raw.strip()
            if not line:
                continue
            try:
                words = shlex.split(line)
            except ValueError as failure:
                self.out(f'error: {failure}')
                continue
            command, args = words[0], words[1:]
            try:
                if command == 'quit':
                    return False
                if command == 'save':
                    if self.shape is None:
                        raise ValueError('set the shape first: shape singular|enumerative')
                    self.out(f'saved {save(self.gold(), self.notes, self.searches)}')
                    continue
                self.dispatch(command, args, line)
            except (ValueError, KeyError, BenchError) as failure:  # a refused save keeps the session
                self.out(f'error: {failure}')
        return True

    def dispatch(self, command, args, line):
        if command == 'help':
            self.out(HELP)
        elif command == 'show':
            self.show()
        elif command == 'entry' and len(args) == 1:
            self.entry(args[0])
        elif command == 'search' and args:
            self.search(line.split(None, 1)[1])
        elif command == 'facet':
            self.facet(args)
        elif command in ('answers', 'related') and len(args) >= 2:
            self.mark(command, args[0], args[1:])
        elif command == 'wrong' and args:
            self.wrong_subject(args)
        elif command == 'clear' and args:
            self.clear(args)
        elif command == 'shape' and args in (['singular'], ['enumerative']):
            self.shape = args[0]
        elif command == 'reason' and len(args) == 1:
            self.reason = None if args[0] == 'none' else args[0]
        elif command == 'note' and args:
            self.notes = line.split(None, 1)[1]
        else:
            raise ValueError(f'unknown or incomplete command {command!r}; type help')
