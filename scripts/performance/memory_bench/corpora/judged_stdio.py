"""Seeded kmp-mcp processes for the repository's judged corpora, over MCP stdio.

The Rust scorecards (`retrieval_kmp_scorecard`, `jev_kmp_scorecard`) drive
`KernelMcpServer::embedded` in process. The bench reproduces them the way an
operator runs KMP: the release binary, one MCP stdio process per store, on a
disposable token_harness store (`runtime.session.BenchProcess`). The store's
configuration files (`typesafe.json`, `rerank.json`, ...) are written into
the data directory before the binary starts, exactly where the scorecards
write them, and each one must be acknowledged by the binary's
`kmp_store_config` log line (SCHEMAS.md section 4), or the store is refused.

`StdioStores` is the adapter; the corpora take any object with the same
`open(label, store_files)` context manager, so their scoring is tested with
fakes and no binary.
"""
from contextlib import contextmanager
from dataclasses import dataclass, field
import itertools
import json
from pathlib import Path

from ..domain import cachekey
from ..domain.errors import BenchError
from ..domain.variant import StoreFile
from ..runtime.session import BenchProcess, ProcessConfig

TIMEOUT_SECONDS = 120.0


class JudgedCallFailed(BenchError):
    """A tool call of a judged corpus failed, or answered out of contract."""
    code = 'JUDGED_CALL_FAILED'


class StoreFileNotApplied(BenchError):
    """A configuration file written beside the store was not acknowledged by the binary."""
    code = 'VARIANT_NOT_APPLIED'


def inline_store_file(name, text):
    """A store file with the exact bytes the Rust scorecard writes (not re-serialized)."""
    return StoreFile(name, cachekey.sha256_hex(text.encode('utf-8')), inline=text)


def compact(value):
    """serde_json's `Value::to_string()` shape: no spaces, non-ASCII kept."""
    return json.dumps(value, ensure_ascii=False, separators=(',', ':'))


def structured(response, tool):
    """`result.structuredContent` of a tools/call response; a tool error is a failure."""
    if 'error' in response:
        raise JudgedCallFailed(f'{tool}: rpc error {compact(response["error"])[:400]}')
    result = response.get('result') or {}
    if result.get('isError') is True:
        raise JudgedCallFailed(f'{tool} failed: {compact(result)[:400]}')
    content = result.get('structuredContent')
    return content if content is not None else {}


def is_accepted(receipt):
    """`WriteReceipt::is_accepted` (crates/kmp-testkit/src/write_receipt.rs): committed or replayed."""
    if not isinstance(receipt, dict) or receipt.get('dry_run') is True:
        return False
    accepted = receipt.get('accepted')
    if isinstance(accepted, bool):
        return accepted and receipt.get('status') in ('committed', 'replayed')
    memory = receipt.get('memory')
    return isinstance(memory, dict) and memory.get('read_after_write_ready') is True


def require_accepted(receipt, where):
    if not is_accepted(receipt):
        status = receipt.get('status') if isinstance(receipt, dict) else None
        raise JudgedCallFailed(f'{where}: write not accepted (status {status!r})')
    return receipt


def commit_with_review(server, write, where):
    """`commit_judged_write`: accepted, or resolved through the review the write asked for.

    The judged fixture resolves its own review deliberately (#691); a generic
    helper doing this for an agent would be recording a judgement nobody made.
    """
    written = server.call('kmp_write_memory', write)
    if is_accepted(written):
        return written
    if not isinstance(written, dict) or written.get('status') != 'needs_review':
        require_accepted(written, where)
    actions = written.get('next_actions') or [{}]
    tool = actions[0].get('tool') if isinstance(actions[0], dict) else None
    if not isinstance(tool, str):
        raise JudgedCallFailed(f'{where}: the review returned no verb')
    return require_accepted(server.call(tool, actions[0].get('arguments')), f'{where} after review')


class JudgedServer:
    """One live process; `call` returns the tool's structured content."""

    def __init__(self, process, tap):
        self.process, self.tap = process, tap

    def call(self, tool, arguments):
        return structured(self.tap.call(tool, arguments), tool)


@dataclass(frozen=True)
class StdioStores:
    """Opens seeded stores on `binary`; traces of each store go to `out_dir/<label>`."""
    binary: Path
    work_dir: Path
    out_dir: Path
    env: dict = field(default_factory=dict)  # allowlisted (KMP_LEXICAL_BRIDGE, cassette, ...)
    timeout_seconds: float = TIMEOUT_SECONDS
    reports: list = field(default_factory=list)  # ProcessReport.as_dict() of every closed store

    @contextmanager
    def open(self, label, store_files=()):
        config = ProcessConfig(Path(self.binary), Path(self.work_dir), label, env=dict(self.env),
                               store_files=tuple(store_files), timeout_seconds=self.timeout_seconds,
                               probe_resources=False)
        out = Path(self.out_dir) / label
        out.mkdir(parents=True, exist_ok=True)
        process = BenchProcess(config, out, 0, itertools.count(1))
        closed = False
        try:
            with process.journey(None) as tap:
                yield JudgedServer(process, tap)
            report = process.close()
            closed = True
        finally:
            if not closed:
                process.close()
        self.reports.append({'label': label, **report.as_dict()})
        refused = [ack for ack in report.store_files if not ack.applied]
        if refused:
            raise StoreFileNotApplied(f'{label}: ' + '; '.join(f'{ack.name}: {ack.reason}'
                                                               for ack in refused))
