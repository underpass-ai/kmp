"""Startup / guide / memory split, ported from surface_acceptance_metrics.journey().

The rules are kept identical so historical totals stay comparable, including
continuation provenance: a continuation first returned by a guide response keeps
later calls that follow it in the guide stage.
"""
from ..domain.exposure import Stage


def continuation_tokens(value):
    if isinstance(value, dict):
        for key, child in value.items():
            if key == 'continuation' and isinstance(child, str):
                yield child
            else:
                yield from continuation_tokens(child)
    elif isinstance(value, list):
        for child in value:
            yield from continuation_tokens(child)


class StageClassifier:
    def __init__(self):
        self.continuations = {}

    def classify(self, request):
        method = request.get('method')
        params = request.get('params') or {}
        name = params.get('name', method)
        arguments = params.get('arguments') or {}
        about = arguments.get('about')
        if method != 'tools/call':
            return Stage.STARTUP_CATALOGUE
        if name == 'kmp_guide' or isinstance(about, str) and about.startswith('guide:'):
            return Stage.GUIDE
        continuation = arguments.get('continuation')
        if isinstance(continuation, str):
            return self.continuations.get(continuation, Stage.MEMORY)
        return Stage.MEMORY

    def observe(self, response, stage):
        for token in continuation_tokens(response.get('result', {})):
            self.continuations[token] = stage
