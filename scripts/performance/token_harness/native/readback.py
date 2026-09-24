"""Oracle read-back after the measured session closed (stage `oracle`, never measured).

It runs in its own MCP process on the same disposable store, so it cannot alter
the journey it checks. The receipt action is executed verbatim, then each
written memory is inspected by its canonical ref.
"""


def read_back(session, scenario, journey_results):
    final = journey_results[-1] if journey_results else {}
    calls = []
    action = (final.get('receipt') or {}).get('action')
    if action:
        calls.append((action['tool'], action['arguments']))
    for ref in (final.get('local_refs') or {}).values():
        calls.append(('kmp_inspect', {'about': scenario.about, 'ref': ref}))
    for tool, arguments in calls:
        session.call(tool, arguments)
    return len(calls)
