"""synth-v1 text: vocabularies, templates and the sentences entries and questions are made of.

Everything here is a pure function of its arguments and a counted SplitMix64 stream; no
floats are drawn, so every platform renders the same bytes. Three rules keep the gold
honest, and `tests/test_generator.py` checks each of them:

- **Facet keywords live only in facet sentences of anchored entries.** The filler that pads
  an anchored entry never contains a facet keyword (`FACETS[*].keyword`), and no facet
  sentence carries another facet's keyword, so "anchor X never co-occurs with attribute A"
  is decided by which facet sentences X's family holds.
- **Anchors live only in family entries.** Nothing else renders an anchor-shaped token
  (`C<m>.<n>`, `#<5+ digits>`, `v0.<m>.<n>`), so an anchor the generator never emits has
  df = 0 at every level.
- **Bare numbers below 10 000.** Filler numbers stay under 10 000 while issue anchors are
  5-digit and chain ids use their own prefixes, so a perturbed anchor cannot be matched by
  a stray number either.
"""
import bisect
from dataclasses import dataclass
import csv
import datetime
from functools import lru_cache
from pathlib import Path

HERE = Path(__file__).resolve().parent
TIME_ORIGIN = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
TIME_ORIGIN_TEXT = '2026-01-01T00:00:00Z'
# Longest entry text the generator writes. The real store's tail reaches 40 000
# characters (p99 21 558); capping it keeps a 10^5 world near 100 MB. The calibration
# report states the length KS with the cap in place.
MAX_TEXT_CHARS = 4000


def instant(minutes):
    """RFC 3339 UTC instant `minutes` after the time origin (integer arithmetic only)."""
    return (TIME_ORIGIN + datetime.timedelta(minutes=minutes)).strftime('%Y-%m-%dT%H:%M:%SZ')


def stamp_text(minutes):
    """A human date and time for dated statements, e.g. `2026-01-01 08:12`."""
    return (TIME_ORIGIN + datetime.timedelta(minutes=minutes)).strftime('%Y-%m-%d %H:%M')


def day_text(minutes):
    """A human date for sentences, e.g. `2026-01-03`."""
    return (TIME_ORIGIN + datetime.timedelta(minutes=minutes)).strftime('%Y-%m-%d')


# --- Facets: the attributes a family of anchored entries states ----------------------------

@dataclass(frozen=True)
class Facet:
    name: str          # gold facet name
    words: str         # how a reader names it
    keyword: str       # lowercase word prefix every sentence of this facet carries
    kind: str          # entry kind
    template: str      # {A} anchor, {S} subject, {V} value
    question: str      # singular question, {A} anchor, {S} subject
    values: tuple


FACETS = (
    Facet('cache-engine', 'the cache engine', 'cache', 'decision',
          '{A} moves the shared cache of {S} to {V} after the spring review.',
          'Which cache engine does {A} choose for {S}?',
          ('Valkey', 'Redis', 'Memcached', 'Hazelcast', 'Dragonfly')),
    Facet('queue-broker', 'the queue broker', 'queue', 'decision',
          '{A} puts the job queue of {S} on {V}.',
          'Which queue broker does {A} set for {S}?',
          ('RabbitMQ', 'Kafka', 'NATS', 'Pulsar', 'ActiveMQ')),
    Facet('database', 'the primary database', 'database', 'decision',
          '{A} keeps the primary database of {S} on {V}.',
          'Which primary database does {A} keep {S} on?',
          ('PostgreSQL', 'MySQL', 'CockroachDB', 'SQLite', 'Cassandra')),
    Facet('deploy-target', 'the deployment target', 'deploy', 'decision',
          '{A} deploys {S} to {V} from now on.',
          'Where does {A} deploy {S}?',
          ('Kubernetes', 'Nomad', 'ECS', 'Fly', 'OpenShift')),
    Facet('owner', 'the owning team', 'owner', 'decision',
          '{A} makes the {V} team the owner of {S}.',
          'Which team does {A} make the owner of {S}?',
          ('Falcon', 'Heron', 'Lynx', 'Otter', 'Puma')),
    Facet('deadline', 'the deadline', 'deadline', 'constraint',
          '{A} sets the deadline for {S} to {V}.',
          'What deadline does {A} set for {S}?',
          ('March 14', 'April 2', 'May 30', 'June 11', 'July 7')),
    Facet('latency-budget', 'the latency budget', 'latency', 'constraint',
          '{A} caps the p99 latency of {S} at {V} ms.',
          'What latency budget does {A} give {S}?',
          ('120', '250', '400', '800', '1500')),
    Facet('region', 'the hosting region', 'region', 'decision',
          '{A} pins {S} to the {V} region.',
          'Which region does {A} pin {S} to?',
          ('eu-west-1', 'eu-central-1', 'us-east-2', 'ap-south-1', 'sa-east-1')),
    Facet('language', 'the implementation language', 'language', 'decision',
          '{A} picks {V} as the implementation language of {S}.',
          'Which implementation language does {A} pick for {S}?',
          ('Rust', 'Go', 'Kotlin', 'Python', 'Elixir')),
    Facet('auth', 'the authentication method', 'authenticat', 'constraint',
          '{A} makes {S} authenticate callers with {V}.',
          'How does {A} make {S} authenticate callers?',
          ('mTLS', 'OAuth', 'signed tokens', 'SAML', 'Kerberos')),
    Facet('retention', 'the log retention', 'retention', 'constraint',
          '{A} sets the log retention of {S} to {V} days.',
          'What log retention does {A} set for {S}?',
          ('7', '14', '30', '90', '365')),
    Facet('cost', 'the monthly cost', 'cost', 'constraint',
          '{A} caps the monthly cost of {S} at {V} EUR.',
          'What monthly cost does {A} allow for {S}?',
          ('420', '900', '1800', '3200', '6400')),
    Facet('rollback', 'the rollback plan', 'rollback', 'instruction',
          '{A} defines the rollback plan of {S}: {V}.',
          'What rollback plan does {A} define for {S}?',
          ('restore the previous image', 'flip the feature flag off',
           'replay the saved snapshot', 'drain and redirect traffic', 'revert the schema')),
    Facet('risk', 'the main risk', 'risk', 'observation',
          '{A} lists {V} as the main risk for {S}.',
          'What main risk does {A} list for {S}?',
          ('losing in-flight messages', 'a vendor lock-in', 'stale reads',
           'a noisy neighbour', 'clock skew')),
    Facet('verification', 'the verification check', 'verif', 'success_path',
          '{A} verifies {S} with the {V}.',
          'How does {A} verify {S}?',
          ('nightly soak run', 'contract suite', 'chaos drill', 'canary rollout', 'load replay')),
    Facet('status', 'the current status', 'status', 'observation',
          '{A} marks the status of {S} as {V}.',
          'What status does {A} give {S}?',
          ('blocked on the vendor', 'in review', 'shipped', 'on hold', 'half done')),
    Facet('replicas', 'the replica count', 'replica', 'decision',
          '{A} scales {S} to {V} replicas.',
          'How many replicas does {A} give {S}?',
          ('2', '3', '5', '8', '12')),
    Facet('timeout', 'the request timeout', 'timeout', 'constraint',
          '{A} sets the request timeout of {S} to {V} seconds.',
          'What request timeout does {A} set for {S}?',
          ('5', '10', '30', '60', '120')),
)
FACET_BY_NAME = {facet.name: facet for facet in FACETS}
FACET_KEYWORDS = tuple(facet.keyword for facet in FACETS)


def facet_sentence(facet, anchor, subject, value):
    return facet.template.format(A=anchor, S=subject, V=value)


# --- Anchors ---------------------------------------------------------------------------------
# Three families of identifier, each with a perturbation the generator never emits.

ANCHOR_STYLES = ('corte', 'issue', 'version')


def anchor(style, group_index, family):
    """The anchor of family `family` (0..2) of global group `group_index`."""
    major = group_index + 1
    if style == 'corte':
        return f'C{major}.{2 * (family + 1)}'            # minors 2, 4, 6 only
    if style == 'issue':
        return f'#{10000 + 10 * group_index + 2 * family}'  # always even, >= 10000
    return f'v0.{major}.{2 * family}'                    # patch always even


def absent_anchor(style, group_index, family):
    """A look-alike of `anchor(...)` that no block ever renders (C6.4 -> C6.24, #188 -> odd)."""
    major = group_index + 1
    if style == 'corte':
        return f'C{major}.2{2 * (family + 1)}'
    if style == 'issue':
        return f'#{10000 + 10 * group_index + 2 * family + 1}'
    return f'v0.{major}.{2 * family + 1}'


# --- Filler: sentences that pad an entry to its sampled length ------------------------------
# Filler carries the vocabulary tail the real store has: template words are few and
# frequent, slot words come from mid-sized lists, and invented words are drawn from a Zipf
# law (s = 1) over `PSEUDO_POOL` ranks, which is what gives the document-frequency curve
# its long df = 1 tail and its df 2-20 shoulder. Numbers, dates and times are rationed so
# the density of digit-bearing terms matches the store's.

NAMES = ('Mara', 'Iker', 'Noa', 'Tomas', 'Lucia', 'Arne', 'Sofia', 'Bruno', 'Ines', 'Karim',
         'Elsa', 'Pau', 'Greta', 'Hugo', 'Nadia', 'Oriol', 'Vera', 'Dario', 'Julia', 'Omar',
         'Clara', 'Emil', 'Rosa', 'Teo', 'Alba', 'Marek', 'Lena', 'Yusuf', 'Irene', 'Samir',
         'Petra', 'Ivo', 'Carmen', 'Anton', 'Zoe', 'Felix', 'Maite', 'Joel', 'Ada', 'Ruben')
DAYS = ('Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday')
VERBS = ('reviewed', 'sketched', 'drafted', 'renamed', 'archived', 'merged', 'split', 'cleaned',
         'sorted', 'shared', 'summarised', 'reopened', 'tagged', 'annotated', 'rewrote',
         'polished', 'trimmed', 'regrouped', 'mapped', 'traced', 'collected', 'compared',
         'bundled', 'labelled', 'discussed', 'flagged', 'untangled', 'parked', 'revisited',
         'simplified', 'reworded', 'catalogued', 'outlined', 'filed', 'moved', 'copied',
         'printed', 'scanned', 'checked', 'counted')
NOUNS = ('notes', 'thread', 'sketch', 'backlog', 'checklist', 'draft', 'diagram', 'agenda',
         'minutes', 'summary', 'board', 'folder', 'page', 'handbook', 'glossary', 'wiki',
         'spreadsheet', 'template', 'proposal', 'outline', 'roadmap', 'retro', 'workshop',
         'survey', 'interview', 'transcript', 'mockup', 'wireframe', 'playbook', 'ledger',
         'inbox', 'calendar', 'pairing', 'demo', 'slides', 'memo', 'ticket', 'branch', 'label',
         'milestone', 'epic', 'story', 'estimate', 'sprint', 'standup', 'huddle', 'channel',
         'digest', 'newsletter', 'questionnaire', 'map', 'journal', 'logbook', 'index',
         'appendix', 'footnote', 'caption', 'heading', 'paragraph', 'section')
ADJECTIVES = ('stale', 'fresh', 'long', 'short', 'messy', 'tidy', 'shared', 'private', 'old',
              'new', 'weekly', 'monthly', 'rough', 'final', 'early', 'late', 'quiet', 'busy',
              'small', 'large', 'second', 'third', 'open', 'closed', 'draft', 'noisy', 'odd',
              'plain', 'handy', 'loose')
FILLER = (
    '{name} {verb} the {adj} {noun} about {p}.',
    '{name} and {other} {verb} the {p} {noun} on {day}.',
    'The {p} {noun} was {verb} by {name}.',
    '{name} {verb} the {noun} for {p} and {q}.',
    'We {verb} the {adj} {p} {noun} after lunch.',
    'The {noun} on {p} still needs a {adj} pass.',
    '{name} asked {other} to look at the {p} {noun}.',
    'Nothing new on the {p} {noun} since {day}.',
    'The {adj} {noun} mentions {p}, {q} and {r}.',
    '{name} left a comment on the {p} {noun}.',
    'Parked the {p} {noun} until {other} is back.',
    '{name} thinks the {p} {noun} reads better now.',
    'The {noun} for {p} moved to the {adj} {q} folder.',
    'Someone {verb} the {p} {noun} twice by mistake.',
    '{name} {verb} {n} {noun} items about {p}.',
    'The {p} {noun} has {n} open points.',
    '{name} {verb} the {p} {noun} at {time}.',
    'Ticket {p}-{n} was closed after a short call.',
    'Nothing blocking on {p} as of {date}.',
    'The {adj} {p} {noun} is linked from the {q} {noun2}.',
    '{name} wants a {adj} {noun} for {p} before {day}.',
    'Kept the {p} {noun} short on purpose.',
    'The {q} {noun} now points at {p} instead of {r}.',
    '{name} {verb} the {p} and {q} {noun} into one.',
    'Quick pass over the {p} {noun} with {other}.',
    '{name} prefers the {adj} version of the {p} {noun}.',
    'Found an {adj} {noun} under {p} from last year.',
    'The {p} {noun} got a {adj} title in revision {n}.',
    '{other} will take over the {p} {noun} next week.',
    'Pinned the {p} {noun} in room {n} of the {q} space.',
    'Two versions of the {p} {noun} exist; keep the {adj} one.',
    '{name} {verb} the {noun} and dropped {p}.',
    'Handed the {p} {noun} to {other} for a read.',
    'The {p} {noun} lists {q} twice.',
    '{name} doubts the {p} {noun} is still useful.',
    'A {adj} {noun} about {p} came in from {other}.',
    'Swapped {p} for {q} in the {noun}.',
    'The {noun} with {p} sits next to the {q} {noun2}.',
    '{name} {verb} {p} while waiting for {other}.',
    'Lunch chat at {time} drifted to the {p} {noun} again.',
)
COMMON_NUMBERS = (2, 3, 4, 5, 10, 12, 15, 20, 24, 30, 40, 50, 60, 90, 100)
SYLLABLES = ('ka', 'ro', 'mi', 'tu', 'le', 'sa', 'no', 'vi', 'pe', 'da', 'zu', 'fo', 'ri',
             'na', 'bo', 'ge', 'lu', 'ti', 'mo', 'ke', 'ba', 'si', 'po', 'ne', 'ha', 'ju',
             'we', 'yo', 'xa', 'qi', 'vu', 'le', 'ma', 'to', 'ze', 'go', 'fi', 'ru', 'do', 'wa')
PSEUDO_POOL = 3000
PSEUDO_OFFSET = 20
RARE_PSEUDO_ONE_IN = 80
RARE_PSEUDO_SPACE = 10 ** 6


def _iroot(value, degree):
    """floor(value ** (1 / degree)) for a non-negative int, exactly (no floats)."""
    low, high = 0, 1 << (value.bit_length() // degree + 1)
    while low < high:
        middle = (low + high + 1) // 2
        if middle ** degree <= value:
            low = middle
        else:
            high = middle - 1
    return low


def _zipf_cumulative():
    """Zipf-Mandelbrot weights 1 / (rank + 1 + PSEUDO_OFFSET) ** 1.2 as exact integers.

    x ** 1.2 = x * x ** (1/5); the fifth root is taken on x * 10**25 so it carries five
    decimal digits, and every step is integer arithmetic: the table is the same on every
    platform and Python version.
    """
    cumulative, total = [], 0
    for rank in range(PSEUDO_POOL):
        x = rank + 1 + PSEUDO_OFFSET
        total += 10 ** 22 // (x * _iroot(x * 10 ** 25, 5))
        cumulative.append(total)
    return tuple(cumulative)


_ZIPF_CUMULATIVE = _zipf_cumulative()


def pseudo_word(rank):
    """The rank-th invented word: two to five CV syllables, never an English facet keyword."""
    word, value = [], rank
    for _ in range(5):
        word.append(SYLLABLES[value % len(SYLLABLES)])
        value //= len(SYLLABLES)
        if value == 0 and len(word) >= 2:
            break
    return ''.join(word)


def zipf_rank(rng):
    """An invented-word rank: Zipf-Mandelbrot (s = 1.2, offset 20) over PSEUDO_POOL ranks,
    or, one time in RARE_PSEUDO_ONE_IN, a rank from an open space above the pool, which keeps
    the df = 1 tail growing with N the way a real vocabulary does (Heaps' law)."""
    if rng.below(RARE_PSEUDO_ONE_IN) == 0:
        return PSEUDO_POOL + rng.below(RARE_PSEUDO_SPACE)
    return bisect.bisect_right(_ZIPF_CUMULATIVE, rng.below(_ZIPF_CUMULATIVE[-1]))


def skewed(rng, bound):
    """A rank in [0, bound) with heavy low ranks: nested uniform draws, integers only."""
    return rng.below(rng.below(rng.below(bound) + 1) + 1)


def filler_number(rng):
    """A bare number below 10 000: common round values, else small-number-heavy."""
    if rng.below(4) == 0:
        return str(rng.choice(COMMON_NUMBERS))
    return str(rng.below(rng.below(10000) + 1))


def filler_sentence(rng, minutes):
    template = FILLER[rng.below(len(FILLER))]
    stamp = minutes - rng.below(600)
    return template.format(
        name=NAMES[rng.below(len(NAMES))], other=NAMES[rng.below(len(NAMES))],
        verb=VERBS[rng.below(len(VERBS))], noun=NOUNS[rng.below(len(NOUNS))],
        noun2=NOUNS[rng.below(len(NOUNS))], adj=ADJECTIVES[rng.below(len(ADJECTIVES))],
        p=pseudo_word(zipf_rank(rng)), q=pseudo_word(zipf_rank(rng)),
        r=pseudo_word(zipf_rank(rng)), n=filler_number(rng), day=DAYS[rng.below(len(DAYS))],
        date=day_text(max(stamp, 0)), time=f'{rng.below(24):02d}:{rng.below(12) * 5:02d}')


def pad(rng, core, target, minutes):
    """`core` followed by filler sentences until `target` characters (never past the cap)."""
    parts, length = [core], len(core)
    target = min(target, MAX_TEXT_CHARS)
    while length < target:
        sentence = filler_sentence(rng, minutes)
        grown = length + 1 + len(sentence)
        # Keep the sentence only if it lands nearer the target than stopping here does.
        if grown > MAX_TEXT_CHARS or grown - target > target - length:
            break
        parts.append(sentence)
        length = grown
    return ' '.join(parts)


def sample_length(rng, percentiles):
    """A text length drawn from a 101-point percentile table, linear between points."""
    index = rng.below(100)
    low, high = percentiles[index], percentiles[index + 1]
    return low + (rng.below(high - low + 1) if high > low else 0)


# --- Causal / motivational chains ------------------------------------------------------------

# (statement, question form). {H} the chain's host; statements are prefixed by the node id.
CHAIN_EVENTS = (
    ('the disk on {H} filled up during the nightly export', 'did the disk on {H} fill up'),
    ('{H} switched itself to read-only mode', 'did {H} switch to read-only mode'),
    ('the billing batch on {H} stalled for two hours', 'did the billing batch on {H} stall'),
    ('the invoice run for {H} slipped to the next day', 'did the invoice run for {H} slip'),
    ('support opened a priority ticket about {H}', 'did support open a priority ticket about {H}'),
    ('the on-call rota for {H} was doubled', 'was the on-call rota for {H} doubled'),
    ('the release train for {H} was frozen', 'was the release train for {H} frozen'),
    ('the vendor escalated the contract for {H}', 'did the vendor escalate the contract for {H}'),
    ('the nightly export of {H} was moved to the weekend', 'was the nightly export of {H} moved'),
    ('finance asked for a refund report on {H}', 'did finance ask for a refund report on {H}'),
)
CHAIN_DECISIONS = (
    ('we decided to add a second disk to {H}', 'did we add a second disk to {H}'),
    ('we agreed to split the export of {H} in two', 'did we split the export of {H}'),
    ('we chose to pause new signups on {H}', 'did we pause new signups on {H}'),
    ('we approved a spending freeze for {H}', 'did we approve a spending freeze for {H}'),
    ('we moved the weekly sync of {H} to mornings', 'did we move the weekly sync of {H}'),
)
CHAIN_LINKS = ('after {P}', 'because of {P}', 'following {P}', 'as a result of {P}')
CHAIN_ID_PREFIXES = ('INC', 'CHG', 'OPS')


def chain_sentence(node_id, statement, host, predecessor=None, link=None):
    text = f'{node_id}: {statement.format(H=host)}'
    if predecessor is not None:
        text += ' ' + link.format(P=predecessor)
    return text + '.'


# --- Supersession ----------------------------------------------------------------------------

@dataclass(frozen=True)
class Changing:
    name: str
    words: str
    statement: str     # {D} date, {S} subject, {V} value
    now: str           # current-state question
    then: str          # historical question (asked with as_of / interval)
    values: tuple


CHANGING = (
    Changing('broker', 'the message broker', 'From {D}, {S} runs its message bus on {V}.',
             'Which message bus does {S} run on now?', 'Which message bus did {S} run on?',
             ('Kafka', 'RabbitMQ', 'NATS', 'Pulsar', 'Redpanda')),
    Changing('storage-tier', 'the storage tier', 'From {D}, {S} writes its archives to {V}.',
             'Where does {S} write its archives now?', 'Where did {S} write its archives?',
             ('Glacier', 'Nearline', 'Coldline', 'Wasabi', 'Backblaze')),
    Changing('oncall', 'the on-call team', 'From {D}, the pager for {S} goes to team {V}.',
             'Which team gets the pager for {S} now?', 'Which team got the pager for {S}?',
             ('Aster', 'Birch', 'Cedar', 'Dahlia', 'Elm')),
)


# --- Hubs, hub notes, distractors, rare identifiers -----------------------------------------

HUBS = (
    ('gw-01', 'The edge gateway gw-01 fronts every public endpoint of the platform.'),
    ('idp-02', 'The identity provider idp-02 signs every session of the platform.'),
    ('bus-03', 'The event bus bus-03 carries every domain event of the platform.'),
    ('obs-04', 'The observability stack obs-04 collects every trace of the platform.'),
)
HUB_NOTES = (
    ('{S} routes its {P} traffic through {H}.', 'What does {S} route through {H}?'),
    ('{S} relies on {H} for its {P} sessions.', 'What does {S} rely on {H} for?'),
    ('{S} publishes its {P} events on {H}.', 'Which events does {S} publish on {H}?'),
    ('{S} ships its {P} traces to {H}.', 'Which traces does {S} ship to {H}?'),
)
# A target and its distractor differ only in the subject and the value.
DISTRACTORS = (
    ('{S} caches user sessions in {V} since the spring audit.',
     'Where does {S} cache user sessions?', ('Valkey', 'Hazelcast', 'Memcached', 'Ignite')),
    ('{S} sends its weekly report to the {V} mailbox.',
     'Which mailbox gets the weekly report of {S}?', ('finance', 'legal', 'ops', 'sales')),
    ('{S} reads feature flags from {V} at startup.',
     'Where does {S} read feature flags from?', ('Unleash', 'Flagsmith', 'etcd', 'Consul')),
)
RARE_NOTES = (
    '{R} documents the migration notes that {name} wrote for the {p} rollout.',
    '{R} records the vendor answer about {p} that {name} forwarded.',
    '{R} summarises the {p} workshop that {name} ran with {other}.',
)
RARE_QUESTION = 'What does {R} record?'


# --- Data tables (paraphrases and the Spanish-English lexicon) ------------------------------

def _tsv(name):
    lines = [line for line in (HERE / name).read_text(encoding='utf-8').splitlines()
             if line and not line.startswith('#')]
    return tuple(csv.DictReader(lines, delimiter='\t'))


@lru_cache(maxsize=None)
def paraphrase_table():
    rows = _tsv('paraphrase.tsv')
    return (tuple(row for row in rows if row['role'] == 'subject'),
            tuple(row for row in rows if row['role'] == 'predicate'))


@lru_cache(maxsize=None)
def lexicon_table():
    rows = _tsv('lexicon_en_es.tsv')
    return (tuple(row for row in rows if row['role'] == 'subject'),
            tuple(row for row in rows if row['role'] == 'fact'))


PARAPHRASE_VALUES = ('Glacierfold', 'Northwind', 'Brightline', 'Oakhurst', 'Marlowe',
                     'Quillon', 'Tessaract', 'Varnholm', 'Kestrelworks', 'Duskwater')
CROSSLANG_VALUES = ('Valdemar', 'Ostrova', 'Pellucid', 'Anselmo', 'Brenhilda', 'Corvalis',
                    'Dunmore', 'Esquivel', 'Fennimore', 'Galdrun')


def paraphrase_entry(subject_row, predicate_row, value):
    stored = f'{subject_row["stored"]} {predicate_row["stored"].format(V=value)}.'
    return stored[0].upper() + stored[1:]


def paraphrase_question(subject_row, predicate_row):
    return predicate_row['asked'].format(S=subject_row['asked'])


def crosslang_entry(subject_row, fact_row, lang, value):
    text = f'{subject_row[lang]} {fact_row[lang].format(V=value)}.'
    return text[0].upper() + text[1:]


def crosslang_question(subject_row, fact_row, lang):
    return fact_row[f'{lang}_question'].format(S=subject_row[lang])
