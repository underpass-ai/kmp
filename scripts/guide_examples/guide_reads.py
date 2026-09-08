"""Read guide context and one lesson without expanding the whole cookbook."""
import json
import hashlib
import re
from pathlib import Path


def prepare(client, root: Path, lesson: Path, mode: str):
    # IDs come from the shipped canonical request, never from rebuilding refs.
    # Sync has already converged these requests in this isolated store.
    requests = json.loads((root / 'plugins/kmp/guide/guide.requests.json').read_text())
    guide = next(request for request in requests if request['about'] == 'guide:kmp-agent')
    entries = guide['memory']['entries']
    lesson_entry = next(entry for entry in entries if entry['text'] == lesson.read_text())
    metrics = {'mode': mode, 'calls': 0, 'structured_bytes': 0, 'markdown_bytes': 0, 'inspected_refs': [], 'reused_guidance': 0}

    topics = {'advanced:relations', 'advanced:scope'}
    if lesson.stem in {'decision-history', 'alias-ownership'}:
        topics.update({'advanced:lifecycle', 'advanced:summary'})
    elif lesson.stem in {'four-clocks', 'quantities'}:
        topics.add('advanced:lifecycle')

    native_call = client.call

    def read(tool, arguments):
        result = native_call(tool, arguments)
        metrics['calls'] += 1
        metrics['structured_bytes'] += len(json.dumps(result, ensure_ascii=False, separators=(',', ':')).encode())
        return result

    def inspect(entry, args):
        for _ in range(100):
            result = read('kmp_inspect', args)
            # Canonical ingest trims outer whitespace; never rewrite returned proof.
            if result['object']['text'] != entry['text'].strip():
                raise ValueError('The synchronized guide body differs from the authored source')
            page = result['page']
            if not page['has_more']:
                break
            if page.get('returned') == 0:
                # A long lesson can fill the stable object and leave no room
                # for its evidence. Use the exact native size, not a new query.
                required = page.get('required_bytes', 0)
                if required <= args['budget']['max_bytes']:
                    raise ValueError('Guide inspection cannot advance at its explicit byte ceiling')
                args = {**args, 'budget': {**args['budget'], 'max_bytes': required}}
            args = {**args, 'page': {'cursor': page['next_cursor']}}
        else:
            raise ValueError('Guide inspection exceeded 100 pages')
        metrics['inspected_refs'].append(entry['id'])

    if mode == 'markdown':
        markdown = (root / 'plugins/kmp/guide/AGENT.md').read_text()
        allowed = set(re.findall(r'^\| .* \| `(guide:kmp-agent:[^`]+)` \|$', markdown, re.M))
        by_ref = {entry['id']: entry for entry in entries}
        expected = {entry['id'] for entry in entries if entry['metadata'].get('guide_title') or entry['metadata'].get('example_title')}
        if allowed != expected:
            raise ValueError('Markdown index differs from canonical extended nodes')
        arguments = json.loads(re.findall(r'```json\n(.*?)\n```', markdown, re.S)[0])
        metrics['markdown_bytes'] = len(markdown.encode())
        metrics['markdown_sha256'] = hashlib.sha256(markdown.encode()).hexdigest()
        client.record({'preparation': 'read installed agent Markdown', 'text': markdown,
                       'markdown_sha256': metrics['markdown_sha256'], 'model_calls': 0})
        seen = set()

        def consult(ref):
            if ref not in allowed:
                raise ValueError('Guide reference is absent from the installed index')
            if ref in seen:
                metrics['reused_guidance'] += 1
                return
            inspect(by_ref[ref], {**arguments, 'ref': ref})
            seen.add(ref)

        source = json.loads((root / 'plugins/kmp/guide/editorial.json').read_text())['abouts'][0]
        topic_texts = {(root / 'plugins/kmp/guide' / entry['text_file']).read_text()
                       if entry.get('text_file') else entry['text']
                       for entry in source['entries'] if entry['id'] in topics}
        consult(lesson_entry['id'])
        # Read the topics this authored lesson uses, then reuse them.
        for ref in sorted(allowed):
            if by_ref[ref]['text'] in topic_texts:
                consult(ref)
        tool_refs = {entry['metadata']['tool_name']: entry['metadata']['guide_ref']
                     for entry in entries if entry['metadata'].get('tool_name')}

        def guided_call(tool, args, expect_error=None):
            consult(tool_refs[tool])
            if tool in {'kmp_write_memory', 'kmp_ingest'}:
                consult(tool_refs['kmp_goto'])  # Distinct clocks before writing.
            return native_call(tool, args, expect_error)

        # Native lesson calls ask for guidance on first use, reuse it on later
        # calls, and never replace actual memory reads with cached work evidence.
        client.call = guided_call
        return metrics

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
    selected_ids = {'verb:write', 'examples:index'} | topics
    # The source entry IDs identify which canonical refs to copy, not how to
    # construct them. The full text must then come back through the live MCP.
    source = json.loads((root / 'plugins/kmp/guide/editorial.json').read_text())['abouts'][0]
    selected_texts = {(root / 'plugins/kmp/guide' / entry['text_file']).read_text() if entry.get('text_file') else entry['text'] for entry in source['entries'] if entry['id'] in selected_ids}
    selected = [entry for entry in entries if entry['text'] in selected_texts or entry['id'] == lesson_entry['id']]
    if len(selected) != len(selected_ids) + 1:
        raise ValueError('Directed guide prerequisites are missing or ambiguous')
    for entry in selected:
        args = {'about': guide['about'], 'ref': entry['id'], 'budget': {'max_bytes': 40000}}
        inspect(entry, args)
    return metrics
