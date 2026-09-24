import gzip
import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest

from ..application.verify import verify
from ..capture.manifest import verify_run
from ..domain.errors import CaptureIntegrityError
from .fakes import rpc, sample_events, write_capture


def rewrite_manifest(folder, change):
    path = Path(folder) / 'manifest.json'
    manifest = json.loads(path.read_text())
    change(manifest)
    path.write_text(json.dumps(manifest))


class IntegrityTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.run_dir = write_capture(Path(self.tmp.name) / 'run', {'lesson': sample_events()})

    def tearDown(self):
        self.tmp.cleanup()

    def assertRejected(self, fragment):
        with self.assertRaises(CaptureIntegrityError) as caught:
            verify_run(self.run_dir)
        self.assertIn(fragment, str(caught.exception))

    def test_intact_capture_verifies(self):
        self.assertEqual(set(verify_run(self.run_dir).files),
                         {'lesson.jsonl', 'lesson.stdout', 'lesson.stderr'})

    def test_altered_byte_rejected(self):
        path = self.run_dir / 'lesson.jsonl.gz'
        raw = bytearray(gzip.decompress(path.read_bytes()))
        raw[raw.index(b'kmp_guide')] ^= 0x01
        path.write_bytes(gzip.compress(bytes(raw), mtime=0))
        self.assertRejected('hash mismatch')

    def test_altered_raw_file_rejected(self):
        (self.run_dir / 'lesson.stdout').write_bytes(b'{]\n')
        self.assertRejected('hash mismatch')

    def test_length_mismatch_rejected_before_hashing(self):
        (self.run_dir / 'lesson.stdout').write_bytes(b'{}\n\n')
        self.assertRejected('length mismatch')

    def test_gzip_expansion_above_declared_length_rejected(self):
        (self.run_dir / 'lesson.jsonl.gz').write_bytes(gzip.compress(b'\n' * 10_000_000))
        self.assertRejected('exceeds its declared length')

    def test_declared_size_above_limit_rejected(self):
        with self.assertRaises(CaptureIntegrityError):
            verify_run(self.run_dir, max_file_bytes=16)

    def test_path_outside_root_rejected(self):
        rewrite_manifest(self.run_dir, lambda m: m['uncompressed_files'].update(
            {'../escape': {'bytes': 0, 'sha256': hashlib.sha256(b'').hexdigest()}}))
        self.assertRejected('outside the run root')

    def test_symlink_rejected(self):
        os.symlink(self.run_dir / 'lesson.stderr', self.run_dir / 'alias.stderr')
        self.assertRejected('symlink')

    def test_duplicate_manifest_key_rejected(self):
        path = self.run_dir / 'manifest.json'
        text = path.read_text()
        entry = '"lesson.stderr": {"bytes": 0, "sha256": "%s"}' % hashlib.sha256(b'').hexdigest()
        path.write_text(text.replace('"uncompressed_files": {', '"uncompressed_files": {' + entry + ',', 1))
        self.assertRejected('duplicate JSON key')

    def test_raw_and_gzip_copy_is_a_duplicate_entry(self):
        (self.run_dir / 'lesson.jsonl').write_bytes(gzip.decompress((self.run_dir / 'lesson.jsonl.gz').read_bytes()))
        self.assertRejected('stored twice')

    def test_unlisted_file_rejected(self):
        (self.run_dir / 'extra.txt').write_text('x')
        self.assertRejected('not covered by the manifest')


class PairingTest(unittest.TestCase):
    def verify_events(self, events):
        with tempfile.TemporaryDirectory() as folder:
            run_dir = write_capture(Path(folder) / 'run', {'lesson': events})
            return verify(run_dir, 1 << 20)

    def test_unpaired_request_is_a_visible_failure(self):
        lost = rpc(9, 'tools/call', {'name': 'kmp_recall', 'arguments': {}}, {})
        del lost['response']
        _, traces, report = self.verify_events(sample_events() + [lost])
        self.assertFalse(report['ok'])
        self.assertEqual([f['code'] for f in report['failures']], ['UNPAIRED_REQUEST'])
        self.assertEqual(traces[0].observed[-1].exposure.kind.value, 'request')

    def test_response_without_request_is_a_failure(self):
        orphan = rpc(9, 'tools/list', {}, {})
        del orphan['request']
        _, _, report = self.verify_events(sample_events() + [orphan])
        self.assertEqual([f['code'] for f in report['failures']], ['RESPONSE_WITHOUT_REQUEST'])

    def test_notification_is_identified_not_failed(self):
        _, traces, report = self.verify_events(sample_events())
        self.assertTrue(report['ok'])
        self.assertEqual(report['journeys'][0]['notifications'], 1)
        kinds = [item.exposure.kind.value for item in traces[0].observed]
        self.assertEqual(kinds.count('notification'), 1)

    def test_stages_follow_continuation_provenance(self):
        _, traces, _ = self.verify_events(sample_events())
        stages = {item.exposure.jsonrpc_id: item.exposure.stage.value
                  for item in traces[0].observed if item.exposure.kind.value == 'request'}
        self.assertEqual(stages, {1: 'startup_catalogue', 2: 'startup_catalogue', 3: 'guide',
                                  4: 'guide', 5: 'memory', 6: 'memory'})


if __name__ == '__main__':
    unittest.main()
