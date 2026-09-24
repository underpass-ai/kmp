"""Store isolation refusals and the driver's follow-the-server rule, without a binary."""
import json
from pathlib import Path
import tempfile
import unittest

from ..native.driver import MAX_CALLS, run_journey
from ..native.isolation import IsolationError, confirm_effective_store, create_store
from ..scenarios import SCENARIOS, scenarios_digest
from ..scenarios.model import JourneySpec


class IsolationTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.home = Path(self.folder.name) / 'real-home'

    def test_store_inside_the_real_data_dir_is_refused(self):
        real = self.home / '.local/share/kmp'
        with self.assertRaises(IsolationError):
            create_store(real / 'work', 'x', {'HOME': str(self.home), 'PATH': '/usr/bin'})

    def test_explicit_real_store_env_is_refused_when_it_contains_the_work_dir(self):
        work = Path(self.folder.name) / 'work'
        with self.assertRaises(IsolationError):
            create_store(work, 'x', {'HOME': str(self.home), 'KMP_MCP_DATA_DIR': str(work)})

    def test_child_env_is_allowlisted_and_contained(self):
        parent = {'HOME': str(self.home), 'PATH': '/usr/bin', 'KMP_KERNEL_GRPC_ENDPOINT': 'x',
                  'OPENAI_API_KEY': 'secret'}
        store = create_store(Path(self.folder.name) / 'work', 'x', parent)
        self.assertNotIn('OPENAI_API_KEY', store.env)
        self.assertNotIn('KMP_KERNEL_GRPC_ENDPOINT', store.env)
        for key in ('HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME', 'KMP_MCP_DATA_DIR'):
            self.assertTrue(Path(store.env[key]).is_relative_to(store.root))

    def test_binary_must_confirm_the_disposable_store(self):
        store = create_store(Path(self.folder.name) / 'work', 'x', {'HOME': str(self.home)})
        line = {'fields': {'message': 'embedded backend data dir resolved', 'rule': 'env',
                           'data_dir': str(store.data_dir)}}
        self.assertEqual(confirm_effective_store(store, json.dumps(line))['rule'], 'env')
        line['fields']['data_dir'] = str(self.home)
        with self.assertRaises(IsolationError):
            confirm_effective_store(store, json.dumps(line))
        with self.assertRaises(IsolationError):
            confirm_effective_store(store, 'banner only')


class FakeSession:
    def __init__(self, pages):
        self.pages, self.sent = list(pages), []

    def call(self, tool, arguments):
        self.sent.append((tool, arguments))
        page = self.pages.pop(0) if self.pages else self.last
        self.last = page
        return {'result': {'structuredContent': page}}


class DriverTest(unittest.TestCase):
    def test_driver_executes_the_servers_next_action_verbatim(self):
        action = {'tool': 'kmp_wake', 'arguments': {'about': 'a', 'page': {'cursor': 'kmp1:9:x'}}}
        session = FakeSession([{'projection': {'next_action': action}}, {'projection': {}}])
        outcome = run_journey(session, JourneySpec('kmp_wake', {'about': 'a'}), 4096)
        self.assertEqual(session.sent[0][1], {'about': 'a', 'budget': {'max_bytes': 4096}})
        self.assertEqual(session.sent[1][1], action['arguments'])
        self.assertEqual((outcome.status, outcome.calls), ('completed', 2))

    def test_endless_continuations_stop_at_the_cap(self):
        loop = {'projection': {'next_action': {'tool': 'kmp_wake', 'arguments': {'about': 'a'}}}}
        outcome = run_journey(FakeSession([loop]), JourneySpec('kmp_wake', {'about': 'a'}), None)
        self.assertEqual((outcome.status, outcome.calls), ('call_cap_reached', MAX_CALLS))


class ScenarioTest(unittest.TestCase):
    def test_digest_is_stable_and_sensitive(self):
        self.assertEqual(scenarios_digest(), scenarios_digest(SCENARIOS))
        self.assertNotEqual(scenarios_digest(SCENARIOS[:1]), scenarios_digest(SCENARIOS[1:2]))
        self.assertEqual(len({s.case_id for s in SCENARIOS}), len(SCENARIOS))


if __name__ == '__main__':
    unittest.main()
