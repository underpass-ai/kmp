"""Reconstruct a context-bound inspection using only its returned short actions."""


def check(saved, client, authored):
    paths = (('evidence',), ('links', 'incoming'), ('links', 'outgoing'), ('raw',))

    def section(value, path):
        for key in path:
            value = value[key]
        return value

    first, page = saved['short_first'], saved['short_next']
    assert first['page']['returned'] == 0
    assert page['page']['returned'] > 0
    assert set(first['next_actions'][0]['arguments']) == {'continuation'}
    whole = client.call('kmp_inspect', dict(authored['short_first'], budget={'max_bytes': 100000}))
    assert not whole['page']['has_more']
    accumulated = {path: list(section(first, path)) for path in paths}
    pages = 1
    while True:
        pages += 1
        assert pages < 50
        assert page['object'] == whole['object']
        for path in paths:
            accumulated[path].extend(section(page, path))
        if not page['page']['has_more']:
            break
        action = page['next_actions'][0]
        assert action['tool'] == 'kmp_inspect'
        assert set(action['arguments']) == {'continuation'}
        previous_offset = page['page']['offset']
        page = client.call(action['tool'], action['arguments'])
        assert page['page']['offset'] > previous_offset
    assert not page['next_actions']
    for path in paths:
        assert accumulated[path] == section(whole, path)
    return {'pages': pages, 'complete_expansion_equal': True, 'returned_actions_only': True}
