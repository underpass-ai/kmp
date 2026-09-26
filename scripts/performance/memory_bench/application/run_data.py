"""One run directory (SCHEMAS.md section 3), verified and parsed for scoring.

The directory is a token_harness capture, so it is read through token_harness'
own `verify`: every file is checked against the manifest's length and SHA-256
before anything is interpreted, and the journey traces come back as the same
`JourneyTrace` objects `token_harness measure` counts. The bench records
(`run.json`, `calls.jsonl`, `journeys.jsonl`) are then validated against their
contracts. A response is read from the trace line its call record points at,
never copied.
"""
from dataclasses import dataclass
import json
from pathlib import Path

from ...token_harness.application.verify import verify
from ...token_harness.capture.manifest import DEFAULT_MAX_FILE_BYTES
from ...token_harness.capture.trace import read_trace
from ..domain import jsonl
from ..domain.errors import BenchError, RunRecordInvalid
from ..domain.run_manifest import RunManifest
from ..domain.run_record import CallRecord, JourneyRecord

RUN_FILE = 'run.json'
CALLS_FILE = 'calls.jsonl'
JOURNEYS_FILE = 'journeys.jsonl'
PROCESS_PREFIX = 'process-'


class RunUnreadable(BenchError):
    code = 'RUN_UNREADABLE'


def _text(files, name):
    if name not in files:
        raise RunUnreadable(f'run has no {name}')
    return files[name].decode('utf-8')


@dataclass(frozen=True)
class RunData:
    root: Path
    manifest: dict  # run.json, validated
    calls: tuple  # CallRecord, file order
    journeys: tuple  # JourneyRecord, file order
    files: dict  # original name -> verified bytes
    traces: tuple  # token_harness JourneyTrace of every measured journey
    verification: dict  # token_harness verify report

    @property
    def run_id(self):
        return self.manifest['run_id']

    @property
    def variant(self):
        return self.manifest['variant']['name']

    @property
    def private(self):
        return bool(self.manifest['private'])

    @property
    def max_bytes(self):
        values = self.manifest.get('max_bytes') or []
        return values[0] if values else None

    def comparability(self):
        return RunManifest(self.manifest).comparability()

    def calls_of(self, journey):
        return tuple(sorted((c for c in self.calls if c.journey == journey), key=lambda c: c.call_index))

    def response(self, call):
        """The JSON-RPC response object of a call, or None when it has none."""
        if call.response_path is None:
            return None
        trace, line = call.response_location()
        if trace not in self.files:
            raise RunUnreadable(f'{call.journey}: response trace {trace} is not in the run')
        lines = self.files[trace].decode('utf-8').split('\n')
        if line >= len(lines) or not lines[line].strip():
            raise RunUnreadable(f'{call.response_path}: no such trace line')
        event = json.loads(lines[line])
        response = event.get('response') if isinstance(event, dict) else None
        return response if isinstance(response, dict) else None

    def structured(self, call):
        """`result.structuredContent` of an ok call, else None."""
        if call.status != 'ok':
            return None
        response = self.response(call)
        result = response.get('result') if response else None
        structured = result.get('structuredContent') if isinstance(result, dict) else None
        return structured if isinstance(structured, dict) else None

    def process_traces(self):
        """Handshake traces (`process-<n>.jsonl`), parsed as token_harness traces."""
        names = sorted(n for n in self.files if n.startswith(PROCESS_PREFIX) and n.endswith('.jsonl'))
        return tuple(read_trace(name[:-len('.jsonl')], self.files[name]) for name in names)


def load_run(root, max_file_bytes=DEFAULT_MAX_FILE_BYTES):
    """Verify a run directory with token_harness and parse its bench records."""
    root = Path(root)
    verified, traces, verification = verify(root, max_file_bytes)
    files = verified.files
    try:
        manifest = json.loads(_text(files, RUN_FILE))
        RunManifest.from_dict(manifest, f'{root.name}/{RUN_FILE}')
        calls = tuple(CallRecord.from_dict(row, f'{CALLS_FILE}:{n}') for n, row in
                      jsonl.parse_lines(_text(files, CALLS_FILE), CALLS_FILE, RunRecordInvalid))
        journeys = tuple(JourneyRecord.from_dict(row, f'{JOURNEYS_FILE}:{n}') for n, row in
                         jsonl.parse_lines(_text(files, JOURNEYS_FILE), JOURNEYS_FILE, RunRecordInvalid))
    except ValueError as error:
        raise RunUnreadable(f'{root}: {error}') from error
    for record in calls + journeys:
        if record.run_id != manifest['run_id']:
            raise RunUnreadable(f'{record.journey}: record of run {record.run_id}, not {manifest["run_id"]}')
    return RunData(root, manifest, calls, journeys, files, tuple(traces), verification)
