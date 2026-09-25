"""kmp.bench.variant.v1: TOML parsing, env allowlist, Jev consistency and the two digests."""
from pathlib import Path
import tempfile
import textwrap
import unittest

from ..domain import cachekey
from ..domain.errors import VariantInvalid
from ..domain.variant import ENV_ALLOWLIST, parse_variant_toml

BASE = '''
name = "candidate"
claim = "quality"
targets = [{ metric = "useful_rate", direction = "up", effect = 0.08 }]
guards = ["false_answer_rate", { metric = "core_precision", margin = 0.0 }]
n = 100
store = "shared"
jev = "off"

[binary]
path = "target/release/kmp-mcp"
'''


class VariantTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        (self.root / 'files').mkdir()
        (self.root / 'files/rerank.json').write_text('{"pool_size": 40}\n')
        (self.root / 'files/bridge.kmpb').write_bytes(b'\x00bridge')
        (self.root / 'files/cassette.json').write_text('{"entries": []}\n')

    def parse(self, text):
        return parse_variant_toml(textwrap.dedent(text), 'v.toml', self.root)

    def refuse(self, text, fragment):
        with self.assertRaises(VariantInvalid) as caught:
            self.parse(text)
        self.assertIn(fragment, str(caught.exception))

    def test_base_variant(self):
        variant = self.parse(BASE)
        self.assertEqual(variant.name, 'candidate')
        self.assertEqual(variant.targets[0].effect, 0.08)
        self.assertEqual([g.metric for g in variant.guards], ['false_answer_rate', 'core_precision'])
        self.assertEqual(variant.binary.path, 'target/release/kmp-mcp')
        self.assertEqual(variant.execution_config(), {'jev': 'off', 'store_files': {}, 'env': {}})
        self.assertEqual(variant.as_dict()['config_digest'], variant.config_digest())

    def test_store_files_by_path_table_and_inline_json(self):
        variant = self.parse(BASE.replace('jev = "off"', 'jev = "replay"') + '''
            [store_files]
            "rerank.json" = "files/rerank.json"
            "lexical-bridge.kmpb" = { path = "files/bridge.kmpb" }
            "ask-gate.json" = { json = { mode = "strict", pool = [1, 2] } }
            ''')
        files = {f.name: f for f in variant.store_files}
        self.assertEqual(sorted(files), ['ask-gate.json', 'lexical-bridge.kmpb', 'rerank.json'])
        self.assertEqual(files['rerank.json'].sha256,
                         cachekey.sha256_hex((self.root / 'files/rerank.json').read_bytes()))
        self.assertEqual(files['ask-gate.json'].inline, '{"mode":"strict","pool":[1,2]}')
        self.assertEqual(files['ask-gate.json'].content(), b'{"mode":"strict","pool":[1,2]}')
        self.assertEqual(files['lexical-bridge.kmpb'].content(), b'\x00bridge')

    def test_config_digest_tracks_behaviour_not_preregistration(self):
        base = self.parse(BASE)
        renamed = self.parse(BASE.replace('"candidate"', '"other"').replace('0.08', '0.05').replace('100', '200'))
        self.assertEqual(base.config_digest(), renamed.config_digest())
        self.assertNotEqual(base.preregistration_digest(), renamed.preregistration_digest())
        logged = self.parse(BASE + '\n[env]\nRUST_LOG = "kmp_mcp::judgement=debug"\n')
        self.assertNotEqual(base.config_digest(), logged.config_digest())
        other_binary = self.parse(BASE.replace('path = "target/release/kmp-mcp"', 'git_ref = "main"'))
        self.assertEqual(base.config_digest(), other_binary.config_digest(), 'the binary is its own key part')

    def test_path_env_digests_file_content_not_location(self):
        text = BASE.replace('jev = "off"', 'jev = "replay"') + '\n[env]\nKMP_TYPESAFE_CASSETTE = "files/cassette.json"\n'
        first = self.parse(text)
        moved = self.root / 'elsewhere'
        moved.mkdir()
        (moved / 'cassette.json').write_bytes((self.root / 'files/cassette.json').read_bytes())
        second = self.parse(text.replace('files/cassette.json', str(moved / 'cassette.json')))
        self.assertEqual(first.config_digest(), second.config_digest())
        self.assertEqual(first.env_map()['KMP_TYPESAFE_CASSETTE'], str(self.root / 'files/cassette.json'))
        (moved / 'cassette.json').write_text('{"entries": [1]}')
        third = self.parse(text.replace('files/cassette.json', str(moved / 'cassette.json')))
        self.assertNotEqual(first.config_digest(), third.config_digest())

    def test_env_allowlist_and_secrets(self):
        for name in ('TYPESAFE_API_KEY', 'OPENAI_API_KEY', 'MY_TOKEN', 'KMP_SECRET'):
            self.refuse(BASE + f'\n[env]\n{name} = "x"\n', 'secrets never live in a variant')
        for name in ('KMP_MCP_DATA_DIR', 'KMP_MCP_BACKEND', 'HOME', 'KMP_VIEWER_ADDR'):
            self.refuse(BASE + f'\n[env]\n{name} = "x"\n', 'not in the allowlist')
        self.refuse(BASE + '\n[env]\nRUST_LOG = ""\n', 'non-empty')
        self.assertIn('KMP_MCP_ENGINE', ENV_ALLOWLIST)
        self.parse(BASE + '\n[env]\nKMP_MCP_ENGINE = "sqlite"\n')

    def test_jev_consistency(self):
        replay = BASE.replace('jev = "off"', 'jev = "replay"')
        record = BASE.replace('jev = "off"', 'jev = "record"')
        cassette = '\n[env]\nKMP_TYPESAFE_CASSETTE = "files/cassette.json"\n'
        self.refuse(BASE + cassette, "jev='off'")
        self.refuse(BASE + '\n[store_files]\n"typesafe.json" = { json = { model = "jev-1.13.0" } }\n', "jev='off'")
        self.refuse(replay + cassette + 'KMP_TYPESAFE_CASSETTE_MODE = "record"\n', 'cannot record')
        self.refuse(record + cassette + 'KMP_TYPESAFE_CASSETTE_MODE = "replay"\n', 'cannot replay')
        self.refuse(replay + '\n[env]\nKMP_TYPESAFE_CASSETTE = "files/none.json"\n', 'replay needs it')
        self.refuse(replay + cassette + 'KMP_TYPESAFE_CASSETTE_MODE = "live"\n', 'replay or record')
        self.refuse(BASE + '\n[env]\nKMP_LEXICAL_BRIDGE = "files/none.kmpb"\n', 'does not exist')
        recorded = self.parse(record + '\n[env]\nKMP_TYPESAFE_CASSETTE = "files/new.json"\n'
                              'KMP_TYPESAFE_CASSETTE_MODE = "record"\n')
        self.assertEqual(recorded.execution_config()['env']['KMP_TYPESAFE_CASSETTE'], {'absent': True})

    def test_structural_refusals(self):
        cases = [
            (BASE + 'extra = 1\n', 'unknown field'),
            (BASE.replace('"candidate"', '"Bad Name"'), 'name'),
            (BASE.replace('"quality"', '"vibes"'), 'claim'),
            (BASE.replace('targets = [{ metric = "useful_rate", direction = "up", effect = 0.08 }]', 'targets = []'),
             'at least one target'),
            (BASE.replace('targets = [{ metric = "useful_rate", direction = "up", effect = 0.08 }]',
                          'targets = ["useful_rate"]'), 'is a table'),
            (BASE.replace('effect = 0.08', 'effect = 0'), 'above zero'),
            (BASE.replace(', effect = 0.08', ''), 'effect'),
            (BASE.replace('direction = "up"', 'direction = "sideways"'), 'direction'),
            (BASE.replace('n = 100', 'n = 0'), 'n'),
            (BASE.replace('n = 100', 'n = true'), 'integer'),
            (BASE.replace('"shared"', '"borrowed"'), 'store'),
            (BASE.replace('path = "target/release/kmp-mcp"', ''), 'exactly one of path or git_ref'),
            (BASE + 'git_ref = "main"\n', 'exactly one of path or git_ref'),
            (BASE.replace('path = "target/release/kmp-mcp"', 'git_ref = "../escape"'), 'git_ref'),
            (BASE + '\n[store_files]\n"../x.json" = "files/rerank.json"\n', 'bare'),
            (BASE + '\n[store_files]\n"x.txt" = "files/rerank.json"\n', 'bare'),
            (BASE + '\n[store_files]\n"x.json" = "files/missing.json"\n', 'cannot read'),
            (BASE + '\n[store_files]\n"x.json" = "files/bridge.kmpb"\n', 'not UTF-8 JSON'),
            (BASE + '\n[store_files]\n"x.json" = { json = { at = 1979-05-27T07:32:00Z } }\n', 'inline json'),
            (BASE + '\n[store_files]\n"x.json" = { other = 1 }\n', 'path string'),
            (BASE.replace('guards = ["false_answer_rate",', 'guards = ["Bad-Metric",'), 'metric name'),
            ('name = ', 'not TOML'),
        ]
        for text, fragment in cases:
            with self.subTest(fragment=fragment):
                self.refuse(text, fragment)

    def test_parity_targets_may_be_bare_names(self):
        variant = self.parse(BASE.replace('"quality"', '"parity"').replace(
            'targets = [{ metric = "useful_rate", direction = "up", effect = 0.08 }]', 'targets = ["parity_rate"]'))
        self.assertEqual(variant.targets[0].metric, 'parity_rate')
        self.assertIsNone(variant.targets[0].effect)


if __name__ == '__main__':
    unittest.main()
