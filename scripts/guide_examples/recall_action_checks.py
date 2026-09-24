"""Execute recall calls verbatim and reconstruct the selected proof."""
from copy import deepcopy


def section(value, name):
    for key in name.split('.'):
        value = value[key]
    return value


def check(saved, client, authored):
    results = {}
    for tool, name in [('kmp_wake', 'wake_action_start'), ('kmp_ask', 'ask_action_start')]:
        original = authored[name]
        whole_args = deepcopy(original)
        whole_args['budget']['max_bytes'] = 100000
        whole = client.call(tool, whole_args)
        assert whole['projection']['next_action'] is None
        page = saved[name]
        pages, restarts, collected, eligible = 0, 0, {}, {}
        while True:
            pages += 1
            assert pages < 100
            # A continuation carries only new items (projection.core_reused);
            # the core arrived with the first page and is not repeated.
            reused = page['projection'].get('core_reused') is True
            assert reused == bool(collected), (tool, 'continuations reuse the core')
            for path, counts in page['projection']['sections'].items():
                try:
                    values = section(page, path)
                except KeyError:
                    values = []
                collected.setdefault(path, []).extend(values)
                # Lean counters: zeros are omitted and eligible is derived from
                # the page that carries the core (core + returned + remaining).
                if not reused:
                    eligible[path] = len(values) + counts.get('remaining', 0)
                assert counts.get('remaining', 0) == eligible[path] - len(collected[path]), (tool, path)
            accounting = page['projection']['page']
            assert sum(c.get('remaining', 0) for c in page['projection']['sections'].values()) == (
                accounting['total'] - accounting['offset'] - accounting['returned'])
            action = page['projection']['next_action']
            if action is None:
                break
            assert action['tool'] == tool
            args = action['arguments']
            # A continuation is a handle the server resolves to the complete
            # call; only a restart restates the request.
            if list(args) != ['continuation']:
                for key in ('about', 'question', 'asked_as', 'axis', 'interval', 'role', 'intent'):
                    assert args.get(key) == original.get(key), key
                assert args['dimensions']['selectors'] == original['dimensions']['selectors']
                assert 'cursor' not in args.get('page', {})
                assert page['projection']['core_text_shortened']
                collected.clear()
                eligible.clear()
                restarts += 1
            page = client.call(tool, args)
        assert not page['projection']['core_text_shortened']
        for path, values in collected.items():
            assert values == section(whole, path), (tool, path)
        results[tool] = {'pages': pages, 'restarts': restarts,
                         'additional_native_calls': pages,  # full reference plus continuation calls
                         'all_sections_equal': True, 'remaining_counts_exact': True}
    return results
