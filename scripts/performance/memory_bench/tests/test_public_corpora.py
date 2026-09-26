"""BT17: public corpora — licence lock, fetch, adapters, stores and evidence recall.

Every dataset here is a tiny synthetic stand-in written in the test: no public
benchmark text is committed. Two cases read the real files when they are present
under tmp/memory-bench/datasets (skipped otherwise), and the end-to-end case loads
a synthetic FactConsolidation corpus through the release binary (skipped when
`target/release/kmp-mcp` is not built).
"""
import contextlib
import dataclasses
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest

from ..application import public_build, public_cli, public_run
from ..application.build import BuildFailed
from ..corpora import fetch, locomo, longmemeval, memoryagentbench as fc, musique_2wiki, parquet
from ..corpora.public_corpus import ingest_calls, evidence_question
from ..corpora.public_errors import CorpusError, FetchFailed, LicenceRefused
from ..domain import cachekey
from ..domain.errors import PrivacyViolation
from ..domain.gold import Chain
from ..domain.jsonl import REPO_ROOT
from ..domain.question import write_questions
from ..runtime import binaries
from ..runtime.layout import public_layout

DATASETS = REPO_ROOT / 'tmp' / 'memory-bench' / 'datasets'
FORBIDDEN_IN_STORE = ('supersedes', 'superseded_by', 'valid_until', 'valid_from', 'has_answer',
                      'is_supporting', 'answer')


# --- synthetic datasets ---------------------------------------------------------------------

FACTS = ('Here is a list of facts:\n'
         '0. Alice is a citizen of France.\n'
         '1. The capital of France is Paris.\n'
         '2. Alice is married to Bob.\n'
         '3. Alice is a citizen of Spain.\n'
         '4. The capital of Spain is Madrid.\n'
         '5. Bob was born in the city of Lyon.\n')


def fc_rows(sh_answer='Spain', mh_answer='Madrid'):
    return [{'context': FACTS, 'source': 'factconsolidation_sh_6k', 'qa_pair_ids': ['sh0', 'sh1'],
             'questions': ['Which country is Alice a citizen of?', 'Who is Zed married to?'],
             'answers': [[sh_answer], ['Nobody']]},
            {'context': FACTS, 'source': 'factconsolidation_mh_6k', 'qa_pair_ids': ['mh0', 'mh1'],
             'questions': ['What is the capital of the country Alice is a citizen of?',
                           'What is the capital of the country where Alice lives?'],
             'answers': [[mh_answer], ['Paris']]}]


def lme_item(qid='q1', abstain=False):
    return {'question_id': qid + ('_abs' if abstain else ''), 'question_type': 'single-session-user',
            'question': 'What degree did I finish?', 'answer': 'Physics',
            'question_date': '2023/06/01 (Thu) 10:00',
            'answer_session_ids': ['s-b'],
            'haystack_session_ids': ['s-a', 's-b'],
            'haystack_dates': ['2023/05/20 (Sat) 09:00', '2023/05/02 (Tue) 08:30'],
            'haystack_sessions': [
                [{'role': 'user', 'content': 'I like tea.'}, {'role': 'assistant', 'content': 'Noted.'}],
                [{'role': 'user', 'content': 'I finished my Physics degree.', 'has_answer': True},
                 {'role': 'assistant', 'content': ''},
                 {'role': 'assistant', 'content': 'Congratulations!'}]]}


def musique_rows():
    def paragraph(idx, title, text, supporting=False):
        return {'idx': idx, 'title': title, 'paragraph_text': text, 'is_supporting': supporting}
    return [{'id': '2hop__1_2', 'question': 'Who founded the label of Green?', 'answer': 'Ann',
             'paragraphs': [paragraph(0, 'Noise', 'Unrelated text.'),
                            paragraph(1, 'Label', 'Label X was founded by Ann.', True),
                            paragraph(2, 'Green', 'Green was released on Label X.', True)],
             'question_decomposition': [{'paragraph_support_idx': 2}, {'paragraph_support_idx': 1}]},
            {'id': '2hop__3_4', 'question': 'Where was Ann born?', 'answer': 'Oslo',
             'paragraphs': [paragraph(0, 'Label', 'Label X was founded by Ann.', True),
                            paragraph(1, 'Ann', 'Ann was born in Oslo.', True)],
             'question_decomposition': [{'paragraph_support_idx': 0}, {'paragraph_support_idx': 1}]}]


def twowiki_rows():
    return [{'_id': 'aa11', 'type': 'compositional', 'question': "Who is the mother of Film Z's director?",
             'answer': 'Eve', 'context': [['Film Z', ['Film Z is directed by Dan.']],
                                          ['Dan', ['Dan is the son of Eve.', ' He lives in Rome.']],
                                          ['Other', ['Nothing here.']]],
             'supporting_facts': [['Film Z', 0], ['Dan', 0]]},
            {'_id': 'bb22', 'type': 'comparison', 'question': 'Who is older, Dan or Eve?', 'answer': 'Eve',
             'context': [['Eve', ['Eve was born in 1950.']], ['Dan', ['Dan is the son of Eve.', ' He lives in Rome.']]],
             'supporting_facts': [['Dan', 0], ['Eve', 0]]}]


def locomo_sample():
    return {'sample_id': 'conv-1', 'conversation': {
        'speaker_a': 'Ana', 'speaker_b': 'Ben',
        'session_1_date_time': '1:56 pm on 8 May, 2023',
        'session_1': [{'speaker': 'Ana', 'dia_id': 'D1:1', 'text': 'I ran a race.'},
                      {'speaker': 'Ben', 'dia_id': 'D1:2', 'text': 'Nice!', 'blip_caption': 'a medal'}],
        'session_2_date_time': '10:00 am on 9 May, 2023',
        'session_2': [{'speaker': 'Ana', 'dia_id': 'D2:1', 'text': 'Next week I swim.'}]},
        'qa': [{'question': 'What did Ana run?', 'answer': 'a race', 'evidence': ['D1:1'], 'category': 4},
               {'question': 'What does Ana do next week?', 'answer': 'swim', 'evidence': ['D2:1; D9:9'],
                'category': 2},
               {'question': 'What did Ben lose?', 'evidence': ['D1:2'], 'category': 5,
                'adversarial_answer': 'a race'}]}


def store_payload_strings(corpus):
    """Every key and value the store receives, flattened."""
    found = []

    def walk(value):
        if isinstance(value, dict):
            for key, item in value.items():
                found.append(str(key))
                walk(item)
        elif isinstance(value, list):
            for item in value:
                walk(item)
        else:
            found.append(str(value))
    for _, arguments in ingest_calls(corpus):
        walk(arguments)
    return found


# --- Parquet --------------------------------------------------------------------------------

class ParquetPieces(unittest.TestCase):
    def test_snappy_literal_copy_and_overlapping_copy(self):
        stream = bytes([14, 12]) + b'abcd' + bytes([1, 4]) + bytes([22, 2, 0])
        self.assertEqual(parquet.snappy_decompress(stream), b'abcdabcdcdcdcd')

    def test_snappy_refuses_a_wrong_length(self):
        with self.assertRaises(parquet.ParquetUnsupported):
            parquet.snappy_decompress(bytes([9, 12]) + b'abcd')

    def test_thrift_compact_struct(self):
        data = bytes([0x15, 10, 0x18, 2]) + b'ab' + bytes([0x29, 0x25, 2, 4, 0])
        fields, end = parquet.thrift_struct(data)
        self.assertEqual(fields, {1: 5, 2: b'ab', 4: [1, 2]})
        self.assertEqual(end, len(data))

    def test_rle_bitpacked_hybrid(self):
        data = bytes([6, 1, 3, 0b00000101])
        self.assertEqual(parquet.rle_hybrid(data, 0, len(data), 1, 11), [1, 1, 1, 1, 0, 1, 0, 0, 0, 0, 0])

    def test_assemble_nested_lists(self):
        self.assertEqual(parquet.assemble([(0, 3), (1, 3), (0, 3)], ['a', 'b', 'c'], 1, 3, (2,)),
                         [['a', 'b'], ['c']])
        rows = parquet.assemble([(0, 5), (2, 5), (1, 5), (0, 5)], 'xyzw', 2, 5, (2, 4))
        self.assertEqual(rows, [[['x', 'y'], ['z']], [['w']]])
        self.assertEqual(parquet.assemble([(0, 1), (0, 0)], ['v'], 0, 1, ()), ['v', None])

    def test_refuses_a_non_parquet_file(self):
        with self.assertRaises(parquet.ParquetUnsupported):
            parquet.ParquetFile(b'not parquet at all')

    @unittest.skipUnless((DATASETS / 'memoryagentbench-cr' / fc.PARQUET).is_file(), 'dataset not fetched')
    def test_reads_the_locked_conflict_resolution_file(self):
        rows = fc.load_rows(DATASETS / 'memoryagentbench-cr' / fc.PARQUET)
        self.assertEqual(len(rows), 8)
        self.assertEqual(sorted(row['source'] for row in rows)[0], 'factconsolidation_mh_262k')
        self.assertTrue(all(len(row['questions']) == len(row['answers']) == 100 for row in rows))


# --- licence lock and fetch -----------------------------------------------------------------

class Lock(unittest.TestCase):
    def setUp(self):
        self.lock = fetch.load_lock()

    def test_every_entry_records_url_sha_licence_and_redistribution(self):
        raw = json.loads(fetch.LOCK_PATH.read_text(encoding='utf-8'))
        for name, item in raw['datasets'].items():
            self.assertIn('redistributable', item, name)
            self.assertTrue(item['licence']['spdx'] and item['licence']['verified_from'], name)
            for locked in item['files']:
                self.assertTrue(locked['url'].startswith('https://'), name)
                cachekey.require_hex64(locked['sha256'], name)
        self.assertIn('osunlp/HippoRAG_v2', raw['refused_sources'])

    def test_nc_and_sa_material_is_private_and_not_redistributable(self):
        self.assertEqual((self.lock['locomo'].storage, self.lock['locomo'].redistributable), ('private', False))
        self.assertEqual((self.lock['hotpotqa'].storage, self.lock['hotpotqa'].redistributable),
                         ('private', False))
        for entry in self.lock.values():
            if entry.storage == 'public':
                self.assertFalse(any(m in entry.spdx for m in ('-NC', '-SA')), entry.name)

    def test_a_public_nc_entry_is_refused(self):
        entry = dataclasses.replace(self.lock['locomo'], storage='public')
        with self.assertRaises(LicenceRefused):
            fetch.check_lock({'locomo': entry})

    def test_private_datasets_never_resolve_inside_the_repository(self):
        with self.assertRaises(LicenceRefused):
            fetch.dataset_dir(self.lock['locomo'], env={})
        with self.assertRaises(PrivacyViolation):
            fetch.dataset_dir(self.lock['locomo'], private_root=REPO_ROOT / 'tmp' / 'x')
        with tempfile.TemporaryDirectory() as outside:
            folder = fetch.dataset_dir(self.lock['locomo'], private_root=outside)
            self.assertNotIn(REPO_ROOT.resolve(), folder.resolve().parents)
        public = fetch.dataset_dir(self.lock['musique'])
        self.assertEqual(public, REPO_ROOT / 'tmp' / 'memory-bench' / 'datasets' / 'musique')

    def test_digest_follows_the_locked_bytes(self):
        entry = self.lock['musique']
        other = dataclasses.replace(entry, files=(dataclasses.replace(entry.files[0], sha256='0' * 64),))
        self.assertNotEqual(entry.digest(), other.digest())


class Fetch(unittest.TestCase):
    def entry(self, payload):
        locked = fetch.LockedFile('data.bin', 'https://example.invalid/data.bin',
                                  hashlib.sha256(payload).hexdigest(), len(payload))
        return fetch.LockedDataset('toy', 'MIT', True, 'public', (locked,), ())

    @staticmethod
    def opener(payload, calls):
        def open_url(request, timeout):
            calls.append(request.full_url)
            return contextlib.closing(io.BytesIO(payload))
        return open_url

    def test_downloads_verifies_and_then_keeps(self):
        payload, calls = b'x' * 3000, []
        with tempfile.TemporaryDirectory() as folder:
            rows = fetch.fetch(self.entry(payload), folder, self.opener(payload, calls))
            self.assertEqual(rows[0]['status'], 'downloaded')
            again = fetch.fetch(self.entry(payload), folder, self.opener(payload, calls))
            self.assertEqual(again[0]['status'], 'present')
            self.assertEqual(len(calls), 1)

    def test_wrong_bytes_leave_nothing_behind(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(FetchFailed):
                fetch.fetch(self.entry(b'expected'), folder, self.opener(b'tampered', []))
            self.assertEqual(list(Path(folder).iterdir()), [])

    def test_verify_only_refuses_a_missing_file(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(FetchFailed):
                fetch.fetch(self.entry(b'abc'), folder, download=False)


# --- FactConsolidation ------------------------------------------------------------------------

class FactConsolidation(unittest.TestCase):
    def test_one_fact_per_entry_with_increasing_sequence_and_time(self):
        corpus = fc.build(fc_rows(), '6k')
        entries = corpus.abouts[0].entries
        self.assertEqual([e.ref for e in entries], [f'factconsolidation:6k:fact:{i}' for i in range(6)])
        self.assertEqual(entries[3].text, 'Alice is a citizen of Spain.')
        self.assertEqual([e.sequence for e in entries], list(range(1, 7)))
        times = [e.occurred_at for e in entries]
        self.assertEqual(times, sorted(set(times)))

    def test_no_supersedes_or_anything_else_derived_from_the_gold(self):
        corpus = fc.build(fc_rows(), '6k')
        for _, arguments in ingest_calls(corpus):
            self.assertEqual(arguments['memory']['relations'], [])
        strings = store_payload_strings(corpus)
        for word in FORBIDDEN_IN_STORE:
            self.assertNotIn(word, strings)
        # The store does not move when the gold does: other answers, same bytes.
        other = fc.build(fc_rows(sh_answer='France', mh_answer='Paris'), '6k')
        self.assertEqual(ingest_calls(corpus), ingest_calls(other))
        self.assertEqual(corpus.content_digest(), other.content_digest())

    @unittest.skipUnless((DATASETS / 'memoryagentbench-cr' / fc.PARQUET).is_file(), 'dataset not fetched')
    def test_the_real_fact_lists_carry_no_relation_either(self):
        rows = fc.load_rows(DATASETS / 'memoryagentbench-cr' / fc.PARQUET)
        corpus = fc.build(rows, '6k')
        self.assertTrue(all(args['memory']['relations'] == [] for _, args in ingest_calls(corpus)))
        self.assertTrue(all(set(e.metadata) == {'serial'} for e in corpus.abouts[0].entries))
        self.assertGreaterEqual(len(corpus.questions), 180)

    def test_the_declared_variant_adds_only_the_writer_rule_supersedes(self):
        plain = fc.build(fc_rows(), '6k')
        declared = fc.build(fc_rows(), '6k', declared=True)
        self.assertEqual(declared.name, 'factconsolidation-6k-declared')
        self.assertEqual(declared.abouts[0].entries, plain.abouts[0].entries)
        self.assertNotEqual(declared.content_digest(), plain.content_digest())
        [(_, arguments)] = ingest_calls(declared)
        relations = arguments['memory']['relations']
        # Alice's citizenship is stated twice: the later statement replaces the earlier.
        self.assertEqual([(r['from'], r['rel'], r['to']) for r in relations],
                         [('factconsolidation:6k:fact:3', 'supersedes', 'factconsolidation:6k:fact:0')])
        self.assertEqual(relations[0]['class'], 'evidential')
        self.assertEqual(declared.notes['declared_relations'], 1)
        # The rule reads the fact list only: other answers, the same relations.
        other = fc.build(fc_rows(sh_answer='France', mh_answer='Paris'), '6k', declared=True)
        self.assertEqual(ingest_calls(declared), ingest_calls(other))
        self.assertEqual(declared.questions_digest(), plain.questions_digest())

    def test_a_relation_travels_with_the_batch_that_holds_its_later_end(self):
        declared = fc.build(fc_rows(), '6k', declared=True)
        calls = ingest_calls(declared, batch_size=2)
        carried = [[(r['from'], r['to']) for r in args['memory']['relations']] for _, args in calls]
        self.assertEqual(carried, [[], [('factconsolidation:6k:fact:3', 'factconsolidation:6k:fact:0')], []])

    def test_sh_gold_is_the_latest_fact_and_names_the_stale_one(self):
        corpus = fc.build(fc_rows(), '6k')
        sh = [q for q in corpus.questions if q.corpus == 'factconsolidation-sh']
        self.assertEqual(len(sh), 1)  # 'Zed' is not a known subject: dropped, never guessed
        self.assertEqual(corpus.notes['dropped'], {'sh': 1, 'mh': 1})
        question = sh[0]
        self.assertEqual(question.type, 'current_after_supersession')
        self.assertEqual(question.gold.current_ref, 'factconsolidation:6k:fact:3')
        self.assertEqual(question.gold.stale_refs, ('factconsolidation:6k:fact:0',))

    def test_mh_gold_is_the_unique_chain_over_effective_facts(self):
        corpus = fc.build(fc_rows(), '6k')
        [mh] = [q for q in corpus.questions if q.corpus == 'factconsolidation-mh']
        self.assertEqual(mh.type, 'multihop_why_k')
        self.assertEqual(mh.gold.chain.refs, ('factconsolidation:6k:fact:3', 'factconsolidation:6k:fact:4'))

    def test_templates_parse_office_holders_last(self):
        self.assertEqual(fc.triple('The capital of Spain is Madrid.'), ('Spain', 'capital', 'Madrid'))
        self.assertEqual(fc.triple('The Mayor of London is Sadiq Khan.'),
                         ('Mayor of London', 'office_holder', 'Sadiq Khan'))
        self.assertIsNone(fc.triple('Nothing to see'))

    def test_refuses_an_unknown_size_and_a_missing_row(self):
        with self.assertRaises(CorpusError):
            fc.build(fc_rows(), '12k')
        with self.assertRaises(CorpusError):
            fc.build(fc_rows(), '32k')


# --- LongMemEval ------------------------------------------------------------------------------

class LongMemEval(unittest.TestCase):
    def test_turns_load_in_date_order_without_gold_or_empty_turns(self):
        about, answers = longmemeval.item_about(lme_item())
        refs = [e.ref for e in about.entries]
        self.assertEqual(refs, ['longmemeval:q1:s1:t1', 'longmemeval:q1:s1:t3',
                                'longmemeval:q1:s0:t1', 'longmemeval:q1:s0:t2'])
        self.assertEqual(answers, ['longmemeval:q1:s1:t1'])
        times = [e.occurred_at for e in about.entries]
        self.assertEqual(times, sorted(set(times)))
        corpus = longmemeval.build([lme_item()], per_type=5, abstention=0)
        for word in FORBIDDEN_IN_STORE:
            self.assertNotIn(word, store_payload_strings(corpus))

    def test_abstention_items_are_unknown(self):
        corpus = longmemeval.build([lme_item(), lme_item('q2', abstain=True)], per_type=5, abstention=5)
        by_id = {q.id: q for q in corpus.questions}
        self.assertEqual(by_id['lme-q2_abs'].gold.answerable, 'UNKNOWN')
        self.assertEqual(by_id['lme-q1'].gold.answer_refs(), {'longmemeval:q1:s1:t1'})
        self.assertEqual(by_id['lme-q1'].source['answer_sessions'], 'longmemeval:q1:s1')
        self.assertEqual(by_id['lme-q1'].answer_policy, 'evidence_or_unknown')

    def test_selection_per_type_then_abstention(self):
        items = [lme_item(f'q{i}') for i in range(4)] + [lme_item(f'a{i}', True) for i in range(3)]
        chosen = longmemeval.select(items, per_type=2, abstention=1)
        self.assertEqual([i['question_id'] for i in chosen], ['q0', 'q1', 'a0_abs'])

    def test_classification_matches_the_rust_runner(self):
        expected = ['t1', 't2']
        self.assertEqual(longmemeval.classify_evidence(expected, {'t1', 't2', 'x'}), longmemeval.FULL)
        self.assertEqual(longmemeval.classify_evidence(expected, {'t2'}), longmemeval.PARTIAL)
        self.assertEqual(longmemeval.classify_evidence(expected, {'x'}), longmemeval.MISSING)
        self.assertEqual(longmemeval.classify_evidence([], {'x'}), longmemeval.NOT_APPLICABLE)
        self.assertEqual(longmemeval.classify_evidence(expected, {'t1'}, True), longmemeval.NOT_APPLICABLE)

    def test_session_of_and_dates(self):
        self.assertEqual(longmemeval.session_of('longmemeval:q1:s12:t3'), 'longmemeval:q1:s12')
        self.assertIsNone(longmemeval.session_of('longmemeval:q1:other'))
        self.assertEqual(longmemeval.parse_date('2023/05/20 (Sat) 02:21').isoformat(), '2023-05-20T02:21:00+00:00')
        with self.assertRaises(CorpusError):
            longmemeval.parse_date('yesterday')


# --- MuSiQue and 2Wiki ------------------------------------------------------------------------

class MultiHop(unittest.TestCase):
    def test_musique_dedups_passages_and_keeps_the_decomposition_order(self):
        corpus = musique_2wiki.build('musique', musique_rows(), ['2hop__1_2', '2hop__3_4'], limit=2)
        texts = [e.text for e in corpus.abouts[0].entries]
        self.assertEqual(len(texts), 4)  # 'Label X was founded by Ann.' is shared
        first, second = corpus.questions
        self.assertEqual(first.gold.chain.refs, ('musique:hipporag2:p00002', 'musique:hipporag2:p00001'))
        self.assertEqual(second.gold.chain.refs, ('musique:hipporag2:p00001', 'musique:hipporag2:p00003'))
        self.assertEqual(first.type, 'multihop_why_k')
        for word in FORBIDDEN_IN_STORE:
            self.assertNotIn(word, store_payload_strings(corpus))

    def test_2wiki_chain_follows_supporting_titles(self):
        corpus = musique_2wiki.build('2wiki', twowiki_rows(), ['aa11', 'bb22'], limit=2)
        compositional, comparison = corpus.questions
        self.assertEqual(compositional.gold.chain, Chain(('2wiki:hipporag2:p00000', '2wiki:hipporag2:p00001'), True))
        self.assertFalse(comparison.gold.chain.ordered)
        self.assertIn('qtype:comparison', comparison.tags)

    def test_the_subset_is_a_prefix_and_the_full_setup_checks_its_size(self):
        corpus = musique_2wiki.build('musique', musique_rows(), ['2hop__1_2', '2hop__3_4'], limit=1)
        self.assertEqual([q.id for q in corpus.questions], ['musique-2hop__1_2'])
        with self.assertRaises(CorpusError):
            musique_2wiki.build('musique', musique_rows(), ['2hop__1_2', '2hop__3_4'], limit=None)
        with self.assertRaises(CorpusError):
            musique_2wiki.build('musique', musique_rows(), ['missing'], limit=1)

    @unittest.skipUnless((DATASETS / 'musique' / 'musique_v1.0.zip').is_file()
                         and (DATASETS / 'hipporag-question-ids' / 'musique.json').is_file(),
                         'datasets not fetched')
    def test_the_real_full_setup_rebuilds_hipporag2_passage_count(self):
        spec = musique_2wiki.DATASETS['musique']
        rows = musique_2wiki.read_archive_rows(DATASETS / 'musique' / spec['archive'], spec['member'])
        ids = musique_2wiki.read_selection(DATASETS / 'hipporag-question-ids' / spec['ids'], spec['id_field'])
        corpus = musique_2wiki.build('musique', rows, ids, limit=None)
        self.assertEqual((corpus.entry_count(), len(corpus.questions)), (11656, 1000))


# --- LoCoMo -----------------------------------------------------------------------------------

class LoCoMo(unittest.TestCase):
    def test_questions_are_private_and_adversarial_items_optional(self):
        corpus = locomo.build([locomo_sample()])
        self.assertTrue(corpus.private)
        self.assertEqual([q.id for q in corpus.questions], ['locomo-conv-1-q000', 'locomo-conv-1-q001'])
        self.assertTrue(all(q.private for q in corpus.questions))
        self.assertEqual(corpus.notes['unknown_evidence_ids'], 1)
        entries = corpus.abouts[0].entries
        self.assertEqual(entries[1].text, 'Ben: Nice! [shares an image: a medal]')
        with_adversarial = locomo.build([locomo_sample()], adversarial=True)
        self.assertEqual(with_adversarial.questions[-1].gold.answerable, 'UNKNOWN')

    def test_nothing_of_it_is_written_inside_the_repository(self):
        corpus = locomo.build([locomo_sample()])
        with self.assertRaises(PrivacyViolation):
            write_questions(REPO_ROOT / 'tmp' / 'memory-bench' / 'locomo.jsonl', corpus.questions)
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaises(BuildFailed):
                public_build.build_public(corpus, fetch.load_lock()['locomo'], None, public_layout(root))


# --- keys, recall reading, CLI ----------------------------------------------------------------

class KeysAndRecall(unittest.TestCase):
    def test_dataset_source_keys_a_store(self):
        source = cachekey.dataset_source(corpus='x', lock_sha256='a' * 64, selection_digest='b' * 64,
                                         adapter_version='v1')
        key = cachekey.store_key(source=source, n=3, bundle_format='stdio-ingest', reader_sha256='c' * 64,
                                 build='ingest')
        cachekey.require_hex64(key, 'key')
        with self.assertRaises(cachekey.CacheKeyInvalid):
            cachekey.store_key(source={'kind': 'other'}, n=3, bundle_format='x', reader_sha256='c' * 64,
                               build='ingest')

    def test_recall_values_read_evidence_core_and_stale(self):
        question = evidence_question(identifier='fc-q', corpus='factconsolidation-sh', about='fc:a',
                                     question='Which?', answers=['fc:a:2'], kind='current_after_supersession',
                                     current_ref='fc:a:2', stale_refs=['fc:a:1'])
        page = {'answer': 'x', 'because': [{'ref': 'entry:fc:a:1'}],
                'proof': {'evidence': [{'id': 'entry:fc:a:1'}, {'id': 'entry:fc:a:2'}]}}
        values = public_run.question_values(question, [page])
        self.assertEqual((values['recall_at_1'], values['recall_at_2']), (0.0, 1.0))
        self.assertTrue(values['stale_served'] and values['stale_before_current'])
        self.assertFalse(values['successor_rescued'])

    def test_recall_values_for_longmemeval_sessions_and_hits(self):
        question = evidence_question(identifier='lme-q', corpus='longmemeval-s', about='longmemeval:q',
                                     question='?', answers=['longmemeval:q:s1:t1', 'longmemeval:q:s1:t2'],
                                     policy='evidence_or_unknown',
                                     source={'answer_sessions': 'longmemeval:q:s1'})
        page = {'answer': 'UNKNOWN', 'because': [],
                'proof': {'evidence': [{'id': 'entry:longmemeval:q:s0:t4'},
                                       {'id': 'entry:longmemeval:q:s1:t2'}]}}
        values = public_run.question_values(question, [page])
        self.assertEqual(values['evidence_hit'], longmemeval.PARTIAL)
        self.assertEqual((values['session_recall_at_1'], values['session_recall_at_2']), (0.0, 1.0))
        self.assertTrue(values['unknown'])
        summary = public_run.aggregate([values, values])
        self.assertEqual(summary['evidence_hit'], {'partial': 2})
        self.assertEqual(summary['unknown']['hits'], 2)

    def test_chain_recovery(self):
        question = evidence_question(identifier='m-q', corpus='musique', about='m:a', question='?',
                                     answers=['m:a:1', 'm:a:2'], kind='multihop_why_k',
                                     chain=Chain(('m:a:1', 'm:a:2'), True))
        page = {'answer': 'x', 'because': [], 'proof': {'evidence': [{'id': 'm:a:1'}, {'id': 'm:a:9'},
                                                                     {'id': 'm:a:2'}]}}
        values = public_run.question_values(question, [page])
        self.assertFalse(values['full_chain_at_2'])
        self.assertTrue(values['full_chain_at_5'])

    def test_cli_parses_every_corpus(self):
        parser = public_cli.build_parser()
        args = parser.parse_args(['run', '--corpus', 'factconsolidation', '--size', '6k,32k'])
        self.assertEqual(args.size, ('6k', '32k'))
        self.assertEqual(parser.parse_args(['run', '--corpus', 'musique', '--setup', 'full']).setup, 'full')
        with self.assertRaises(SystemExit), contextlib.redirect_stderr(io.StringIO()):
            parser.parse_args(['build', '--corpus', 'factconsolidation', '--size', '9k'])


RELEASE = binaries.DEFAULT_BINARY


@unittest.skipUnless(RELEASE.is_file(), 'target/release/kmp-mcp not built')
class EndToEnd(unittest.TestCase):
    def test_a_synthetic_fc_corpus_loads_caches_and_answers(self):
        corpus = fc.build(fc_rows(), '6k')
        entry = fetch.load_lock()['memoryagentbench-cr']
        writer = binaries.KmpBinary.locate(RELEASE)
        with tempfile.TemporaryDirectory() as root:
            layout = public_layout(root)
            quiet = io.StringIO()
            built = public_build.build_public(corpus, entry, writer, layout, cpus=None, timeout_s=120, log=quiet)
            self.assertEqual(built['built'], ['ingest', 'import'])
            again = public_build.build_public(corpus, entry, writer, layout, cpus=None, timeout_s=120, log=quiet)
            self.assertEqual(again['built'], [])
            timings = public_build.timings(built)
            self.assertEqual(timings['entries'], 6)
            self.assertIsNotNone(timings['ingest_ms'])
            _, run = public_run.run_public(corpus, built, layout, binary=RELEASE, warmup=0, timeout_s=120)
            summary = public_run.recall_summary(corpus, run)
            self.assertEqual(summary['statuses'], {'completed': 2})
            sh = summary['corpora']['factconsolidation-sh']['all']
            self.assertEqual(sh['questions'], 1)
            self.assertEqual(sh['recall_at_10']['value'], 1.0)


if __name__ == '__main__':
    unittest.main()
