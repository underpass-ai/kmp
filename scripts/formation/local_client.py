"""Bounded loopback client; no credentials, remote hosts, redirects or retries."""
import ipaddress
import json
import time
import urllib.parse
import urllib.request
from contracts import digest

GENERATION = {'temperature': 0, 'seed': 0, 'max_tokens': 4096,
              'chat_template_kwargs': {'enable_thinking': False}}
PHASE_GENERATION = {'formation_review': {'chat_template_kwargs': {'enable_thinking': True}}}
GENERATION_PROFILE = {'default': GENERATION, 'phase_overrides': PHASE_GENERATION}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class LocalClient:
    def __init__(self, base_url, model, trace, max_prompt_tokens=8000):
        url = urllib.parse.urlsplit(base_url)
        if (url.scheme != 'http' or url.username or url.password or url.query or url.fragment
                or url.path.rstrip('/') != '/v1' or not ipaddress.ip_address(url.hostname).is_loopback):
            raise ValueError('a literal loopback HTTP /v1 endpoint is required')
        self.base = base_url.rstrip('/')
        self.model, self.trace, self.max_prompt_tokens = model, trace, max_prompt_tokens
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())

    def request(self, endpoint, body, phase):
        start = time.monotonic()
        record = {'phase': phase, 'endpoint': endpoint, 'request': body,
                  'request_sha256': digest(body), 'api_cost_usd': 0, 'retry': 0}
        try:
            request = urllib.request.Request(endpoint, data=json.dumps(body).encode(),
                                             headers={'Content-Type': 'application/json'})
            with self.opener.open(request, timeout=180) as response:
                data = response.read(2 * 1024 * 1024 + 1)
            if len(data) > 2 * 1024 * 1024:
                raise ValueError('model response exceeds 2 MiB')
            result = json.loads(data)
            record['response'] = result
            return result
        except Exception as error:
            record['error'] = str(error)
            raise
        finally:
            record['elapsed_seconds'] = time.monotonic() - start
            self.trace(record)

    def generate(self, messages, schema, phase):
        # Counting serialized messages overestimates their text slightly; reserve
        # another 1024 tokens for the template. Refuse overflow, never truncate.
        tokens = self.request(self.base.removesuffix('/v1') + '/tokenize',
            {'model': self.model, 'prompt': json.dumps(messages, ensure_ascii=False)}, phase + '_tokenize')
        if type(tokens.get('count')) is not int or tokens['count'] + 1024 > self.max_prompt_tokens:
            raise ValueError('formation prompt exceeds reserved input budget')
        result = self.request(self.base + '/chat/completions', {
            'model': self.model, 'messages': messages, **GENERATION, **PHASE_GENERATION.get(phase, {}),
            'response_format': {'type': 'json_schema', 'json_schema': {
                'name': phase, 'strict': True, 'schema': schema}}}, phase)
        choice = result['choices'][0]
        if choice['finish_reason'] != 'stop':
            raise ValueError('incomplete formation generation: ' + str(choice['finish_reason']))
        return json.loads(choice['message']['content'])
