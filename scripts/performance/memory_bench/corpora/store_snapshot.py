"""Read a copy of a real KMP SQLite store into plain, frozen documents.

This is the only module that knows the store's tables. It opens the copy
read-only (`mode=ro`) and never the live store: the caller passes the path of
a copy, and a path under the live data directory is refused. Each memory entry
becomes one `EntryDoc` whose `texts` hold everything a reader could meet with
it — its text, its English summary, the evidence that supports it and the
explanations of the entry-to-entry relations that touch it — so "a term occurs
in this entry" is judged generously, which is the safe side for UNKNOWN gold.
"""
from dataclasses import dataclass
import json
from pathlib import Path
import sqlite3

from ..domain import cachekey
from .errors import StoreUnreadable

LIVE_STORE_PARTS = ('.local/share/kmp', '.config/kmp')
SUMMARY_KEYS = ('summary_en', 'search_summary')
SEARCH_SUMMARY_KEY = 'summary_en'  # kmp_domain::SearchSummary::METADATA_KEY
EXPLANATION_KEYS = ('rationale', 'motivation', 'evidence')


@dataclass(frozen=True)
class EntryDoc:
    ref: str
    about: str
    kind: str
    active: bool
    text: str
    texts: tuple  # text first, then summaries, evidence and relation explanations
    own_count: int = 0  # how many of `texts` are the entry's own (text, summaries, evidence)

    def joined(self):
        return '\n'.join(self.texts)

    def own(self):
        """The entry's own words, without the explanations of relations that touch it."""
        return '\n'.join(self.texts[:self.own_count or len(self.texts)])


@dataclass(frozen=True)
class AboutDocs:
    about: str
    entries: tuple  # EntryDoc, sorted by ref
    about_texts: tuple  # what the ranker reads the about's language from
    neighbors: tuple  # (ref, (neighbor refs...)) over entry-to-entry relations, sorted
    # Whether a memory of the about carries an English search summary: the ranker's
    # fallback language when `about_texts` read as no language (`ProbeScope`).
    carries_search_summary: bool = False

    def neighbor_map(self):
        return dict(self.neighbors)


@dataclass(frozen=True)
class StoreSnapshot:
    abouts: tuple  # AboutDocs, sorted by about

    def about(self, name):
        for about in self.abouts:
            if about.about == name:
                return about
        raise StoreUnreadable(f'about {name!r} is not in the store copy')

    def digest(self):
        """Content identity of what the generator read (independent of SQLite pages and WAL)."""
        return cachekey.digest([[a.about, [[e.ref, e.kind, e.active, e.own_count, list(e.texts)] for e in a.entries],
                                 [[r, list(n)] for r, n in a.neighbors]]
                                + (['carries_search_summary'] if a.carries_search_summary else [])
                                for a in self.abouts])


def _refuse_live(path):
    text = str(Path(path).expanduser().resolve())
    if any(part in text for part in LIVE_STORE_PARTS):
        raise StoreUnreadable(f'refusing to read a live KMP store: {text}; pass a copy')


def _database(store_dir):
    store_dir = Path(store_dir)
    for candidate in (store_dir / 'store' / 'kernel.sqlite3', store_dir / 'kernel.sqlite3', store_dir):
        if candidate.is_file():
            return candidate
    raise StoreUnreadable(f'no kernel.sqlite3 under {store_dir}')


def _payload(properties):
    try:
        return json.loads(properties.get('memory_payload_json') or '{}')
    except ValueError as failure:
        raise StoreUnreadable(f'unreadable memory payload: {failure}') from failure


def _rows(path):
    try:
        connection = sqlite3.connect(f'file:{path}?mode=ro&immutable=1', uri=True)
    except sqlite3.Error as failure:
        raise StoreUnreadable(f'{path}: {failure}') from failure
    try:
        nodes = {key: json.loads(value) for key, value in connection.execute('SELECT k, v FROM nodes')}
        details = {key: json.loads(value).get('detail', '')
                   for key, value in connection.execute('SELECT k, v FROM details')}
        relations = [(source, target, kind, json.loads(value)) for source, target, kind, value
                     in connection.execute('SELECT k1, k2, k3, v FROM relations_by_source')]
    except (sqlite3.Error, ValueError) as failure:
        raise StoreUnreadable(f'{path}: {failure}') from failure
    finally:
        connection.close()
    return nodes, details, relations


def _metadata(properties, payload):
    """`persisted_memory_metadata` (bundle_views.rs): `payload_metadata`, else `metadata`, as
    JSON; the payload's own `metadata` when neither property is there."""
    for key in ('payload_metadata', 'metadata'):
        if key in properties:
            try:
                value = json.loads(properties[key])
            except (TypeError, ValueError):
                return {}
            return value if isinstance(value, dict) else {}
    value = payload.get('metadata')
    return value if isinstance(value, dict) else {}


def carries_search_summary(properties, payload):
    """Whether a node carries a non-empty English search summary.

    The ranker (`bundle_carries_search_summary`, answer_recall_context.rs) also lints the
    summary against the node text; the lint lives in the kernel and is not copied here, so
    this reads presence only and may say yes where the lint would say no. It matters only
    for an about whose texts read as no language (a mixed store)."""
    summary = _metadata(properties, payload).get(SEARCH_SUMMARY_KEY)
    return isinstance(summary, str) and bool(summary.strip())


def _explanations(value):
    return [value[key] for key in EXPLANATION_KEYS if isinstance(value.get(key), str) and value[key].strip()]


def _doc(ref, about, item):
    own = tuple(dict.fromkeys(t for t in item['texts'] if t))
    linked = tuple(t for t in dict.fromkeys(item['linked']) if t and t not in own)
    return EntryDoc(ref, about, item['kind'], item['active'], item['texts'][0], own + linked, len(own))


def read_store(store_dir):
    """A StoreSnapshot of every about in the copy at `store_dir`."""
    _refuse_live(store_dir)
    nodes, details, relations = _rows(_database(store_dir))
    entries, about_texts, summarized = {}, {}, set()
    for ref, node in nodes.items():
        properties = node.get('properties') or {}
        about = properties.get('memory_about')
        if not about:
            continue
        about_texts.setdefault(about, []).extend(
            text for text in (node.get('summary'), details.get(ref)) if text)
        entry = properties.get('memory_entity_kind') == 'memory_entry'
        try:
            payload = _payload(properties)
        except StoreUnreadable:
            if entry:
                raise
            payload = {}  # a non-entry node's payload is read for its summary flag only
        if carries_search_summary(properties, payload):
            summarized.add(about)
        if not entry:
            continue
        metadata = payload.get('metadata') or {}
        texts = [payload.get('text') or node.get('summary') or '']
        texts += [metadata[key] for key in SUMMARY_KEYS if isinstance(metadata.get(key), str)]
        entries[ref] = {'about': about, 'kind': properties.get('entry_kind') or node.get('node_kind'),
                        'active': node.get('status') == 'ACTIVE', 'texts': texts, 'linked': []}
    links = {}
    for source, target, kind, value in relations:
        source_about = (nodes.get(source, {}).get('properties') or {}).get('memory_about')
        if source_about:
            about_texts.setdefault(source_about, []).extend(_explanations(value))
        if kind == 'supports' and target in entries and source not in entries:
            evidence = _payload(nodes.get(source, {}).get('properties') or {}).get('text')
            if evidence:
                entries[target]['texts'].append(evidence)
        elif source in entries and target in entries:
            for ref in (source, target):
                entries[ref]['linked'].extend(_explanations(value))
            links.setdefault(source, set()).add(target)
            links.setdefault(target, set()).add(source)
    abouts = []
    for about in sorted({item['about'] for item in entries.values()}):
        docs = tuple(_doc(ref, about, item) for ref, item in sorted(entries.items()) if item['about'] == about)
        refs = {doc.ref for doc in docs}
        neighbors = tuple((ref, tuple(sorted(links[ref] & refs)))
                          for ref in sorted(refs) if links.get(ref, set()) & refs)
        abouts.append(AboutDocs(about, docs, tuple(about_texts.get(about, ())), neighbors,
                                about in summarized))
    return StoreSnapshot(tuple(abouts))
