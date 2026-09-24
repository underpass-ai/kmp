"""verify: integrity of every original, then pairing of every journey trace."""
from ..capture.manifest import verify_run
from ..capture.trace import read_trace
from ..domain.errors import CaptureIntegrityError


def journey_traces(verified):
    """Parse the traces the manifest names; the lesson list itself must be sound."""
    runs = verified.manifest.get('journeys')
    if not isinstance(runs, list) or not runs:
        raise CaptureIntegrityError('manifest lists no journeys')
    lessons = [run.get('lesson') if isinstance(run, dict) else None for run in runs]
    if len(set(lessons)) != len(lessons):
        raise CaptureIntegrityError('duplicate journey in manifest')
    traces = []
    for lesson in lessons:
        name = f'{lesson}.jsonl' if isinstance(lesson, str) else None
        if name not in verified.files:
            raise CaptureIntegrityError(f'journey {lesson!r} has no verified trace')
        traces.append(read_trace(lesson, verified.files[name]))
    return traces


def verify(root, max_file_bytes):
    verified = verify_run(root, max_file_bytes)
    traces = journey_traces(verified)
    failures = [failure for trace in traces for failure in trace.failures]
    return verified, traces, {
        'run': verified.root.name, 'manifest_sha256': verified.manifest_sha256,
        'binary_sha256': verified.manifest.get('binary_sha256'),
        'evidence_scope': verified.manifest.get('evidence_scope', 'historical_import'),
        'verified_files': len(verified.files), 'journeys': [t.summary() for t in traces],
        'failures': failures, 'ok': not failures}
