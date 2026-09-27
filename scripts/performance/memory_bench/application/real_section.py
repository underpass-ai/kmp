"""The real-store section: B-real, the hard negatives and the identifier guards.

All three corpora are private and ask the same frozen copy of the real store
(`<private root>/<date>/store/`), so they share one run per arm and one report,
written to the private cache (`$MEMORY_BENCH_PRIVATE_ROOT/cache`). Each arm opens
a fresh copy of the frozen data directory (`build = copy` in the store key, with
the reader's SHA-256): the store is shared by content, not rebuilt.

- B-real runs only when the freeze has `gold.jsonl` (labels resolved); otherwise
  it is skipped and the reason becomes a limitation of the report.
- The hard negatives and guards come from `<freeze>/negatives/`. Before any
  candidate runs on them, the rules they were generated under must be the rules
  `config/negatives.sha256` pins and be registered in the private registry
  (`negative_rules.require_registered`); otherwise the section refuses to run.
- A mode may take a stratified subset: the first `per_stratum` questions (by id)
  of every stratum (hard negatives: type x form; guards: family x polarity).
"""
from pathlib import Path
import os
import re

from ..corpora import negative_rules, real_freeze
from ..domain import cachekey
from ..domain.errors import BenchError
from ..domain.question import load_questions
from ..runtime.layout import PRIVATE_ROOT_ENV, private_layout
from . import sections
from .run_questions import StoreRef

NAME = 'real-store'
NEGATIVES = ('negatives', 'hard-negatives.jsonl')
GUARDS = ('negatives', 'identifier-guards.jsonl')
NEGATIVE_STRATA = ('form:',)
GUARD_STRATA = ('family:', 'polarity:')
DATED = re.compile(r'\d{4}-\d{2}-\d{2}')


class RealSectionRefused(BenchError):
    code = 'REAL_SECTION_REFUSED'


def find_freeze(freeze=None, private_root=None, env=None):
    """The freeze directory: `freeze`, else the latest `YYYY-MM-DD` one under the private root; None if neither."""
    if freeze:
        return real_freeze.FreezeDir.at(freeze)
    env = os.environ if env is None else env
    root = private_root or env.get(PRIVATE_ROOT_ENV)
    if not root:
        return None
    # Only a date-named freeze is a default: a named one (`breal-ext`) sorts after
    # every date and would silently replace the questions; pass it with --freeze.
    dated = sorted(p for p in Path(root).expanduser().iterdir()
                   if DATED.fullmatch(p.name) and (p / 'freeze.json').is_file()) \
        if Path(root).expanduser().is_dir() else []
    return real_freeze.FreezeDir.at(dated[-1]) if dated else None


def stratified(questions, per_stratum, prefixes):
    """The first `per_stratum` questions (by id) of each stratum; 0 keeps every question."""
    if not per_stratum:
        return tuple(sorted(questions, key=lambda q: q.id))
    strata = {}
    for question in sorted(questions, key=lambda q: q.id):
        key = (question.type,) + tuple(t for t in question.tags if t.startswith(prefixes))
        strata.setdefault(key, []).append(question)
    return tuple(q for key in sorted(strata) for q in strata[key][:per_stratum])


def require_rules(questions, private_root):
    """Every generated question was made under the pinned and registered negative rules."""
    pinned = negative_rules.registered_sha256()
    found = sorted({(q.source or {}).get('rules_sha256') for q in questions})
    if found != [pinned]:
        raise RealSectionRefused(f'negatives were generated under rules {found}, config pins {pinned}: '
                                 'regenerate them (corpora.hard_negatives generate)')
    negative_rules.require_registered(pinned, private_root)
    return pinned


def select(fd, strata):
    """(questions, limitations) of the section under the mode's strata."""
    questions, notes = [], []
    if strata.b_real:
        if fd.gold.is_file():
            questions += load_questions(fd.gold)
        else:
            notes.append('B-real skipped: the freeze has no gold.jsonl yet (label, agree and resolve first)')
    generated = []
    for parts, per, prefixes in ((NEGATIVES, strata.hard_negatives_per_stratum, NEGATIVE_STRATA),
                                 (GUARDS, strata.guards_per_stratum, GUARD_STRATA)):
        path = fd.path(*parts)
        if not path.is_file():
            notes.append(f'{parts[-1]} missing in the freeze: not run')
            continue
        generated += stratified(load_questions(path), per, prefixes)
    if generated:
        require_rules(generated, fd.private_root)
    return tuple(questions) + tuple(generated), notes


def store_for(fd, manifest, binary):
    label = f'b-real {fd.root.name}'
    source = cachekey.bundle_source(content_digest=manifest['bundle']['content_digest'], label=label)
    key = cachekey.store_key(source=source, n=manifest['bundle']['event_count'],
                             bundle_format=manifest['bundle']['bundle_format'],
                             reader_sha256=binary.sha256, build='copy')
    return StoreRef(key, manifest['bundle']['content_digest'], label, fd.data_dir)


def run(specs, mode, freeze=None, private_root=None, replica_nonce=None):
    """The section's SectionResult: skipped without a freeze, failed on a bench error."""
    try:
        fd = find_freeze(freeze, private_root)
        if fd is None:
            return sections.skipped(NAME, f'no private root: set {PRIVATE_ROOT_ENV} or pass --freeze')
        manifest = real_freeze.verify(fd)
        questions, notes = select(fd, mode.real)
    except BenchError as error:
        return sections.failed(NAME, error)
    if not questions:
        return sections.skipped(NAME, '; '.join(notes) or 'no question in the freeze')
    layout = private_layout(fd.private_root)
    stores = [store_for(fd, manifest, spec.binary) for spec in specs]
    return sections.run_section(NAME, specs, stores, questions, mode, layout, replica_nonce,
                                limitations=notes)
