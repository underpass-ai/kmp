"""Load a scenario's fixture through the binary's own write tool (fixture_preparation).

`${id}` strings name a canonical ref returned by an earlier write of the same
fixture; nothing is guessed. A review round is followed verbatim, as a writer
would. Any refusal stops the capture: a scenario without its world is not run.
"""
from ..domain.errors import HarnessError

MAX_REVIEW_ROUNDS = 3


class FixtureError(HarnessError):
    code = 'FIXTURE_LOAD_FAILED'


def bind(value, refs):
    if isinstance(value, dict):
        return {key: bind(item, refs) for key, item in value.items()}
    if isinstance(value, list):
        return [bind(item, refs) for item in value]
    if isinstance(value, str) and value.startswith('${') and value.endswith('}'):
        name = value[2:-1]
        if name not in refs:
            raise FixtureError(f'unbound fixture ref {name}')
        return refs[name]
    return value


def _write(session, arguments):
    for _ in range(MAX_REVIEW_ROUNDS + 1):
        response = session.call('kmp_write_memory', arguments)
        result = response.get('result') or {}
        structured = result.get('structuredContent') or {}
        if 'error' in response or result.get('isError'):
            raise FixtureError(str(response.get('error') or structured.get('error'))[:500])
        if structured.get('status') != 'needs_review':
            if structured.get('accepted') is not True:
                raise FixtureError('fixture write not accepted: ' + str(structured.get('status')))
            return structured
        arguments = structured['next_actions'][0]['arguments']
    raise FixtureError('fixture write still under review')


def load_fixture(session, scenario):
    refs, calls = {}, 0
    for write in scenario.fixture:
        structured = _write(session, bind(write.arguments, refs))
        calls += 1
        refs.update(structured.get('local_refs') or {})
    return refs
