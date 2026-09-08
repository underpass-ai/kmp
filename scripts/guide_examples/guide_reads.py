"""Read guide context and one lesson without expanding the whole cookbook."""
import json
from pathlib import Path


def prepare(client, root: Path, lesson: Path, mode: str):
    # IDs come from the shipped canonical request, never from rebuilding refs.
    # Sync has already converged these requests in this isolated store.
    requests = json.loads((root / 'plugins/kmp/guide/guide.requests.json').read_text())
    guide = next(request for request in requests if request['about'] == 'guide:kmp-agent')
    entries = guide['memory']['entries']
    lesson_entry = next(entry for entry in entries if entry['text'] == lesson.read_text())
    metrics = {'mode': mode, 'calls': 0, 'structured_bytes': 0, 'inspected_refs': []}

    def read(tool, arguments):
        result = client.call(tool, arguments)
        metrics['calls'] += 1
        metrics['structured_bytes'] += len(json.dumps(result, ensure_ascii=False, separators=(',', ':')).encode())
        return result

    args = {'about': guide['about'], 'budget': {'max_bytes': 20000, 'detail': 'full' if mode == 'full' else 'compact'}}
    for _ in range(100):
        result = read('kmp_wake', args)
        page = result['projection']['page']
        if not page['has_more']:
            break
        if page.get('returned') == 0:
            ceiling = args['budget']['max_bytes'] * 2
            if ceiling > 320000:
                raise ValueError('Guide page cannot advance within the replay ceiling')
            args = {**args, 'budget': {**args['budget'], 'max_bytes': ceiling}}
        args = {**args, 'page': {'cursor': page['next_cursor']}}
    else:
        raise ValueError('Guide recall did not complete within 100 pages')

    if mode == 'full':
        return metrics
    selected_ids = {'verb:write', 'advanced:relations', 'advanced:scope',
                    'advanced:lifecycle', 'advanced:summary', 'examples:index'}
    # The source entry IDs identify which canonical refs to copy, not how to
    # construct them. The full text must then come back through the live MCP.
    source = json.loads((root / 'plugins/kmp/guide/editorial.json').read_text())['abouts'][0]
    selected_texts = {entry['text'] for entry in source['entries'] if entry['id'] in selected_ids}
    selected = [entry for entry in entries if entry['text'] in selected_texts or entry['id'] == lesson_entry['id']]
    if len(selected) != len(selected_ids) + 1:
        raise ValueError('Directed guide prerequisites are missing or ambiguous')
    for entry in selected:
        args = {'about': guide['about'], 'ref': entry['id'], 'budget': {'max_bytes': 40000}}
        for _ in range(100):
            result = read('kmp_inspect', args)
            # Canonical ingest trims only the outer whitespace of entry text.
            # Compare to that declared boundary normalization; never replace
            # or normalize the actual proof returned by MCP.
            if result['object']['text'] != entry['text'].strip():
                raise ValueError('The synchronized guide body differs from the authored source')
            page = result['page']
            if not page['has_more']:
                break
            if page.get('returned') == 0:
                raise ValueError('Guide inspection cannot advance at its explicit byte ceiling')
            args = {**args, 'page': {'cursor': page['next_cursor']}}
        else:
            raise ValueError('Guide inspection exceeded 100 pages')
        metrics['inspected_refs'].append(entry['id'])
    return metrics
