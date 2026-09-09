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
        pages, restarts, collected = 0, 0, {}
        while True:
            pages += 1
            assert pages < 100
            for path, counts in page['projection']['sections'].items():
                skip = counts['core'] if path in collected else 0
                collected.setdefault(path, []).extend(section(page, path)[skip:])
            action = page['projection']['next_action']
            if action is None:
                break
            assert action['tool'] == tool
            args = action['arguments']
            for key in ('about', 'question', 'asked_as', 'axis', 'interval', 'role', 'intent'):
                assert args.get(key) == original.get(key), key
            assert args['dimensions']['selectors'] == original['dimensions']['selectors']
            if 'cursor' not in args.get('page', {}):
                assert page['projection']['core_text_shortened']
                collected.clear()
                restarts += 1
            page = client.call(tool, args)
        assert not page['projection']['core_text_shortened']
        for path, values in collected.items():
            assert values == section(whole, path), (tool, path)
        results[tool] = {'pages': pages, 'restarts': restarts,
                         'additional_native_calls': pages,  # full reference plus continuation calls
                         'all_sections_equal': True}
    return results
