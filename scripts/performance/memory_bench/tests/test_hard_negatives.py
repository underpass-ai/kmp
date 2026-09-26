"""Hard negatives and identifier guards (BT08) on a synthetic store, with a fake probe.

Nothing here reads the real store: the fixtures are invented texts. The fake
probe reads identifiers and keys the way the kernel does in shape (tokens
split on non-alphanumerics, identifiers carry a digit, a '#' or a joiner) but
never stems, which is all the invariants below depend on. One test runs the
real kmp_search_probe when it has been built, and is skipped otherwise.
"""
import copy
import json
from pathlib import Path
import re
import sqlite3
import tempfile
import unittest

from ..corpora import identifier_guards, negative_audit
from ..corpora.anchor_forms import Anchor, anchors_in
from ..corpora.errors import AuditInvalid, NegativeRulesInvalid, StoreUnreadable
from ..corpora.hard_negatives import build_context, generate_negatives, shortfalls
from ..corpora.negative_builders import FORMS, TYPES
from ..corpora.negative_render import join_words
from ..corpora.negative_rules import (HASH_PATH, RULES_PATH, NegativeRules, load_rules, parse_rules,
                                      register, registered_sha256, require_registered, rules_sha256)
from ..corpora.search_probe import BinaryProbe, ProbeScope, ProbeTerms
from ..corpora.store_snapshot import AboutDocs, EntryDoc, StoreSnapshot, carries_search_summary, read_store
from ..domain.question import parse_questions, questions_digest
from ..domain.jsonl import REPO_ROOT, dump_lines

STOP = {'the', 'of', 'and', 'for', 'was', 'is', 'a', 'an', 'to', 'in', 'on', 'what', 'which', 'about',
        'with', 'are', 'were', 'do', 'we', 'it', 'its', 'by', 'or'}
JOINERS = '-_/.:+'
NOUNS = ['migration', 'adapter', 'partition', 'compiler', 'validation', 'retention', 'encoder',
         'allocation', 'provider', 'collector', 'deployment', 'ownership']
UNIQUE = ['rotor', 'monitor', 'sensor', 'reactor', 'vector', 'tractor', 'motor', 'cursor', 'router',
          'buffer', 'filter', 'header']
PROVENANCE = {'rules_version': 'negatives.v1', 'rules_sha256': '0' * 64, 'seed': 7,
              'store_digest': '1' * 64, 'probe_sha256': '2' * 64, 'verified': True}


def _is_identifier(token):
    inner = token.strip(JOINERS)
    return len(token) >= 2 and (any(c.isdigit() for c in token) or token.startswith('#')
                                or any(j in inner for j in JOINERS))


class FakeProbe:
    """Kernel-shaped tokenizer without stemming; `calls` counts probe processes."""

    sha256 = '2' * 64

    def __init__(self):
        self.calls = 0

    def probe(self, about_texts, texts):
        self.calls += 1
        out = []
        for text in texts:
            tokens = [t.strip('.,;:!?¿¡()"\'') for t in text.split()]
            identifiers = frozenset(t.lower() for t in tokens if t and _is_identifier(t))
            keys = frozenset(w for w in re.split(r'[^0-9a-záéíóúñü]+', text.lower())
                             if w and w not in STOP and (len(w) > 1 or w.isdigit()))
            out.append(ProbeTerms(text, 'english', identifiers, keys, keys))
        return tuple(out)


def _entry(about, n, text, active=True):
    return EntryDoc(f'{about}:entry:decision:e{n:03d}', about, 'decision', active, text, (text,))


def alpha_docs():
    about, entries = 'project:alpha', []
    for i in range(12):
        text = (f'Issue #{101 + i} changed the {NOUNS[i]} and the {NOUNS[(i + 1) % 12]} '
                f'of the scheduler; the {UNIQUE[i]} stayed.')
        entries.append(_entry(about, i, text))
    entries.append(_entry(about, 12, 'Corte 3 split C3.1+C3.2 and the range C3.1-C3.4 into the migration plan.'))
    entries.append(_entry(about, 13, 'Release v1.2.3 shipped the encoder and the Kubernetes budget.'))
    entries.append(_entry(about, 14, 'C3.2 keeps the provider; issue 105 stays open for the collector.'))
    entries.append(_entry(about, 15, 'An old note about the deployment.', active=False))
    neighbors = ((entries[0].ref, (entries[12].ref,)), (entries[12].ref, (entries[0].ref,)))
    return AboutDocs(about, tuple(entries), tuple(e.text for e in entries), neighbors)


def beta_docs():
    about, entries = 'project:beta', []
    for i in range(6):
        text = f'Ticket #{201 + i} moved the {NOUNS[i]} next to the {NOUNS[(i + 5) % 12]} for the {UNIQUE[i]}.'
        entries.append(_entry(about, i, text))
    return AboutDocs(about, tuple(entries), tuple(e.text for e in entries), ())


def rules_for_tests(**overrides):
    raw = copy.deepcopy(load_rules().raw)
    raw['store'] = {'abouts': ['project:alpha', 'project:beta'], 'min_entries': 1}
    raw.update({'per_form': 3, 'min_per_form': 2})
    raw['attributes']['min_df'] = 1
    raw['attributes']['max_df_fraction'] = 0.5
    raw['identifier_guards']['per_family'] = 2
    for key, value in overrides.items():
        raw[key] = value
    return NegativeRules(raw, '0' * 64)


def generate(rules=None):
    rules = rules or rules_for_tests()
    snapshot = StoreSnapshot((alpha_docs(), beta_docs()))
    probe = FakeProbe()
    context, texts = build_context(snapshot, probe, rules)
    negatives, rows, counts = generate_negatives(context, texts, probe, PROVENANCE)
    guards, guard_rows = identifier_guards.generate_guards(context, texts, probe, PROVENANCE)
    return context, negatives, rows, counts, guards, guard_rows


def raw_of(context, question):
    return {d.ref: d.raw for d in context.indexes[question.about].docs}


class AnchorFormsTest(unittest.TestCase):
    def test_parse_display_and_keysets(self):
        self.assertEqual(Anchor.parse('#188').display, '#188')
        self.assertEqual(Anchor.parse('c6.4').display, 'C6.4')
        self.assertEqual(Anchor.parse('v0.18.6').parts, (0, 18, 6))
        self.assertIsNone(Anchor.parse('pr183'))
        self.assertIn(frozenset({'issue188'}), Anchor.parse('#188').keysets())
        self.assertEqual(Anchor.parse('c6.4').keysets(), (frozenset({'c6', '4'}),))

    def test_strict_forms(self):
        hash_188 = Anchor.parse('#188').strict_pattern()
        for text in ('see #188.', 'issue 188', 'Issue #188', 'PR188 landed', 'issues 188'):
            self.assertTrue(hash_188.search(text), text)
        for text in ('#1880', '188 ms', 'v1.188'):
            self.assertFalse(hash_188.search(text), text)
        dotted = Anchor.parse('c6.4').strict_pattern()
        self.assertTrue(dotted.search('C6.1-C6.4 and more') and dotted.search('c6-4') and dotted.search('C6.4.'))
        self.assertFalse(dotted.search('C6.40') or dotted.search('C6.4.1'))
        version = Anchor.parse('v0.18.6').strict_pattern()
        self.assertTrue(version.search('KMP 0.18.6 ships') and version.search('v0.18.6.'))
        self.assertFalse(version.search('v0.18.60') or version.search('10.18.6'))

    def test_perturbations_are_distinct_and_deterministic(self):
        twins = Anchor.parse('#188').perturbations()
        self.assertEqual(twins[0].display, '#288')
        self.assertEqual(len(set(twins)), len(twins))
        self.assertEqual(Anchor.parse('c6.4').perturbations()[0].display, 'C6.24')
        self.assertNotIn(Anchor.parse('v0.5.0'), Anchor.parse('v0.5.0').perturbations())

    def test_compound_members(self):
        found = anchors_in({'c6.8+c6.9', 'c6.1-c6.4', '#12'})
        self.assertEqual({a.display for a in found}, {'C6.8', 'C6.9', 'C6.1', 'C6.4', '#12'})


class HardNegativesTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.context, cls.negatives, cls.rows, cls.counts, cls.guards, cls.guard_rows = generate()

    def by_type(self, kind):
        return [q for q in self.negatives if q.type == kind]

    def test_every_type_and_form_reaches_the_minimum(self):
        self.assertEqual(shortfalls(self.counts, 2), {})
        for kind in TYPES:
            for form in FORMS:
                self.assertGreaterEqual(self.counts[f'{kind}/{form}']['kept'], 2, f'{kind}/{form}')

    def test_records_are_valid_private_and_round_trip(self):
        text = dump_lines(q.as_dict() for q in self.negatives + self.guards)
        parsed = parse_questions(text)
        self.assertEqual(questions_digest(parsed), questions_digest(self.negatives + self.guards))
        self.assertTrue(all(q.private for q in parsed))
        self.assertTrue(all(q.source['verified'] for q in parsed))

    def test_short_forms_have_two_to_five_words_and_long_are_longer(self):
        for question in self.negatives:
            words = len(question.question.split())
            if 'form:short' in question.tags:
                self.assertTrue(2 <= words <= 5, question.question)
            else:
                self.assertGreater(words, 5, question.question)

    def test_anchor_absent_has_no_occurrence_anywhere(self):
        for question in self.by_type('anchor_absent'):
            anchor = Anchor.parse(question.anchors[0].lower())
            for index in self.context.indexes.values():
                self.assertFalse(index.strict(anchor), question.question)
            self.assertFalse(self.context.indexes[question.about].mentions(anchor))
            self.assertEqual(question.gold.unknown_reason, 'anchor_not_found')
            self.assertEqual(question.gold.absent_terms, (question.anchors[0],))

    def test_near_miss_attribute_is_present_but_never_with_the_anchor(self):
        for question in self.by_type('near_miss_attribute'):
            index = self.context.indexes[question.about]
            anchor = Anchor.parse(question.anchors[0].lower())
            zone = index.around(index.mentions(anchor) | index.strict(anchor), 1)
            for facet in question.gold.facets:
                key = index.word_key[facet.words]
                self.assertGreater(index.df(key), 0)
                self.assertFalse(index.docs_with_key(key) & zone, question.question)
                self.assertEqual(set(facet.related), set(index.strict(anchor)))

    def test_neighbor_attribute_lives_only_with_the_neighbour(self):
        for question in self.by_type('anchor_neighbor_existing'):
            index = self.context.indexes[question.about]
            anchor = Anchor.parse(question.anchors[0].lower())
            neighbor = Anchor.parse(question.source['neighbor_anchor'].lower())
            self.assertEqual(anchor.family_prefix(), neighbor.family_prefix())
            for facet in question.gold.facets:
                docs = index.docs_with_key(index.word_key[facet.words])
                self.assertTrue(docs <= index.mentions(neighbor) | index.strict(neighbor))
                self.assertFalse(docs & index.strict(anchor))
            self.assertTrue(question.gold.wrong_subject)

    def test_negated_anchor_answers_avoid_the_excluded_anchor(self):
        for question in self.by_type('negated_anchor'):
            index = self.context.indexes[question.about]
            anchor = Anchor.parse(question.anchors[0].lower())
            self.assertEqual(question.gold.answerable, 'KNOWN')
            zone = index.mentions(anchor) | index.strict(anchor)
            self.assertTrue(set(question.gold.excluded_refs) <= zone)
            for facet in question.gold.facets:
                self.assertTrue(facet.answers)
                self.assertFalse(set(facet.answers) & zone)
                self.assertTrue(all(index.by_ref[r].active for r in facet.answers))

    def test_twin_is_singular_and_attribute_group_is_absent_around_the_anchor(self):
        for question in self.by_type('singular_anchored_twin'):
            self.assertEqual(question.gold.shape, 'singular')
            index = self.context.indexes[question.about]
            anchor = Anchor.parse(question.anchors[0].lower())
            raw = raw_of(self.context, question)
            zone = index.around(index.mentions(anchor) | index.strict(anchor), 1)
            word = question.gold.facets[0].words
            self.assertFalse(any(word in raw[ref].lower() for ref in zone), question.question)
            self.assertTrue({'attr_df:zero', 'attr_df:present'} & set(question.tags))

    def test_cross_about_anchor_lives_only_elsewhere(self):
        questions = self.by_type('cross_about_anchor')
        for question in questions:
            anchor = Anchor.parse(question.anchors[0].lower())
            owner = self.context.indexes[question.source['owner_about']]
            self.assertNotEqual(owner.about, question.about)
            self.assertTrue(owner.strict(anchor))
            queried = self.context.indexes[question.about]
            self.assertFalse(queried.strict(anchor) or queried.mentions(anchor))
            self.assertEqual(question.gold.unknown_reason, 'anchor_in_other_about')
        self.assertEqual({q.about for q in questions}, {'project:alpha', 'project:beta'})

    def test_every_kept_question_passed_every_probe_check(self):
        kept = [row for row in self.rows if row['kept']]
        self.assertEqual(len(kept), len(self.negatives))
        self.assertTrue(all(all(row['checks'].values()) for row in kept))

    def test_same_seed_same_bytes_other_seed_other_questions(self):
        _, again, *_ = generate()
        self.assertEqual(dump_lines(q.as_dict() for q in again), dump_lines(q.as_dict() for q in self.negatives))
        _, other, *_ = generate(rules_for_tests(seed=11))
        self.assertNotEqual(questions_digest(other), questions_digest(self.negatives))

    def test_join_words(self):
        self.assertEqual(join_words(['a'], 'and'), 'a')
        self.assertEqual(join_words(['a', 'b', 'c'], 'y'), 'a, b y c')


class IdentifierGuardsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        rules = rules_for_tests()
        rules.raw['identifier_guards']['per_family'] = 50
        cls.context, _, _, _, cls.guards, cls.rows = generate(rules)

    def families(self, polarity):
        return {dict(t.split(':', 1) for t in q.tags)['family'] for q in self.guards
                if f'polarity:{polarity}' in q.tags}

    def test_families_have_positives_and_negatives(self):
        self.assertEqual(self.families('positive'), set(identifier_guards.FAMILIES))
        self.assertTrue({'hash', 'issue_hash', 'version', 'lower_hyphen', 'corte_c'} <= self.families('negative'))

    def test_positive_gold_writes_the_identifier_and_negative_is_unknown(self):
        for question in self.guards:
            if 'polarity:positive' in question.tags:
                self.assertEqual(question.gold.answerable, 'KNOWN')
                self.assertTrue(question.gold.facets[0].answers)
            else:
                self.assertEqual(question.gold.unknown_reason, 'anchor_not_found')
        surfaces = {q.anchors[0] for q in self.guards}
        self.assertTrue({'C3.1+C3.2', 'C3.1-C3.4', 'issue 105', 'corte 3', 'C3', 'c3-1'} <= surfaces)

    def test_issue_form_finds_the_hash_entry(self):
        question = next(q for q in self.guards if q.anchors[0] == 'issue 105')
        answers = question.gold.facets[0].answers
        self.assertIn('project:alpha:entry:decision:e004', answers)
        self.assertIn('project:alpha:entry:decision:e014', answers)

    def test_summary_counts(self):
        summary = identifier_guards.summarize(self.guards)
        self.assertEqual(sum(summary.values()), len(self.guards))


class RulesTest(unittest.TestCase):
    def test_repository_rules_match_their_registered_hash(self):
        self.assertEqual(rules_sha256(RULES_PATH.read_bytes()), registered_sha256(HASH_PATH))
        self.assertEqual(load_rules().sha256, registered_sha256(HASH_PATH))

    def test_an_edited_rule_is_refused_until_registered(self):
        data = RULES_PATH.read_bytes().replace(b'seed = 7', b'seed = 8')
        with self.assertRaises(NegativeRulesInvalid):
            parse_rules(data, registered_sha256(HASH_PATH))
        self.assertEqual(parse_rules(data)['seed'], 8)

    def test_schema_checks(self):
        with self.assertRaises(NegativeRulesInvalid):
            parse_rules(b'schema = "other"')
        data = RULES_PATH.read_bytes().replace(b'audit_fraction = 0.2', b'audit_fraction = 1.5')
        with self.assertRaises(NegativeRulesInvalid):
            parse_rules(data)

    def test_register_and_require(self):
        with tempfile.TemporaryDirectory() as root:
            rules_copy, hash_copy = Path(root) / 'n.toml', Path(root) / 'n.sha256'
            rules_copy.write_bytes(RULES_PATH.read_bytes())
            private = Path(root) / 'private'
            with self.assertRaises(NegativeRulesInvalid):
                require_registered('a' * 64, private)
            digest = register(private, rules_copy, hash_copy)
            self.assertEqual(registered_sha256(hash_copy), digest)
            self.assertTrue(require_registered(digest, private))
            with self.assertRaises(NegativeRulesInvalid):
                require_registered('a' * 64, private)

    def test_registry_may_not_live_in_the_repository(self):
        with tempfile.TemporaryDirectory() as root:
            rules_copy, hash_copy = Path(root) / 'n.toml', Path(root) / 'n.sha256'
            rules_copy.write_bytes(RULES_PATH.read_bytes())
            with self.assertRaises(Exception):
                register(REPO_ROOT / 'tmp' / 'memory-bench' / 'private', rules_copy, hash_copy)


class AuditTest(unittest.TestCase):
    def test_sample_is_twenty_percent_per_group_and_deterministic(self):
        _, negatives, *_ = generate()
        chosen = negative_audit.sample(negatives, 0.2, 7)
        self.assertEqual(chosen, negative_audit.sample(negatives, 0.2, 7))
        for group, ids in chosen.items():
            size = sum(negative_audit.group_of(q) == group for q in negatives)
            self.assertEqual(len(ids), -(-size // 5))

    def test_score_counts_errors_and_refuses_a_partial_audit(self):
        chosen = {'near_miss_attribute/short': ['a', 'b'], 'identifier_guard/hash/positive': ['c']}
        verdicts = [{'id': 'a', 'verdict': 'correct'}, {'id': 'b', 'verdict': 'error'},
                    {'id': 'c', 'verdict': 'correct'}]
        summary = negative_audit.score(chosen, verdicts)
        self.assertEqual(summary['hard_negatives']['errors'], 1)
        self.assertEqual(summary['identifier_guards']['error_rate'], 0.0)
        with self.assertRaises(AuditInvalid):
            negative_audit.score(chosen, verdicts[:2])
        with self.assertRaises(AuditInvalid):
            negative_audit.score(chosen, verdicts + [{'id': 'a', 'verdict': 'correct'}])

    def test_sheet_names_the_gold_entries(self):
        context, negatives, *_ = generate()
        snapshot = StoreSnapshot((alpha_docs(), beta_docs()))
        chosen = negative_audit.sample(negatives, 0.2, 7)
        rows = negative_audit.sheet_rows(negatives, chosen, snapshot)
        self.assertEqual(len(rows), sum(len(ids) for ids in chosen.values()))
        near = [r for r in rows if r['group'].startswith('near_miss_attribute')]
        self.assertTrue(all(r['entries'] for r in near))


def _sqlite_store(root):
    path = Path(root) / 'store' / 'kernel.sqlite3'
    path.parent.mkdir(parents=True)
    connection = sqlite3.connect(path)
    connection.execute('CREATE TABLE nodes (k TEXT PRIMARY KEY, v BLOB)')
    connection.execute('CREATE TABLE details (k TEXT PRIMARY KEY, v BLOB)')
    connection.execute('CREATE TABLE relations_by_source (k1 TEXT, k2 TEXT, k3 TEXT, v BLOB)')

    def node(ref, kind, about, text, status='ACTIVE', entity='memory_entry', metadata=None):
        payload = {'text': text, 'metadata': metadata or {}}
        value = {'node_id': ref, 'node_kind': kind, 'summary': text, 'status': status,
                 'properties': {'memory_about': about, 'memory_entity_kind': entity, 'entry_kind': kind,
                                'memory_payload_json': json.dumps(payload)}}
        connection.execute('INSERT INTO nodes VALUES (?, ?)', (ref, json.dumps(value)))
    node('p:a:entry:decision:one', 'decision', 'p:a', 'Issue #12 fixed the adapter.',
         metadata={'summary_en': 'Issue #12 fixed the adapter.'})
    node('p:a:entry:decision:two', 'decision', 'p:a', 'Old router note.', status='SUPERSEDED')
    node('evidence:p:a:one', 'memory_evidence', 'p:a', 'Proof: the encoder log.', entity='memory_evidence')
    node('p:b:entry:decision:one', 'decision', 'p:b', 'Nota sin resumen en ingles.')
    detail = json.dumps({'detail': 'Issue #12 fixed the adapter.'})
    connection.execute('INSERT INTO details VALUES (?, ?)', ('p:a:entry:decision:one', detail))
    connection.execute('INSERT INTO relations_by_source VALUES (?, ?, ?, ?)',
                       ('evidence:p:a:one', 'p:a:entry:decision:one', 'supports',
                        json.dumps({'rationale': 'Evidence supports this memory entry.'})))
    connection.execute('INSERT INTO relations_by_source VALUES (?, ?, ?, ?)',
                       ('p:a:entry:decision:two', 'p:a:entry:decision:one', 'follows',
                        json.dumps({'rationale': 'The router came first.'})))
    connection.commit()
    connection.close()


class StoreSnapshotTest(unittest.TestCase):
    def test_reads_entries_evidence_and_relations(self):
        with tempfile.TemporaryDirectory() as root:
            _sqlite_store(root)
            snapshot = read_store(root)
        about = snapshot.about('p:a')
        one, two = about.entries
        self.assertEqual(one.ref, 'p:a:entry:decision:one')
        self.assertIn('Proof: the encoder log.', one.texts)
        self.assertIn('The router came first.', one.texts)
        self.assertNotIn('The router came first.', one.own())
        self.assertIn('Proof: the encoder log.', one.own())
        self.assertEqual(one.texts.count('Issue #12 fixed the adapter.'), 1)
        self.assertFalse(two.active)
        self.assertEqual(about.neighbor_map()[one.ref], (two.ref,))
        self.assertEqual(len(snapshot.digest()), 64)
        with self.assertRaises(StoreUnreadable):
            snapshot.about('p:missing')

    def test_carries_search_summary_per_about(self):
        """The ranker's fallback language (`search_morphology`) needs to know whether an about
        carries an English search summary; the snapshot hands it to the probe."""
        with tempfile.TemporaryDirectory() as root:
            _sqlite_store(root)
            snapshot = read_store(root)
        self.assertTrue(snapshot.about('p:a').carries_search_summary)
        self.assertFalse(snapshot.about('p:b').carries_search_summary)
        self.assertEqual(ProbeScope.of(snapshot.about('p:a')).request(['x'])['carries_search_summary'], True)
        self.assertEqual(ProbeScope.of(snapshot.about('p:b')).request(['x'])['carries_search_summary'], False)
        self.assertTrue(carries_search_summary({'payload_metadata': json.dumps({'summary_en': 'An English line.'})}, {}))
        self.assertFalse(carries_search_summary({'payload_metadata': '{}'}, {'metadata': {'summary_en': 'x'}}))
        self.assertFalse(carries_search_summary({}, {'metadata': {'summary_en': '  '}}))

    def test_binary_probe_sends_the_summary_flag(self):
        with tempfile.TemporaryDirectory() as root:
            seen = Path(root) / 'request.jsonl'
            fake = Path(root) / 'kmp_search_probe'
            fake.write_text(FAKE_PROBE.format(seen=str(seen)))
            fake.chmod(0o755)
            probe = BinaryProbe(fake)
            about = AboutDocs('p:a', (), ('mixed text',), (), True)
            terms = probe.probe(ProbeScope.of(about), ['valves'])
            request = json.loads(seen.read_text())
            probe.probe(['plain about texts'], ['valves'])
            plain = json.loads(seen.read_text())
        self.assertEqual(terms[0].language, 'english')
        self.assertEqual((request['carries_search_summary'], request['about_texts']), (True, ['mixed text']))
        self.assertFalse(plain['carries_search_summary'])

    def test_refuses_the_live_store_and_missing_files(self):
        with self.assertRaises(StoreUnreadable):
            read_store(Path.home() / '.local/share/kmp')
        with tempfile.TemporaryDirectory() as root, self.assertRaises(StoreUnreadable):
            read_store(root)


PROBE = REPO_ROOT / 'target' / 'release' / 'kmp_search_probe'
FAKE_PROBE = """#!/usr/bin/env python3
import json, sys
line = sys.stdin.readline()
open({seen!r}, 'w').write(line)
request = json.loads(line)
language = 'english' if request.get('carries_search_summary') else None
for index, text in enumerate(request['texts']):
    print(json.dumps({{'schema': 'kmp.bench.search_probe.v1', 'text': text, 'language': language,
                      'identifiers': [], 'search_keys': [], 'informative_terms': []}}))
"""


@unittest.skipUnless(PROBE.is_file(), 'kmp_search_probe not built')
class RealProbeTest(unittest.TestCase):
    def test_kernel_reads_anchors_as_identifiers(self):
        probe = BinaryProbe(PROBE)
        terms = probe.probe(['The deployment of C6.4 moved after #188.'],
                            ['boundary for #188', 'What changed in C6.24 and v0.18.6?'])
        self.assertIn('#188', terms[0].identifiers)
        self.assertTrue({'c6.24', 'v0.18.6'} <= terms[1].identifiers)
        self.assertEqual(len(probe.sha256), 64)

    def test_summary_flag_reaches_the_kernel_fallback(self):
        mixed = ('The deployment of the gateway was frozen and the audit was in the way.',
                 'El despliegue de la pasarela se congelo por la auditoria de la semana.')
        probe = BinaryProbe(PROBE)
        plain = probe.probe(ProbeScope(mixed, False), ['Deployments of the gateways stayed frozen.'])
        summarized = probe.probe(ProbeScope(mixed, True), ['Deployments of the gateways stayed frozen.'])
        self.assertIsNone(plain[0].language)
        self.assertEqual(summarized[0].language, 'english')


if __name__ == '__main__':
    unittest.main()
