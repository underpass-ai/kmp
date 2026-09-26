"""BT19: the Jev tail test through a local fault proxy, against a loopback upstream only.

Nothing here reaches the network or reads a key: real mode is exercised only as far
as its skip without TYPESAFE_API_KEY, and every measurement runs against
`LocalUpstream` on 127.0.0.1.
"""
import contextlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import unittest
from unittest import mock
import urllib.error

from .. import cli
from ..application import jev_tail
from ..domain.jsonl import REPO_ROOT
from ..runtime.jev_fault_proxy import (Fault, FaultPlan, FaultProxy, LocalUpstream, ProxyRefused,
                                       check_upstream)
from ..runtime.layout import public_layout

# The client (typesafe_judgement.rs) and its HTTP exchange with retries (typesafe_transport.rs).
RUST = tuple(REPO_ROOT / f'crates/kmp-mcp/src/serving/adapters/{name}.rs'
             for name in ('typesafe_judgement', 'typesafe_transport'))
STAND_IN = 'stand-in-key-7f3a9c'


def fast_settings(concurrency=(2, 4), requests=12, plan=jev_tail.SELF_CHECK_PLAN):
    sites = jev_tail.SITES
    return jev_tail.TailSettings(concurrency=concurrency, requests=requests, sites=sites,
                                 deadlines_ms={site: 1000 for site in sites}, plan=FaultPlan.parse(plan),
                                 policy=jev_tail.RetryPolicy(timeout_s=1.0))


class RetryPolicyMirrorTest(unittest.TestCase):
    def test_the_client_mirrors_the_binary_retry_policy(self):
        source = '\n'.join(path.read_text(encoding='utf-8') for path in RUST)
        self.assertEqual(int(re.search(r'const MAX_RETRIES: u32 = (\d+);', source).group(1)), jev_tail.MAX_RETRIES)
        self.assertEqual(int(re.search(r'const MAX_RETRY_WAIT_SECS: u64 = (\d+);', source).group(1)),
                         jev_tail.MAX_RETRY_WAIT_SECS)
        self.assertIn(f'.unwrap_or({jev_tail.DEFAULT_RETRY_WAIT_SECS})', source)
        self.assertIn('.no_proxy()', source)  # why the proxy cannot sit in front of the binary
        for message in ('TypeSafe timed out', 'TypeSafe unavailable', 'TypeSafe rate limit persisted after retries',
                        'TypeSafe rejected the API key', 'TypeSafe returned HTTP {code}'):
            self.assertIn(message, source)
            self.assertIn(message, jev_tail.MESSAGES.values())

    def test_retry_after_is_capped_and_defaults_to_one_second(self):
        policy = jev_tail.RetryPolicy()
        self.assertEqual([policy.wait_for(v) for v in (None, '2', '60', 'soon', '-3')], [1, 2, 5, 1, 0])

    def test_a_persistent_429_gives_up_after_two_retries(self):
        class Opener:
            calls = 0

            def open(self, request, timeout):
                Opener.calls += 1
                self.last = request
                raise urllib.error.HTTPError(request.full_url, 429, 'slow down', {'Retry-After': '9'}, None)
        slept = []
        outcome = jev_tail.send('http://127.0.0.1:9/', b'{}', STAND_IN, 'rerank', jev_tail.RetryPolicy(),
                                opener=Opener(), sleep=slept.append)
        self.assertEqual((outcome.status, outcome.attempts, Opener.calls, slept), ('rate_limited', 3, 3, [5, 5]))
        self.assertEqual(outcome.warning(), 'TypeSafe rate limit persisted after retries')


class FaultProxyTest(unittest.TestCase):
    def test_faults_parse_and_label(self):
        plan = FaultPlan.parse('pass,429:2,429,delay:150,timeout:1.5')
        self.assertEqual(plan.label(), 'pass,429:2,429,delay:150,timeout:1.5')
        self.assertEqual(plan.at(6).kind, 'rate_limit')
        for bad in ('boom', 'delay', 'delay:x', '429:-1', ''):
            with self.subTest(bad=bad), self.assertRaises(ProxyRefused):
                FaultPlan.parse(bad) if bad else FaultPlan(())
        self.assertEqual(Fault.parse('timeout:2').hold_s, 2.0)

    def test_only_loopback_or_the_provider_host_is_an_upstream(self):
        for good in ('http://127.0.0.1:8080/v1', 'http://localhost:1/', 'https://api.typesafe.ai/v1/systemone'):
            self.assertEqual(check_upstream(good), good)
        for bad in ('https://example.com/v1', 'http://api.typesafe.ai/v1', 'https://api.typesafe.ai:8443/v1',
                    'https://u:p@api.typesafe.ai/v1', 'https://api.typesafe.ai/v1?x=1', 'http://10.0.0.1/'):
            with self.subTest(bad=bad), self.assertRaises(ProxyRefused):
                check_upstream(bad)

    def test_the_proxy_injects_429_and_forwards_the_rest_without_keeping_secrets(self):
        with LocalUpstream() as upstream, FaultProxy(upstream.url, FaultPlan.parse('429:1,pass')) as proxy:
            slept = []
            outcome = jev_tail.send(proxy.url, jev_tail.body_for('rerank', 0), STAND_IN, 'rerank',
                                    jev_tail.RetryPolicy(timeout_s=2.0), sleep=slept.append)
        events = proxy.events()
        self.assertEqual((outcome.status, outcome.attempts, slept), ('ok', 2, [1]))
        self.assertEqual([(e.fault, e.status, e.site) for e in events], [('429:1', 429, 'rerank'),
                                                                           ('pass', 200, 'rerank')])
        self.assertEqual(upstream.requests, 1)
        self.assertEqual(upstream.authorizations, {f'Bearer {STAND_IN}'})  # passed through untouched
        self.assertEqual(upstream.site_headers, 0)  # the proxy's own header is stripped
        self.assertNotIn(STAND_IN, json.dumps([e.as_dict() for e in events]))

    def test_a_held_connection_times_the_client_out_and_is_not_retried(self):
        with LocalUpstream() as upstream, FaultProxy(upstream.url, FaultPlan.parse('timeout:3')) as proxy:
            outcome = jev_tail.send(proxy.url, b'{}', STAND_IN, 'paths', jev_tail.RetryPolicy(timeout_s=0.5))
        self.assertEqual((outcome.status, outcome.attempts), ('timed_out', 1))
        self.assertEqual([(e.fault, e.status) for e in proxy.events()], [('timeout:3', None)])  # recorded on exit
        self.assertEqual(outcome.warning(), 'TypeSafe timed out')
        self.assertEqual(upstream.requests, 0)


class TailTest(unittest.TestCase):
    def test_without_a_key_real_mode_is_skipped_with_its_reason(self):
        def refuse(*args, **kwargs):
            raise AssertionError('real mode without a key must not open a proxy')
        report = jev_tail.run_tail(fast_settings(), env={}, proxy_factory=refuse)
        self.assertEqual((report['status'], report['reason']), ('skipped', jev_tail.NO_KEY))
        self.assertEqual(report['levels'], [])

    def test_the_real_key_is_never_written(self):
        secret = 'sk-must-not-leak-0123456789'
        seen = []

        def fake_measure(upstream, key, settings, proxy_factory):
            seen.append((upstream, key))
            return []
        with mock.patch.object(jev_tail, 'measure', fake_measure):
            report = jev_tail.run_tail(fast_settings(), env={jev_tail.API_KEY_ENV: secret})
        self.assertEqual(seen, [(jev_tail.PROVIDER_URL, secret)])
        self.assertEqual(report['key'], {'source': 'env TYPESAFE_API_KEY', 'recorded': False})
        self.assertNotIn(secret, json.dumps(report))

    def test_concurrent_asks_report_added_latency_deadlines_and_degradation(self):
        with LocalUpstream(latency_ms=5) as upstream:
            report = jev_tail.run_tail(fast_settings(), upstream.url, key=STAND_IN)
        self.assertEqual((report['status'], report['mode']), ('ran', 'self_check'))
        self.assertNotIn(STAND_IN, json.dumps(report))
        self.assertEqual([level['concurrency'] for level in report['levels']], [2, 4])
        for level in report['levels']:
            row = level['all']
            self.assertEqual(row['n'], 12)
            self.assertIsNotNone(row['added_ms']['max'])
            self.assertIsNone(row['added_ms']['p99'])  # n < 100
            self.assertIn('n >= 100', row['added_ms']['p99_absent_reason'])
            self.assertGreaterEqual(row['retries'], 1)  # a 429 arrived and was retried
            self.assertGreaterEqual(row['degraded'], 1)  # the held connection timed out
            self.assertEqual(row['warned'], row['degraded'])
            self.assertIn('TypeSafe timed out', row['warnings'])
            self.assertLess(row['within_deadline'], row['n'])
            self.assertEqual(row['clean_errors'], 0)
            self.assertEqual(sorted(level['by_site']), sorted(jev_tail.SITES))
            self.assertEqual(sum(site['n'] for site in level['by_site'].values()), 12)
            self.assertGreaterEqual(level['proxy']['faults'].get('429:1', 0) + level['proxy']['faults'].get('429', 0), 1)
            self.assertEqual(sum(level['proxy']['faults'].values()), level['proxy']['faulted_requests'])
            self.assertEqual(level['proxy']['faulted_requests'], 12 + row['retries'])

    def test_p99_needs_a_hundred_requests(self):
        clean = [jev_tail.Outcome(i, 'rerank', 'ok', 200, 1, 0.0, 10.0) for i in range(120)]
        faulted = [jev_tail.Outcome(i, 'rerank', 'ok', 200, 1, 0.0, 10.0 + i) for i in range(120)]
        row = jev_tail.tail_row(clean, faulted, 100)
        self.assertEqual((row['added_ms']['max'], row['added_ms']['p99']), (119.0, 118.0))
        self.assertEqual(row['within_deadline'], 91)
        self.assertIsNone(jev_tail.tail_row(clean[:99], faulted[:99], 100)['added_ms']['p99'])

    def test_concurrency_outside_two_to_four_is_refused(self):
        with self.assertRaises(jev_tail.TailRefused):
            jev_tail.run_tail(fast_settings(concurrency=(1, 2)), 'http://127.0.0.1:9/', key=STAND_IN)

    def test_the_cli_skips_without_a_key_and_exits_zero(self):
        out = public_layout().root / 'jev-tail' / f'test-{os.getpid()}'
        self.addCleanup(shutil.rmtree, out, True)
        env = {k: v for k, v in os.environ.items() if k != jev_tail.API_KEY_ENV}
        stdout = io.StringIO()
        with mock.patch.dict(os.environ, env, clear=True), contextlib.redirect_stdout(stdout):
            code = cli.main(['jev-tail', '--out', str(out)])
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(stdout.getvalue())['status'], 'skipped')
        written = json.loads((Path(out) / 'report.json').read_text())
        self.assertEqual((written['schema'], written['reason']), (jev_tail.SCHEMA, jev_tail.NO_KEY))


if __name__ == '__main__':
    unittest.main()
