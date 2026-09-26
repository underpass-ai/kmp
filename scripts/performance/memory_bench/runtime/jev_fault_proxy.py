"""A local proxy that injects faults in front of the Jev (TypeSafe) endpoint (BENCH_SPEC 9, BT19).

The proxy listens on 127.0.0.1 and forwards every POST to one upstream: the
provider (`https://api.typesafe.ai/...`, real mode only) or a loopback server
(`LocalUpstream`, what the tests and `jev-tail --self-check` use). Before
forwarding, the request's position in arrival order picks a `Fault` from a
cycled `FaultPlan`:

- `pass`: forward as is;
- `rate_limit`: answer 429 with `Retry-After: <s>` and never forward;
- `delay`: hold the request `delay_ms`, then forward;
- `timeout`: hold the connection `hold_s` (longer than the client's timeout), then
  close it without an answer.

What the proxy keeps of a request is its arrival index, the site the client
named (`X-Bench-Site`, stripped before forwarding), the fault, how long it was
held, the upstream's latency and the status: never a header, a body or the
credential. The `Authorization` header is passed through to the upstream
untouched and forgotten. Any upstream other than a loopback address or the
provider host is refused, and forwarding ignores proxy environment variables
and redirects, as the binary's client does.
"""
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import ipaddress
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

from ..domain.errors import BenchError

PROVIDER_HOST = 'api.typesafe.ai'
SITE_HEADER = 'X-Bench-Site'
FAULTS = ('pass', 'rate_limit', 'delay', 'timeout')
UPSTREAM_TIMEOUT_S = 90.0
MAX_BODY = 256 * 1024  # the binary's REQUEST_BYTES


class ProxyRefused(BenchError):
    code = 'JEV_PROXY_REFUSED'


@dataclass(frozen=True)
class Fault:
    kind: str
    retry_after_s: int | None = None  # rate_limit: the Retry-After header; None sends none
    delay_ms: int = 0  # delay
    hold_s: float = 0.0  # timeout

    def __post_init__(self):
        if self.kind not in FAULTS:
            raise ProxyRefused(f'unknown fault {self.kind!r}; one of {", ".join(FAULTS)}')
        if self.delay_ms < 0 or self.hold_s < 0 or (self.retry_after_s is not None and self.retry_after_s < 0):
            raise ProxyRefused(f'{self.kind}: negative duration')

    @classmethod
    def parse(cls, text):
        """`pass`, `429` / `429:<s>` (Retry-After seconds), `delay:<ms>`, `timeout:<s>`."""
        name, _, value = text.strip().partition(':')
        try:
            if name == 'pass' and not value:
                return cls('pass')
            if name == '429':
                return cls('rate_limit', retry_after_s=int(value) if value else None)
            if name == 'delay':
                return cls('delay', delay_ms=int(value))
            if name == 'timeout':
                return cls('timeout', hold_s=float(value))
        except ValueError:
            pass
        raise ProxyRefused(f'{text!r} is not a fault (pass, 429[:s], delay:<ms>, timeout:<s>)')

    def label(self):
        if self.kind == 'rate_limit':
            return '429' if self.retry_after_s is None else f'429:{self.retry_after_s}'
        if self.kind == 'delay':
            return f'delay:{self.delay_ms}'
        if self.kind == 'timeout':
            return f'timeout:{self.hold_s:g}'
        return 'pass'


@dataclass(frozen=True)
class FaultPlan:
    faults: tuple  # Fault, cycled by arrival order

    def __post_init__(self):
        if not self.faults:
            raise ProxyRefused('a fault plan needs at least one fault')

    @classmethod
    def parse(cls, text):
        return cls(tuple(Fault.parse(item) for item in text.split(',') if item.strip()))

    @classmethod
    def clean(cls):
        return cls((Fault('pass'),))

    def at(self, index):
        return self.faults[index % len(self.faults)]

    def label(self):
        return ','.join(fault.label() for fault in self.faults)


@dataclass(frozen=True)
class ProxyEvent:
    index: int
    site: str | None
    fault: str
    held_ms: float
    upstream_ms: float | None  # None: not forwarded
    status: int | None  # None: the connection was closed without an answer

    def as_dict(self):
        return {'index': self.index, 'site': self.site, 'fault': self.fault, 'held_ms': round(self.held_ms, 3),
                'upstream_ms': None if self.upstream_ms is None else round(self.upstream_ms, 3),
                'status': self.status}


def check_upstream(url):
    """The upstream URL, or ProxyRefused: loopback http(s), or https on the provider host."""
    parsed = urllib.parse.urlsplit(url)
    host = parsed.hostname or ''
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ProxyRefused('the upstream carries no credential, query or fragment')
    if parsed.scheme == 'https' and host == PROVIDER_HOST and parsed.port is None:
        return url
    try:
        loopback = host == 'localhost' or ipaddress.ip_address(host).is_loopback
    except ValueError:
        loopback = False
    if parsed.scheme in ('http', 'https') and loopback:
        return url
    raise ProxyRefused(f'upstream {parsed.scheme}://{host} is neither loopback nor https://{PROVIDER_HOST}')


def is_provider(url):
    return urllib.parse.urlsplit(url).hostname == PROVIDER_HOST


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None  # the binary's client follows no redirect


def _opener():
    return urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirect())


class _Handler(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    server_version = 'memory-bench-jev-proxy'

    def log_message(self, *args):  # the default would print client addresses and paths
        return

    def _answer(self, status, body=b'', headers=()):
        self.send_response(status)
        for name, value in headers:
            self.send_header(name, value)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):  # noqa: N802 (http.server naming)
        proxy = self.server.proxy
        proxy.enter()
        try:
            self._serve(proxy)
        finally:
            proxy.leave()

    def _serve(self, proxy):
        length = int(self.headers.get('Content-Length') or 0)
        if length > MAX_BODY:
            self._answer(413)
            return
        body = self.rfile.read(length)
        site = self.headers.get(SITE_HEADER)
        index, fault = proxy.next_fault()
        started = time.perf_counter()
        if fault.kind == 'rate_limit':
            headers = [] if fault.retry_after_s is None else [('Retry-After', str(fault.retry_after_s))]
            self._answer(429, b'{}', headers)
            proxy.record(ProxyEvent(index, site, fault.label(), 0.0, None, 429))
            return
        if fault.kind == 'timeout':
            proxy.stopping.wait(fault.hold_s)
            self.close_connection = True
            proxy.record(ProxyEvent(index, site, fault.label(), (time.perf_counter() - started) * 1e3, None, None))
            return
        if fault.kind == 'delay':
            proxy.stopping.wait(fault.delay_ms / 1e3)
        held = (time.perf_counter() - started) * 1e3
        status, payload, upstream_ms = proxy.forward(body, self.headers.get('Authorization'))
        try:
            self._answer(status, payload, [('Content-Type', 'application/json')])
        except OSError:  # the client gave up meanwhile
            pass
        proxy.record(ProxyEvent(index, site, fault.label(), held, upstream_ms, status))


class FaultProxy:
    """`with FaultProxy(upstream, plan) as proxy:` then POST to `proxy.url`."""

    def __init__(self, upstream, plan, upstream_timeout_s=UPSTREAM_TIMEOUT_S, opener=None):
        self.upstream = check_upstream(upstream)
        self.plan = plan
        self.upstream_timeout_s = upstream_timeout_s
        self._opener = opener or _opener()
        self._lock = threading.Lock()
        self._next = 0
        self._events = []
        self.stopping = threading.Event()
        self._active = 0
        self._idle = threading.Condition(self._lock)
        self._server = None
        self._thread = None

    @property
    def url(self):
        host, port = self._server.server_address[:2]
        return f'http://{host}:{port}/'

    def __enter__(self):
        self._server = ThreadingHTTPServer(('127.0.0.1', 0), _Handler)
        self._server.daemon_threads = True
        self._server.proxy = self
        self._thread = threading.Thread(target=self._server.serve_forever, name='jev-fault-proxy', daemon=True)
        self._thread.start()
        return self

    def __exit__(self, *exc):
        self.stopping.set()  # releases held connections, so every request records its event
        with self._idle:
            self._idle.wait_for(lambda: self._active == 0, timeout=5)
        self._server.shutdown()
        self._server.server_close()
        self._thread.join(timeout=5)
        return False

    def enter(self):
        with self._lock:
            self._active += 1

    def leave(self):
        with self._idle:
            self._active -= 1
            self._idle.notify_all()

    def next_fault(self):
        with self._lock:
            index = self._next
            self._next += 1
        return index, self.plan.at(index)

    def record(self, event):
        with self._lock:
            self._events.append(event)

    def events(self):
        """Every request's event so far; complete once the proxy has exited."""
        with self._lock:
            return tuple(sorted(self._events, key=lambda e: e.index))

    def forward(self, body, authorization):
        """(status, body, upstream ms) of the upstream's answer; 502 when it cannot be reached."""
        headers = {'Content-Type': 'application/json'}
        if authorization:
            headers['Authorization'] = authorization
        request = urllib.request.Request(self.upstream, data=body, headers=headers, method='POST')
        started = time.perf_counter()
        try:
            with self._opener.open(request, timeout=self.upstream_timeout_s) as response:
                payload = response.read(MAX_BODY + 1)
                status = response.status
        except urllib.error.HTTPError as error:
            payload, status = error.read(MAX_BODY + 1), error.code
        except (urllib.error.URLError, OSError):
            payload, status = b'{}', 502
        return status, payload[:MAX_BODY], (time.perf_counter() - started) * 1e3


class LocalUpstream:
    """A loopback stand-in for the provider: answers every question `true` after `latency_ms`.

    It answers the provider's shape closely enough for the tail client (a JSON object
    with one answer per question key and `input_tokens`); it is not a Jev model."""

    def __init__(self, latency_ms=0):
        self.latency_ms = latency_ms
        self.requests = 0
        self.authorizations = set()
        self.site_headers = 0  # requests that still carried the proxy's site header (must stay 0)
        self._lock = threading.Lock()
        self._server = None
        self._thread = None

    @property
    def url(self):
        host, port = self._server.server_address[:2]
        return f'http://{host}:{port}/v1/systemone'

    def __enter__(self):
        upstream = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = 'HTTP/1.1'

            def log_message(self, *args):
                return

            def do_POST(self):  # noqa: N802
                import json
                body = self.rfile.read(int(self.headers.get('Content-Length') or 0))
                with upstream._lock:
                    upstream.requests += 1
                    upstream.authorizations.add(self.headers.get('Authorization'))
                    upstream.site_headers += SITE_HEADER in self.headers
                time.sleep(upstream.latency_ms / 1e3)
                try:
                    questions = json.loads(body).get('questions') or {}
                except ValueError:
                    questions = {}
                answer = json.dumps({'answers': {key: {'p_true': 0.9} for key in questions},
                                     'input_tokens': len(body) // 4}).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(answer)))
                self.end_headers()
                self.wfile.write(answer)

        self._server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self._server.daemon_threads = True
        self._thread = threading.Thread(target=self._server.serve_forever, name='jev-local-upstream', daemon=True)
        self._thread.start()
        return self

    def __exit__(self, *exc):
        self._server.shutdown()
        self._server.server_close()
        self._thread.join(timeout=5)
        return False
